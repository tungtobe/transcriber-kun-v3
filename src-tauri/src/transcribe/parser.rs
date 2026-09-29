//! Response parser for ordinary Gemini generateContent file transcription.
use std::collections::HashSet;

use serde_json::Value;

use crate::core::error::{AppError, Code};
use crate::media::Chunk;

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MissingRange {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkTranscript {
    pub segments: Vec<Segment>,
    pub unresolved: Vec<MissingRange>,
    pub confirmed_silence: bool,
}

fn shape(reason: &'static str) -> AppError {
    tracing::warn!(reason, "Gemini transcription response rejected as Shape");
    AppError::new(
        Code::Shape,
        "Gemini transcription response is incomplete or invalid",
    )
}

pub fn parse_general_response(body: &str, chunk: &Chunk) -> Result<ChunkTranscript, AppError> {
    let response: Value =
        serde_json::from_str(body).map_err(|_| shape("body is not valid JSON"))?;
    if response.pointer("/promptFeedback/blockReason").is_some() {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini blocked the transcription",
        ));
    }
    let candidate = response
        .pointer("/candidates/0")
        .ok_or_else(|| shape("no candidates[0] in response"))?;
    if candidate.get("finishReason").and_then(Value::as_str) == Some("SAFETY") {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini blocked the transcription",
        ));
    }
    let text = candidate
        .pointer("/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| shape("candidate has no content.parts[0].text"))?;
    let finish_reason = candidate.get("finishReason").and_then(Value::as_str);
    tracing::debug!(
        finish_reason,
        text_chars = text.chars().count(),
        chunk_start_ms = chunk.start_ms,
        chunk_duration_ms = chunk.duration_ms,
        "Gemini transcription response received"
    );
    let mut result = parse_general_text(text, chunk)?;
    if candidate.get("finishReason").and_then(Value::as_str) != Some("STOP") {
        if result.confirmed_silence {
            return Err(shape("non-STOP finishReason with empty transcript"));
        }
        if result.unresolved.is_empty() {
            result.unresolved.push(MissingRange {
                start: chunk.start_ms as f64 / 1000.0,
                end: (chunk.start_ms + chunk.duration_ms) as f64 / 1000.0,
            });
        }
    }
    Ok(result)
}

/// Strips a leading ```` ``` ```` or ```` ```json ```` code fence
/// case-insensitively, allowing optional whitespace between the backticks
/// and the `json` label (spec Always: "parser strips a leading code fence
/// case-insensitively, with optional whitespace between ``` and json").
fn strip_leading_fence(text: &str) -> &str {
    let Some(after_backticks) = text.strip_prefix("```") else {
        return text;
    };
    let after_ws = after_backticks.trim_start();
    let consumed_ws = after_backticks.len() - after_ws.len();
    let is_json_label =
        after_ws.len() >= 4 && after_ws.as_bytes()[..4].eq_ignore_ascii_case(b"json");
    if is_json_label {
        &after_backticks[consumed_ws + 4..]
    } else {
        after_backticks
    }
}

