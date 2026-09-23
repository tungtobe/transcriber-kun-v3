//! Pure response parser shared by both Gemini transcription families.
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

fn shape() -> AppError {
    AppError::new(
        Code::Shape,
        "Gemini transcription response is incomplete or invalid",
    )
}

pub fn parse_general_response(body: &str, chunk: &Chunk) -> Result<ChunkTranscript, AppError> {
    let response: Value = serde_json::from_str(body).map_err(|_| shape())?;
    if response.pointer("/promptFeedback/blockReason").is_some() {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini blocked the transcription",
        ));
    }
    let candidate = response.pointer("/candidates/0").ok_or_else(shape)?;
    if candidate.get("finishReason").and_then(Value::as_str) == Some("SAFETY") {
        return Err(AppError::new(
            Code::Blocked,
            "Gemini blocked the transcription",
        ));
    }
    let text = candidate
        .pointer("/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or_else(shape)?;
    let mut result = parse_general_text(text, chunk)?;
    if candidate.get("finishReason").and_then(Value::as_str) != Some("STOP") {
        if result.confirmed_silence {
            return Err(shape());
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

pub fn parse_general_text(text: &str, chunk: &Chunk) -> Result<ChunkTranscript, AppError> {
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if clean.is_empty() {
        return Err(shape());
    }
    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        let items = value.as_array().ok_or_else(shape)?;
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

fn parse_offset(value: &str) -> Option<f64> {
    let seconds = value.strip_suffix('s')?.parse::<f64>().ok()?;
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

pub fn parse_interaction_response(body: &str, chunk: &Chunk) -> Result<ChunkTranscript, AppError> {
    let response: Value = serde_json::from_str(body).map_err(|_| shape())?;
    let completed = response.get("status").and_then(Value::as_str) == Some("completed");
    let steps = response
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(shape)?;
    let mut words = Vec::new();
    let mut invalid = false;
    for step in steps {
        if step.get("type").and_then(Value::as_str) != Some("model_output") {
            continue;
        }
        for content in step
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for annotation in content
                .get("annotations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if annotation.get("type").and_then(Value::as_str) != Some("word_info") {
                    continue;
                }
                let word = (|| {
                    let start = parse_offset(annotation.get("start_offset")?.as_str()?)?;
                    let end = parse_offset(annotation.get("end_offset")?.as_str()?)?;
                    let text = annotation.get("text")?.as_str()?.trim();
                    if text.is_empty() {
                        return None;
                    }
                    Some(Segment {
                        start,
                        end,
                        text: text.to_owned(),
                        speaker: annotation
                            .get("speaker")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })();
                if let Some(word) = word {
                    if word.end > word.start && word.end <= chunk.duration_ms as f64 / 1000.0 {
                        words.push(word);
                    } else {
                        invalid = true;
                    }
                } else {
                    invalid = true;
                }
            }
        }
    }
    if words.is_empty() {
        let output = response.get("output_text").and_then(Value::as_str);
        if completed && output.is_some_and(|text| text.trim().is_empty()) && !invalid {
            return Ok(ChunkTranscript {
                segments: vec![],
                unresolved: vec![],
                confirmed_silence: true,
            });
        }
        return Err(shape());
    }
    words.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut groups: Vec<Segment> = Vec::new();
    for word in words {
        if let Some(last) = groups.last_mut() {
            if last.speaker == word.speaker
                && word.end - last.start <= 15.0
                && word.start - last.start < 8.0
            {
                last.end = last.end.max(word.end);
                last.text.push(' ');
                last.text.push_str(&word.text);
                continue;
            }
        }
        groups.push(word);
    }
    normalize(groups, chunk, invalid || !completed)
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
        return Err(shape());
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
        assert!(parse_general_text("", &chunk()).is_err());
        assert!(parse_general_response("{\"candidates\":[]}", &chunk()).is_err());
        assert!(parse_general_response(
            r#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":"[]"}]}}]}"#,
            &chunk()
        )
        .is_err());
    }

    #[test]
    fn interaction_groups_words_and_splits_speakers() {
        let body = r#"{"status":"completed","steps":[{"type":"model_output","content":[{"type":"text","annotations":[{"type":"word_info","text":"Hello","speaker":"spk_1","start_offset":"0.100s","end_offset":"0.400s"},{"type":"word_info","text":"world","speaker":"spk_1","start_offset":"0.500s","end_offset":"0.900s"},{"type":"word_info","text":"Yes","speaker":"spk_2","start_offset":"1.000s","end_offset":"1.500s"}]}]}]}"#;
        let result = parse_interaction_response(body, &chunk()).unwrap();
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].text, "Hello world");
        assert_eq!(result.segments[0].speaker.as_deref(), Some("spk_1"));
        assert_eq!(result.segments[0].start, 10.1);
        assert!(result.unresolved.is_empty());
    }

    #[test]
    fn interaction_drops_invalid_word_before_grouping_and_marks_missing() {
        let body = r#"{"status":"completed","steps":[{"type":"model_output","content":[{"type":"text","annotations":[{"type":"word_info","text":"Valid","speaker":"spk_1","start_offset":"0.100s","end_offset":"0.400s"},{"type":"word_info","text":"Invalid","speaker":"spk_1","start_offset":"0.500s","end_offset":"30.000s"}]}]}]}"#;
        let result = parse_interaction_response(body, &chunk()).unwrap();
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].text, "Valid");
        assert!(!result.unresolved.is_empty());
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
