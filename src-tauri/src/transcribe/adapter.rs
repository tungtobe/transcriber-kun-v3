//! Family-specific request construction behind the single Gemini gateway.
//! Only exact, documented model IDs get a profile. Alias and free-text model
//! names never inherit a thinking configuration guessed from their spelling.

use serde_json::{json, Value};

use crate::core::error::{AppError, Code};
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway};
use crate::media::{serialize_transcribe_request, Chunk};

use super::parser::{parse_general_response, parse_interaction_response, ChunkTranscript};

const PROMPT: &str = "Transcribe the spoken audio into a JSON array of segments. Each segment must have start and end as MM:SS relative to this audio chunk and text. Target segments of 5 to 15 seconds. Return [] only when the audio contains no speech.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProfile {
    General25Flash,
    General25FlashLite,
    General25Pro,
    General35FlashLite,
    General35Flash,
    General38Flash,
    /// Not returned by `verified_profile` until S2 proves inline support.
    SpecializedInline,
}

pub fn verified_profile(model: &str) -> Result<ModelProfile, AppError> {
    match canonical_model_id(model) {
        "gemini-2.5-flash" => Ok(ModelProfile::General25Flash),
        "gemini-2.5-flash-lite" => Ok(ModelProfile::General25FlashLite),
        "gemini-2.5-pro" => Ok(ModelProfile::General25Pro),
        "gemini-3.5-flash-lite" => Ok(ModelProfile::General35FlashLite),
        "gemini-3.5-flash" => Ok(ModelProfile::General35Flash),
        "gemini-3.8-flash" => Ok(ModelProfile::General38Flash),
        "gemini-3.5-transcribe" => Err(AppError::new(
            Code::Blocked,
            "S2 inline audio is not verified for this transcribe model",
        )),
        _ => Err(AppError::new(
            Code::Model,
            "This Gemini model has no verified transcribe adapter profile",
        )),
    }
}