pub fn parse_general_text(text: &str, chunk: &Chunk) -> Result<ChunkTranscript, AppError> {
    let clean = strip_leading_fence(text.trim())
        .trim_end_matches("```")
        .trim();
    if clean.is_empty() {
        return Err(shape("response text is empty"));
    }
    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        let items = value
            .as_array()
            .ok_or_else(|| shape("response JSON is not an array"))?;
        if items.is_empty() {
            return Ok(ChunkTranscript {
                segments: vec![],
                unresolved: vec![],
                confirmed_silence: true,
            });
        }
        let parsed: Vec<_> = items.iter().filter_map(parse_general_item).collect();
        let incomplete = items.len() != parsed.len();
        return normalize(parsed, chunk, incomplete);
    }
    let mut parsed = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in clean.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(from) = start.take() {
                        if let Ok(item) = serde_json::from_str::<Value>(&clean[from..index + 1]) {
                            if let Some(segment) = parse_general_item(&item) {
                                parsed.push(segment);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    normalize(parsed, chunk, true)
}

fn parse_general_item(item: &Value) -> Option<Segment> {
    let start = parse_clock(item.get("start")?.as_str()?)?;
    let end = parse_clock(item.get("end")?.as_str()?)?;
    let text = item.get("text")?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    Some(Segment {
        start,
        end,
        text: text.to_owned(),
        speaker: None,
    })
}

fn parse_clock(value: &str) -> Option<f64> {
    let (minutes, seconds) = value.split_once(':')?;
    if minutes.is_empty() || !minutes.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let minutes = minutes.parse::<u64>().ok()?;
    let seconds = seconds.parse::<f64>().ok()?;
    if !seconds.is_finite() || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some(minutes as f64 * 60.0 + seconds)
}

fn normalize(
    raw: Vec<Segment>,
    chunk: &Chunk,
    mut incomplete: bool,
) -> Result<ChunkTranscript, AppError> {
    let duration = chunk.duration_ms as f64 / 1000.0;
    let offset = chunk.start_ms as f64 / 1000.0;
    let mut seen = HashSet::new();
    let mut valid = Vec::new();
    for item in raw {
        if !item.start.is_finite()
            || !item.end.is_finite()
            || item.start < 0.0
            || item.end <= item.start
            || item.end > duration
            || item.text.trim().is_empty()
        {
            incomplete = true;
            continue;
        }
        let key = (
            item.start.to_bits(),
            item.end.to_bits(),
            item.text.clone(),
            item.speaker.clone(),
        );
        if !seen.insert(key) {
            incomplete = true;
            continue;
        }
        valid.push(item);
    }
    valid.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
    let mut segments: Vec<Segment> = Vec::new();
    for mut item in valid {
        let previous_end = segments
            .last()
            .map(|segment| segment.end - offset)
            .unwrap_or(0.0);
        if item.start < previous_end {
            item.start = previous_end;
            incomplete = true;
        }
        if item.end <= item.start {
            incomplete = true;
            continue;
        }
        item.start += offset;
        item.end += offset;
        segments.push(item);
    }
    if segments.is_empty() {
        return Err(shape("no valid segments after normalization"));
    }
    let mut unresolved = Vec::new();
    if incomplete {
        let mut cursor = offset;
        for segment in &segments {
            if segment.start > cursor {
                unresolved.push(MissingRange {
                    start: cursor,
                    end: segment.start,
                });
            }
            cursor = cursor.max(segment.end);
        }
        if cursor < offset + duration {
            unresolved.push(MissingRange {
                start: cursor,
                end: offset + duration,
            });
        }
        if unresolved.is_empty() {
            unresolved.push(MissingRange {
                start: offset,
                end: offset + duration,
            });
        }
    }
    Ok(ChunkTranscript {
        segments,
        unresolved,
        confirmed_silence: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk() -> Chunk {
        Chunk {
            start_sample: 160_000,
            start_ms: 10_000,
            duration_ms: 20_000,
            sample_count: 320_000,
            mime_type: "audio/flac",
            flac_base64: "AAAA".into(),
        }
    }

    #[test]
    fn general_fence_truncation_and_invalid_member_keep_valid_absolute_segments() {
        let result = parse_general_text("```json\n[{\"start\":\"00:01\",\"end\":\"00:03\",\"text\":\"hello\"}, {\"start\":\"00:40\",\"end\":\"00:41\",\"text\":\"bad\"}, {\"start\":", &chunk()).unwrap();
        assert_eq!(result.segments.len(), 1);
        assert_eq!(
            (result.segments[0].start, result.segments[0].end),
            (11.0, 13.0)
        );
        assert!(!result.unresolved.is_empty());
        assert!(!result.confirmed_silence);
    }

    #[test]
    fn general_empty_array_is_only_explicit_silence() {
        assert!(
            parse_general_text("[]", &chunk())
                .unwrap()
                .confirmed_silence
        );
        // Spec I/O Matrix "Upper fence silence": an upper-case ```` ```JSON
        // ```` fence, with or without whitespace before the label, still
        // strips to a fenced `[]` and yields confirmed silence.
        for fenced in ["```JSON\n[]\n```", "``` JSON\n[]\n```", "```Json\n[]\n```"] {
            assert!(
                parse_general_text(fenced, &chunk())
                    .unwrap()
                    .confirmed_silence,
                "{fenced} must parse as confirmed silence"
            );
        }
        assert!(parse_general_text("", &chunk()).is_err());
        assert!(parse_general_response("{\"candidates\":[]}", &chunk()).is_err());
        assert!(parse_general_response(
            r#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":"[]"}]}}]}"#,
            &chunk()
        )
        .is_err());
    }

    #[test]
    fn duplicate_and_invalid_timestamps_do_not_become_silence() {
        let result = parse_general_text(r#"[{"start":"00:01","end":"00:02","text":"A"},{"start":"00:01","end":"00:02","text":"A"},{"start":"00:19","end":"00:21","text":"bad"}]"#, &chunk()).unwrap();
        assert_eq!(result.segments.len(), 1);
        assert!(!result.unresolved.is_empty());
        assert!(parse_general_text(
            r#"[{"start":"00:22","end":"00:23","text":"bad"}]"#,
            &chunk()
        )
        .is_err());
    }
}
