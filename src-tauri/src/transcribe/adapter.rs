//! File transcription through ordinary Gemini generateContent models.
//! Live Translate has its own WebSocket pipeline in the Live epic.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::error::{AppError, Code};
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway, JobObserver};
use crate::media::{serialize_transcribe_request, Chunk};
use crate::settings::TranscribeLanguage;

use super::parser::{parse_general_response, ChunkTranscript};

const PROMPT: &str = "Transcribe this audio accurately, including code-switching between Vietnamese and Japanese. Detect each spoken language automatically; do not translate. Return ONLY a JSON array, no markdown or prose. Each segment has start and end in MM:SS relative to THIS audio chunk and text. Target segments of 5 to 15 seconds. Timestamps must match the audio. Return [] only if there is no speech.";

/// One sentence appended after [`PROMPT`] when `language != Auto` (spec
/// Design Notes: "Câu chỉ định ngôn ngữ đặt sau `PROMPT` cố định để `auto`
/// không đổi byte nào"). Never translates -- only names the primary spoken
/// language for the model, mirroring the design note's own example.
fn language_sentence(language: TranscribeLanguage) -> Option<&'static str> {
    match language {
        TranscribeLanguage::Auto => None,
        TranscribeLanguage::Ja => Some(
            "The primary spoken language is Japanese; transcribe in the spoken language without translating.",
        ),
        TranscribeLanguage::Vi => Some(
            "The primary spoken language is Vietnamese; transcribe in the spoken language without translating.",
        ),
        TranscribeLanguage::En => Some(
            "The primary spoken language is English; transcribe in the spoken language without translating.",
        ),
    }
}

/// `Auto` returns [`PROMPT`] byte-for-byte (spec Always: "`auto` giữ nguyên
/// prompt và request JSON hiện có byte-for-byte"); any other language
/// appends its [`language_sentence`].
fn prompt_text(language: TranscribeLanguage) -> String {
    match language_sentence(language) {
        Some(sentence) => format!("{PROMPT} {sentence}"),
        None => PROMPT.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThinkingProfile {
    BudgetZero,
    BudgetDynamic,
    LevelMinimal,
    LevelLow,
    ModelDefault,
}

/// Only exact model IDs receive model-specific thinking controls. Versionless
/// aliases and user-entered names use the model's own default configuration;
/// their name is never parsed to guess a major version.
fn thinking_profile(model: &str) -> ThinkingProfile {
    match model {
        "gemini-2.5-flash" | "gemini-2.5-flash-lite" => ThinkingProfile::BudgetZero,
        "gemini-2.5-pro" => ThinkingProfile::BudgetDynamic,
        "gemini-3.5-flash" | "gemini-3.5-flash-lite" => ThinkingProfile::LevelMinimal,
        "gemini-3.8-flash" => ThinkingProfile::LevelLow,
        _ => ThinkingProfile::ModelDefault,
    }
}

/// Model listing returns `models/<id>` while Settings may hold a bare ID.
fn model_id(model: &str) -> Result<&str, AppError> {
    let id = model.strip_prefix("models/").unwrap_or(model);
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::new(Code::Model, "Gemini model ID is invalid"));
    }
    let lowercase_id = id.to_ascii_lowercase();
    if lowercase_id.contains("transcribe") || lowercase_id.contains("live-translate") {
        return Err(AppError::new(
            Code::Model,
            "Select a general Gemini model for file transcription",
        ));
    }
    Ok(id)
}

fn generation_config(model: &str) -> Value {
    let mut config = json!({
        "responseMimeType": "application/json",
        "responseSchema": {
            "type": "ARRAY",
            "items": {
                "type": "OBJECT",
                "properties": {"start": {"type": "STRING"}, "end": {"type": "STRING"}, "text": {"type": "STRING"}},
                "required": ["start", "end", "text"]
            }
        }
    });
    let thinking = match thinking_profile(model) {
        ThinkingProfile::BudgetZero => Some(json!({"thinkingBudget": 0})),
        ThinkingProfile::BudgetDynamic => Some(json!({"thinkingBudget": -1})),
        ThinkingProfile::LevelMinimal => Some(json!({"thinkingLevel": "minimal"})),
        ThinkingProfile::LevelLow => Some(json!({"thinkingLevel": "low"})),
        ThinkingProfile::ModelDefault => None,
    };
    if let Some(thinking) = thinking {
        config["thinkingConfig"] = thinking;
    }
    config
}

pub fn build_general_request(
    model: &str,
    chunk: &Chunk,
    language: TranscribeLanguage,
) -> Result<(String, String), AppError> {
    let model = model_id(model)?;
    let body = json!({
        "contents": [{"role": "user", "parts": [
            {"text": prompt_text(language)},
            {"inline_data": {"mime_type": "audio/flac", "data": chunk.flac_base64}}
        ]}],
        "generationConfig": generation_config(model)
    });
    let serialized = serialize_transcribe_request(&body)?;
    Ok((
        format!("/v1beta/models/{model}:generateContent"),
        serialized,
    ))
}

#[derive(Debug)]
pub struct TranscribeFailure {
    pub error: AppError,
    pub retryable: bool,
}

impl From<AppError> for TranscribeFailure {
    fn from(error: AppError) -> Self {
        let retryable = matches!(error.code, Code::Network | Code::Quota);
        Self { error, retryable }
    }
}

pub async fn transcribe_chunk(
    gateway: &GeminiGateway,
    model: &str,
    chunk: &Chunk,
    language: TranscribeLanguage,
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
) -> Result<ChunkTranscript, TranscribeFailure> {
    transcribe_chunk_observed(gateway, model, chunk, language, consent, cancellation, None).await
}

