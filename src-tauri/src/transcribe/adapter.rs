//! File transcription through ordinary Gemini generateContent models.
//! Live Translate has its own WebSocket pipeline in the Live epic.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::error::{AppError, Code};
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway, JobObserver};
use crate::media::{serialize_transcribe_request, Chunk};

use super::parser::{parse_general_response, ChunkTranscript};

const PROMPT: &str = "Transcribe this audio accurately, including code-switching between Vietnamese and Japanese. Detect each spoken language automatically; do not translate. Return ONLY a JSON array, no markdown or prose. Each segment has start and end in MM:SS relative to THIS audio chunk and text. Target segments of 5 to 15 seconds. Timestamps must match the audio. Return [] only if there is no speech.";

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
    if id.contains("transcribe") || id.contains("live-translate") {
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

pub fn build_general_request(model: &str, chunk: &Chunk) -> Result<(String, String), AppError> {
    let model = model_id(model)?;
    let body = json!({
        "contents": [{"role": "user", "parts": [
            {"text": PROMPT},
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
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
) -> Result<ChunkTranscript, TranscribeFailure> {
    transcribe_chunk_observed(gateway, model, chunk, consent, cancellation, None).await
}

/// Same contract as [`transcribe_chunk`], plus an optional progress observer
/// threaded through to [`GeminiGateway::post_job_observed`] (story 2.4
/// Tasks: "chuyển observer xuyên qua `transcribe_chunk`"). `observer: None`
/// behaves exactly like [`transcribe_chunk`].
pub async fn transcribe_chunk_observed(
    gateway: &GeminiGateway,
    model: &str,
    chunk: &Chunk,
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
    observer: Option<Arc<dyn JobObserver>>,
) -> Result<ChunkTranscript, TranscribeFailure> {
    let (path, body) = build_general_request(model, chunk)?;
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
        let (path, body) = build_general_request("models/gemini-3.8-flash", &chunk()).unwrap();
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
            let (path, body) = build_general_request(model, &chunk()).unwrap();
            assert_eq!(path, format!("/v1beta/models/{model}:generateContent"));
            let value: Value = serde_json::from_str(&body).unwrap();
            assert!(value["generationConfig"].get("thinkingConfig").is_none());
        }
        assert_eq!(
            thinking_profile("gemini-2.5-flash-lite"),
            ThinkingProfile::BudgetZero
        );
        assert!(build_general_request("gemini-3.5-transcribe", &chunk()).is_err());
        assert!(build_general_request("gemini-3.5-live-translate-preview", &chunk()).is_err());
        assert!(build_general_request("models/../oops", &chunk()).is_err());
    }
}
