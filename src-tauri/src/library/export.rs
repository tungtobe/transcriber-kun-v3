//! Plain transcript export formatters. These functions do not read or mutate
//! the database; the IPC layer supplies the selected transcript's DB record
//! and writes the rendered bytes after the native save dialog returns a path.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::db::repo::segments::{SegmentKind, SegmentRow};
use crate::db::repo::transcripts::{Status, Variant};
use crate::library::store::TranscriptExportData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptExportFormat {
    Txt,
    Srt,
    Json,
}

impl TranscriptExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Srt => "srt",
            Self::Json => "json",
        }
    }

    pub fn filter_label(self) -> &'static str {
        match self {
            Self::Txt => "Text",
            Self::Srt => "SubRip subtitles",
            Self::Json => "JSON",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptExportOutcome {
    pub saved: bool,
    pub has_gaps: bool,
}

/// Localized gap notes for the TXT formatter, supplied by the frontend (the
/// same `session.export.gap*` i18n strings Copy already uses) so the export
/// note matches the UI language instead of a hard-coded English string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GapLabels {
    pub chunk_failed: String,
    pub disconnected: String,
    pub unknown: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedTranscriptExport {
    pub content: String,
    pub has_gaps: bool,
}

fn safe_offset(offset_sec: f64) -> f64 {
    if offset_sec.is_finite() {
        offset_sec
    } else {
        0.0
    }
}

/// Apply the display/export offset without changing the stored timestamp.
/// Non-finite values are normalized to zero so they cannot produce malformed
/// timestamps or invalid JSON numbers.
pub fn display_seconds(seconds: f64, offset_sec: f64) -> f64 {
    let seconds = if seconds.is_finite() { seconds } else { 0.0 };
    let adjusted = seconds + safe_offset(offset_sec);
    if !adjusted.is_finite() || adjusted < 0.0 {
        0.0
    } else {
        adjusted
    }
}

fn padded_timestamp(seconds: f64) -> String {
    let rounded = display_seconds(seconds, 0.0).round() as u64;
    let hours = rounded / 3_600;
    let minutes = (rounded % 3_600) / 60;
    let seconds = rounded % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn srt_timestamp(seconds: f64) -> String {
    let total_millis = (display_seconds(seconds, 0.0) * 1_000.0).round() as u128;
    let hours = total_millis / 3_600_000;
    let minutes = (total_millis / 60_000) % 60;
    let seconds = (total_millis / 1_000) % 60;
    let millis = total_millis % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn gap_note<'a>(row: &SegmentRow, labels: &'a GapLabels) -> &'a str {
    match row.gap_reason {
        Some(crate::db::repo::segments::GapReason::ChunkFailed) => &labels.chunk_failed,
        Some(crate::db::repo::segments::GapReason::Disconnected) => &labels.disconnected,
        None => &labels.unknown,
    }
}

fn has_gaps(data: &TranscriptExportData) -> bool {
    data.segments
        .iter()
        .any(|segment| segment.kind == SegmentKind::Gap)
}

fn format_txt(data: &TranscriptExportData, offset_sec: f64, labels: &GapLabels) -> String {
    data.segments
        .iter()
        .map(|segment| match segment.kind {
            SegmentKind::Text => format!(
                "[{}] {}",
                padded_timestamp(display_seconds(segment.start_sec, offset_sec)),
                segment.text
            ),
            SegmentKind::Gap => format!(
                "[{}–{}] {}",
                padded_timestamp(display_seconds(segment.start_sec, offset_sec)),
                padded_timestamp(display_seconds(segment.end_sec, offset_sec)),
                gap_note(segment, labels)
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_srt(data: &TranscriptExportData, offset_sec: f64) -> String {
    let mut cues = Vec::new();
    for segment in &data.segments {
        if segment.kind != SegmentKind::Text || segment.text.trim().is_empty() {
            continue;
        }
        let start = display_seconds(segment.start_sec, offset_sec);
        let end = display_seconds(segment.end_sec, offset_sec);
        if end <= start {
            continue;
        }
        cues.push(format!(
            "{}\n{} --> {}\n{}",
            cues.len() + 1,
            srt_timestamp(start),
            srt_timestamp(end),
            segment.text.trim()
        ));
    }
    cues.join("\n\n")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonSegment<'a> {
    idx: i64,
    start_sec: f64,
    end_sec: f64,
    kind: &'a str,
    gap_reason: Option<&'a str>,
    text: &'a str,
    speaker: Option<&'a str>,
}

fn serialize_variant(variant: Variant) -> &'static str {
    variant.as_str()
}

fn serialize_status(status: Status) -> &'static str {
    status.as_str()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonTranscript<'a> {
    transcript_id: String,
    session_id: String,
    variant: &'static str,
    status: &'static str,
    model: &'a str,
    language: &'a Option<String>,
    offset_seconds: f64,
    segments: Vec<JsonSegment<'a>>,
}

fn format_json(data: &TranscriptExportData, offset_sec: f64) -> Result<String, AppError> {
    let offset_sec = safe_offset(offset_sec);
    let document = JsonTranscript {
        transcript_id: data.transcript.id.to_string(),
        session_id: data.transcript.session_id.to_string(),
        variant: serialize_variant(data.transcript.variant),
        status: serialize_status(data.transcript.status),
        model: &data.transcript.model,
        language: &data.transcript.language,
        offset_seconds: offset_sec,
        segments: data
            .segments
            .iter()
            .map(|segment| JsonSegment {
                idx: segment.idx,
                start_sec: display_seconds(segment.start_sec, offset_sec),
                end_sec: display_seconds(segment.end_sec, offset_sec),
                kind: segment.kind.as_str(),
                gap_reason: segment.gap_reason.map(|reason| reason.as_str()),
                text: &segment.text,
                speaker: segment.speaker.as_deref(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&document)
        .map_err(|error| AppError::new(Code::Storage, error.to_string()))
}

/// Render one selected transcript in the chosen format. SRT intentionally
/// omits gaps and unusable cues; `has_gaps` lets the UI report that omission.
pub fn render_transcript(
    data: &TranscriptExportData,
    format: TranscriptExportFormat,
    offset_sec: f64,
    labels: &GapLabels,
) -> Result<RenderedTranscriptExport, AppError> {
    let content = match format {
        TranscriptExportFormat::Txt => format_txt(data, safe_offset(offset_sec), labels),
        TranscriptExportFormat::Srt => format_srt(data, safe_offset(offset_sec)),
        TranscriptExportFormat::Json => format_json(data, offset_sec)?,
    };
    Ok(RenderedTranscriptExport {
        content,
        has_gaps: has_gaps(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::{SessionId, TranscriptId};
    use crate::db::repo::segments::GapReason;

    fn record(segments: Vec<SegmentRow>) -> TranscriptExportData {
        TranscriptExportData {
            transcript: crate::db::repo::transcripts::TranscriptRow {
                id: TranscriptId::new(),
                session_id: SessionId::new(),
                variant: Variant::Primary,
                status: Status::Partial,
                model: "gemini-flash-lite-latest".to_string(),
                language: Some("vi".to_string()),
            },
            segments,
        }
    }

    fn text(
        idx: i64,
        start_sec: f64,
        end_sec: f64,
        text: &str,
        speaker: Option<&str>,
    ) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec,
            end_sec,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: text.to_string(),
            speaker: speaker.map(str::to_string),
        }
    }

    fn gap(idx: i64, start_sec: f64, end_sec: f64, reason: GapReason) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec,
            end_sec,
            kind: SegmentKind::Gap,
            gap_reason: Some(reason),
            text: String::new(),
            speaker: None,
        }
    }

    /// Distinct (non-English) labels so a test that passes them and checks
    /// the rendered output actually proves `render_transcript` used the
    /// supplied labels instead of a hard-coded string.
    fn labels() -> GapLabels {
        GapLabels {
            chunk_failed: "Khoảng này bị lỗi khi transcribe".to_string(),
            disconnected: "Mất kết nối".to_string(),
            unknown: "Khoảng thiếu".to_string(),
        }
    }

    #[test]
    fn txt_keeps_text_and_describes_gaps_with_offset() {
        let transcript = record(vec![
            text(0, -4.0, 2.0, "Xin chào", None),
            gap(1, 2.0, 4.5, GapReason::ChunkFailed),
        ]);
        let output =
            render_transcript(&transcript, TranscriptExportFormat::Txt, 3.0, &labels()).unwrap();
        assert_eq!(
            output.content,
            "[00:00] Xin chào\n[00:05–00:08] Khoảng này bị lỗi khi transcribe"
        );
        assert!(output.has_gaps);
    }

    #[test]
    fn txt_uses_the_disconnected_and_unknown_gap_labels() {
        let transcript = record(vec![
            gap(0, 0.0, 1.0, GapReason::Disconnected),
            SegmentRow {
                idx: 1,
                start_sec: 1.0,
                end_sec: 2.0,
                kind: SegmentKind::Gap,
                gap_reason: None,
                text: String::new(),
                speaker: None,
            },
        ]);
        let output =
            render_transcript(&transcript, TranscriptExportFormat::Txt, 0.0, &labels()).unwrap();
        assert_eq!(
            output.content,
            "[00:00–00:01] Mất kết nối\n[00:01–00:02] Khoảng thiếu"
        );
    }

    #[test]
    fn srt_omits_gaps_and_invalid_or_blank_cues_then_renumbers_survivors() {
        let transcript = record(vec![
            text(0, -5.0, -2.0, "clamped away", None),
            gap(1, 0.0, 1.0, GapReason::Disconnected),
            text(2, 1.2344, 2.5007, "  Câu còn lại  ", None),
            text(3, 4.0, 4.0, "zero duration", None),
            text(4, 5.0, 6.0, "   ", None),
        ]);
        let output =
            render_transcript(&transcript, TranscriptExportFormat::Srt, 1.0, &labels()).unwrap();
        assert_eq!(
            output.content,
            "1\n00:00:02,234 --> 00:00:03,501\nCâu còn lại"
        );
        assert!(output.has_gaps);
    }

    #[test]
    fn negative_offset_clamps_invalid_srt_cue_and_renumbers_survivor() {
        let transcript = record(vec![
            text(0, 1.0, 2.0, "clamped to zero", None),
            text(1, 5.0, 7.0, "surviving cue", None),
        ]);
        let output =
            render_transcript(&transcript, TranscriptExportFormat::Srt, -3.0, &labels()).unwrap();
        assert_eq!(
            output.content,
            "1\n00:00:02,000 --> 00:00:04,000\nsurviving cue"
        );
    }

    #[test]
    fn json_preserves_speaker_gap_metadata_and_exports_offset_timestamps() {
        let transcript = record(vec![
            text(0, 1.25, 2.5, "Xin chào", Some("speaker-a")),
            gap(1, 2.5, 3.5, GapReason::Disconnected),
        ]);
        let value: serde_json::Value = serde_json::from_str(
            &render_transcript(&transcript, TranscriptExportFormat::Json, 10.0, &labels())
                .unwrap()
                .content,
        )
        .unwrap();
        assert_eq!(value["transcriptId"], transcript.transcript.id.to_string());
        assert_eq!(value["offsetSeconds"], 10.0);
        assert_eq!(value["segments"][0]["startSec"], 11.25);
        assert_eq!(value["segments"][0]["speaker"], "speaker-a");
        assert_eq!(value["segments"][1]["kind"], "gap");
        assert_eq!(value["segments"][1]["gapReason"], "disconnected");
    }

    #[test]
    fn json_preserves_negative_offset_metadata_and_clamps_adjusted_timestamps() {
        let transcript = record(vec![text(0, 1.0, 2.0, "clamped", None)]);
        let value: serde_json::Value = serde_json::from_str(
            &render_transcript(&transcript, TranscriptExportFormat::Json, -3.0, &labels())
                .unwrap()
                .content,
        )
        .unwrap();
        assert_eq!(value["offsetSeconds"], -3.0);
        assert_eq!(value["segments"][0]["startSec"], 0.0);
        assert_eq!(value["segments"][0]["endSec"], 0.0);
    }

    #[test]
    fn timestamps_round_to_milliseconds_and_clamp_negative_seconds() {
        assert_eq!(srt_timestamp(display_seconds(-0.4, 0.0)), "00:00:00,000");
        assert_eq!(srt_timestamp(3661.2346), "01:01:01,235");
    }
}