/// Same contract as [`transcribe_chunk`], plus an optional progress observer
/// threaded through to [`GeminiGateway::post_job_observed`] (story 2.4
/// Tasks: "chuyển observer xuyên qua `transcribe_chunk`"). `observer: None`
/// behaves exactly like [`transcribe_chunk`].
#[allow(clippy::too_many_arguments)]
pub async fn transcribe_chunk_observed(
    gateway: &GeminiGateway,
    model: &str,
    chunk: &Chunk,
    language: TranscribeLanguage,
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
    observer: Option<Arc<dyn JobObserver>>,
) -> Result<ChunkTranscript, TranscribeFailure> {
    let (path, body) = build_general_request(model, chunk, language)?;
    let response = gateway
        .post_job_observed(&path, body, consent, cancellation, observer)
        .await?;
    parse_general_response(&response.body, chunk).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk() -> Chunk {
        Chunk {
            start_sample: 0,
            start_ms: 0,
            duration_ms: 1_000,
            sample_count: 16_000,
            mime_type: "audio/flac",
            flac_base64: "AAAA".into(),
        }
    }

    #[test]
    fn ordinary_model_request_has_inline_flac_json_schema_and_mixed_language_prompt() {
        let (path, body) = build_general_request(
            "models/gemini-3.8-flash",
            &chunk(),
            TranscribeLanguage::Auto,
        )
        .unwrap();
        assert_eq!(path, "/v1beta/models/gemini-3.8-flash:generateContent");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value["contents"][0]["parts"][1],
            json!({"inline_data":{"mime_type":"audio/flac","data":"AAAA"}})
        );
        assert!(value["contents"][0]["parts"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Vietnamese and Japanese"));
        assert_eq!(
            value["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            value["generationConfig"]["responseSchema"]["items"]["required"],
            json!(["start", "end", "text"])
        );
        assert_eq!(
            value["generationConfig"]["thinkingConfig"],
            json!({"thinkingLevel":"low"})
        );
    }

    #[test]
    fn default_alias_and_custom_name_use_generate_content_without_version_guessing() {
        for model in ["gemini-flash-lite-latest", "my-custom-model"] {
            let (path, body) =
                build_general_request(model, &chunk(), TranscribeLanguage::Auto).unwrap();
            assert_eq!(path, format!("/v1beta/models/{model}:generateContent"));
            let value: Value = serde_json::from_str(&body).unwrap();
            assert!(value["generationConfig"].get("thinkingConfig").is_none());
        }
        assert_eq!(
            thinking_profile("gemini-2.5-flash-lite"),
            ThinkingProfile::BudgetZero
        );
        assert!(
            build_general_request("gemini-3.5-transcribe", &chunk(), TranscribeLanguage::Auto)
                .is_err()
        );
        assert!(build_general_request(
            "gemini-3.5-live-translate-preview",
            &chunk(),
            TranscribeLanguage::Auto
        )
        .is_err());
        assert!(
            build_general_request("models/../oops", &chunk(), TranscribeLanguage::Auto).is_err()
        );
    }

    #[test]
    fn model_guard_rejects_mixed_case_transcribe_and_live_translate_ids() {
        // Spec I/O Matrix "Mixed-case model": `Gemini-2.5-Transcribe` must be
        // rejected the same as its lowercase form (`Code::Model`).
        for model in [
            "Gemini-2.5-Transcribe",
            "GEMINI-3.5-TRANSCRIBE",
            "models/Gemini-3.5-Live-Translate-Preview",
        ] {
            let error =
                build_general_request(model, &chunk(), TranscribeLanguage::Auto).unwrap_err();
            assert_eq!(error.code, Code::Model);
        }
    }

    #[test]
    fn auto_language_leaves_the_prompt_text_byte_for_byte_unchanged() {
        // Spec Always: "`auto` giữ nguyên prompt và request JSON hiện có
        // byte-for-byte".
        let (_, body) = build_general_request(
            "gemini-flash-lite-latest",
            &chunk(),
            TranscribeLanguage::Auto,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["contents"][0]["parts"][0]["text"], json!(PROMPT));
    }

    #[test]
    fn a_specific_language_appends_one_sentence_after_the_fixed_prompt() {
        // Spec Always: "`ja|vi|en` chỉ thêm một câu chỉ định ngôn ngữ chính,
        // không dịch." / Design Notes: câu đặt sau `PROMPT` cố định.
        let (_, body) =
            build_general_request("gemini-flash-lite-latest", &chunk(), TranscribeLanguage::Ja)
                .unwrap();
        let value: Value = serde_json::from_str(&body).unwrap();
        let text = value["contents"][0]["parts"][0]["text"].as_str().unwrap();
        assert_eq!(
            text,
            format!(
                "{PROMPT} The primary spoken language is Japanese; transcribe in the spoken language without translating."
            )
        );
        assert!(text.starts_with(PROMPT), "PROMPT phải giữ nguyên ở đầu");
        assert!(
            !text.to_lowercase().contains("translate this"),
            "không phải câu yêu cầu dịch"
        );

        for (language, name) in [
            (TranscribeLanguage::Vi, "Vietnamese"),
            (TranscribeLanguage::En, "English"),
        ] {
            let (_, body) =
                build_general_request("gemini-flash-lite-latest", &chunk(), language).unwrap();
            let value: Value = serde_json::from_str(&body).unwrap();
            let text = value["contents"][0]["parts"][0]["text"].as_str().unwrap();
            assert!(text.contains(name));
            assert!(text.starts_with(PROMPT));
        }
    }
}