/// Model listing returns `models/<id>` while Settings may hold a bare ID.
/// Both forms identify the same model and keep the user's selection intact.
fn canonical_model_id(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

fn generation_config(profile: ModelProfile) -> Value {
    let thinking = match profile {
        ModelProfile::General25Flash | ModelProfile::General25FlashLite => {
            json!({"thinkingBudget": 0})
        }
        ModelProfile::General25Pro => json!({"thinkingBudget": -1}),
        ModelProfile::General35FlashLite | ModelProfile::General35Flash => {
            json!({"thinkingLevel": "minimal"})
        }
        ModelProfile::General38Flash => json!({"thinkingLevel": "low"}),
        ModelProfile::SpecializedInline => unreachable!(),
    };
    json!({
        "responseMimeType": "application/json",
        "responseSchema": {
            "type": "ARRAY",
            "items": {
                "type": "OBJECT",
                "properties": {"start": {"type": "STRING"}, "end": {"type": "STRING"}, "text": {"type": "STRING"}},
                "required": ["start", "end", "text"]
            }
        },
        "thinkingConfig": thinking
    })
}

pub fn build_general_request(
    model: &str,
    profile: ModelProfile,
    chunk: &Chunk,
) -> Result<(String, String), AppError> {
    if profile == ModelProfile::SpecializedInline {
        return Err(AppError::new(Code::Model, "Wrong Gemini adapter family"));
    }
    // A profile is bound to its exact verified model; callers cannot select
    // one model's thinking settings for a different or user-entered name.
    if verified_profile(model)? != profile {
        return Err(AppError::new(Code::Model, "Gemini model profile mismatch"));
    }
    let model = canonical_model_id(model);
    let body = json!({
        "contents": [{"role":"user", "parts": [
            {"text": PROMPT},
            {"inline_data": {"mime_type": "audio/flac", "data": chunk.flac_base64}}
        ]}],
        "generationConfig": generation_config(profile)
    });
    let serialized = serialize_transcribe_request(&body)?;
    Ok((
        format!("/v1beta/models/{model}:generateContent"),
        serialized,
    ))
}

pub fn build_specialized_request(
    model: &str,
    chunk: &Chunk,
    language_codes: &[String],
) -> Result<(String, String), AppError> {
    let model = canonical_model_id(model);
    if model != "gemini-3.5-transcribe" {
        return Err(AppError::new(
            Code::Model,
            "Unknown specialized transcribe model",
        ));
    }
    let body = json!({
        "model": model,
        "input": [{"type": "audio", "data": chunk.flac_base64, "mime_type": "audio/flac"}],
        "generation_config": {"transcription_config": {
            "mode": {"type": "verbatim", "timestamp_granularities": ["word"], "diarization_mode": "speaker"},
            "language_codes": language_codes
        }}
    });
    let serialized = serialize_transcribe_request(&body)?;
    Ok(("/v1beta/interactions".to_string(), serialized))
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
    let profile = verified_profile(model)?;
    // The specialized branch is dormant until an exact-model S2 probe makes
    // `verified_profile` return SpecializedInline. It is already wired to the
    // Interactions transport and parser, so enabling it cannot silently fall
    // through to generateContent.
    let (path, body) = match profile {
        ModelProfile::SpecializedInline => build_specialized_request(model, chunk, &[])?,
        _ => build_general_request(model, profile, chunk)?,
    };
    let response = gateway.post_job(&path, body, consent, cancellation).await?;
    match profile {
        ModelProfile::SpecializedInline => parse_interaction_response(&response.body, chunk),
        _ => parse_general_response(&response.body, chunk),
    }
    .map_err(Into::into)
}

/// The specialized builder is deliberately available for exact-shape tests,
/// while the public execution path remains blocked at S2 until inline audio
/// is proven for this exact model. No Files API fallback is permitted.
pub fn specialized_inline_gate() -> Result<(), AppError> {
    Err(AppError::new(
        Code::Blocked,
        "S2 inline audio is not verified for this transcribe model",
    ))
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
    fn general_request_has_exact_inline_schema_and_thinking_profiles() {
        let (path, body) = build_general_request(
            "gemini-2.5-flash-lite",
            ModelProfile::General25FlashLite,
            &chunk(),
        )
        .unwrap();
        assert_eq!(path, "/v1beta/models/gemini-2.5-flash-lite:generateContent");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value["contents"][0]["parts"][1],
            json!({"inline_data":{"mime_type":"audio/flac","data":"AAAA"}})
        );
        assert_eq!(
            value["generationConfig"]["thinkingConfig"],
            json!({"thinkingBudget":0})
        );
        assert_eq!(
            value["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            value["generationConfig"]["responseSchema"]["items"]["required"],
            json!(["start", "end", "text"])
        );
        let (_, body3) =
            build_general_request("gemini-3.8-flash", ModelProfile::General38Flash, &chunk())
                .unwrap();
        let value3: Value = serde_json::from_str(&body3).unwrap();
        assert_eq!(
            value3["generationConfig"]["thinkingConfig"],
            json!({"thinkingLevel":"low"})
        );
    }

    #[test]
    fn specialized_shape_is_exact_but_execution_is_gated() {
        let (path, body) =
            build_specialized_request("gemini-3.5-transcribe", &chunk(), &["ja-JP".into()])
                .unwrap();
        assert_eq!(path, "/v1beta/interactions");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value["input"][0],
            json!({"type":"audio","data":"AAAA","mime_type":"audio/flac"})
        );
        assert_eq!(
            value["generation_config"]["transcription_config"]["mode"],
            json!({"type":"verbatim","timestamp_granularities":["word"],"diarization_mode":"speaker"})
        );
        assert_eq!(
            value["generation_config"]["transcription_config"]["language_codes"],
            json!(["ja-JP"])
        );
        assert!(verified_profile("gemini-3.5-transcribe").is_err());
        assert!(specialized_inline_gate().is_err());
        assert!(verified_profile("gemini-flash-lite-latest").is_err());
        assert_eq!(
            verified_profile("models/gemini-2.5-flash-lite").unwrap(),
            ModelProfile::General25FlashLite
        );
        let (listed_path, _) = build_general_request(
            "models/gemini-2.5-flash-lite",
            ModelProfile::General25FlashLite,
            &chunk(),
        )
        .unwrap();
        assert_eq!(
            listed_path,
            "/v1beta/models/gemini-2.5-flash-lite:generateContent"
        );
    }
}
