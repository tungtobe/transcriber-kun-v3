//! Gộp `ChunkTranscript` liên tiếp (theo đúng thứ tự Chunk) thành một danh
//! sách `SegmentDraft` đơn điệu cho cả file (spec Tasks: "merge Chunk ->
//! `SegmentDraft` đơn điệu + gap"). Mỗi Chunk được xử lý tuần tự nên thời
//! gian tuyệt đối của nó (`segments`/`unresolved` từ `transcribe::parser`)
//! không bao giờ chồng lên Chunk trước — [`MergeBuilder`] vẫn tự vệ bằng một
//! con trỏ thời gian đơn điệu (`cursor`) để "Segment không lùi thời gian"
//! (spec I/O Matrix "Happy path") đúng kể cả khi một Chunk trả dữ liệu bất
//! thường.

use crate::db::repo::segments::{GapReason, SegmentDraft, SegmentKind};
use crate::media::Chunk;
use crate::transcribe::parser::ChunkTranscript;

/// Gộp dần từng Chunk (thành công hoặc lỗi) thành một danh sách
/// `SegmentDraft` cuối cùng, dùng cho `TranscriptDraft::segments`.
#[derive(Debug, Default)]
pub struct MergeBuilder {
    segments: Vec<SegmentDraft>,
    cursor: f64,
}

impl MergeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Một Chunk transcribe thành công (dù có thể có `unresolved` từ
    /// parser): text segment và gap `chunk_failed` (từ `unresolved`) được
    /// gộp và sắp theo thời gian bắt đầu trước khi chèn (spec I/O Matrix
    /// "Phần chưa cứu trong Chunk -> Thành gap `chunk_failed`").
    pub fn push_success(&mut self, transcript: ChunkTranscript) {
        let mut items: Vec<(f64, f64, SegmentDraft)> =
            Vec::with_capacity(transcript.segments.len() + transcript.unresolved.len());
        for segment in transcript.segments {
            items.push((
                segment.start,
                segment.end,
                SegmentDraft {
                    start_sec: segment.start,
                    end_sec: segment.end,
                    kind: SegmentKind::Text,
                    gap_reason: None,
                    text: segment.text,
                    speaker: segment.speaker,
                },
            ));
        }
        for gap in transcript.unresolved {
            items.push((
                gap.start,
                gap.end,
                SegmentDraft {
                    start_sec: gap.start,
                    end_sec: gap.end,
                    kind: SegmentKind::Gap,
                    gap_reason: Some(GapReason::ChunkFailed),
                    text: String::new(),
                    speaker: None,
                },
            ));
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for (start, end, draft) in items {
            self.push_forward(start, end, draft);
        }
    }

    /// Toàn bộ Chunk lỗi (không phải Auth/Model/Consent — Job vẫn tiếp tục,
    /// spec I/O Matrix "Chunk lỗi"): trở thành đúng một gap `chunk_failed`
    /// phủ hết khoảng thời gian của Chunk đó.
    pub fn push_failed_chunk(&mut self, chunk: &Chunk) {
        let start = chunk.start_ms as f64 / 1000.0;
        let end = (chunk.start_ms + chunk.duration_ms) as f64 / 1000.0;
        self.push_forward(
            start,
            end,
            SegmentDraft {
                start_sec: start,
                end_sec: end,
                kind: SegmentKind::Gap,
                gap_reason: Some(GapReason::ChunkFailed),
                text: String::new(),
                speaker: None,
            },
        );
    }

    /// Chèn một mục, kẹp `start` không bao giờ lùi trước con trỏ hiện tại;
    /// bỏ qua mục suy biến (`start >= end` sau khi kẹp).
    fn push_forward(&mut self, mut start: f64, end: f64, mut draft: SegmentDraft) {
        if start < self.cursor {
            start = self.cursor;
        }
        if start >= end {
            return;
        }
        draft.start_sec = start;
        draft.end_sec = end;
        self.cursor = end;
        self.segments.push(draft);
    }

    pub fn finish(self) -> Vec<SegmentDraft> {
        self.segments
    }
}

/// Kẹp một segment mới trong đúng vùng retry `[range_start, range_end]`
/// (spec Always: "Segment mới được kẹp trong vùng"). Bỏ mục suy biến sau khi
/// kẹp (vùng đã hết hoặc segment nằm hẳn ngoài vùng).
fn clamp_to_range(
    mut draft: SegmentDraft,
    range_start: f64,
    range_end: f64,
) -> Option<SegmentDraft> {
    let start = draft.start_sec.max(range_start);
    let end = draft.end_sec.min(range_end);
    if start >= end {
        return None;
    }
    draft.start_sec = start;
    draft.end_sec = end;
    Some(draft)
}

/// Hợp nhất kết quả Chạy lại (2.5): Segment cũ nằm hẳn ngoài mọi vùng retry
/// giữ nguyên; Segment mới của mỗi vùng (đã qua `MergeBuilder` riêng của
/// vùng đó) được kẹp trong đúng vùng rồi chèn, tất cả sắp lại theo thời gian
/// bắt đầu (spec Tasks: "hàm hợp nhất ... kẹp trong vùng ... đơn điệu, không
/// trùng text/gap"). `old_segments` rỗng cho scope `all` (spec Always: "bỏ
/// Segment cũ").
pub fn splice_rerun(
    old_segments: Vec<SegmentDraft>,
    ranges: &[(f64, f64)],
    per_range_new: Vec<Vec<SegmentDraft>>,
) -> Vec<SegmentDraft> {
    let mut items: Vec<SegmentDraft> = old_segments
        .into_iter()
        .filter(|segment| {
            !ranges
                .iter()
                .any(|&(start, end)| segment.start_sec < end && segment.end_sec > start)
        })
        .collect();

    for (range, new_segments) in ranges.iter().zip(per_range_new) {
        for draft in new_segments {
            if let Some(clamped) = clamp_to_range(draft, range.0, range.1) {
                items.push(clamped);
            }
        }
    }

    items.sort_by(|a, b| {
        a.start_sec
            .total_cmp(&b.start_sec)
            .then(a.end_sec.total_cmp(&b.end_sec))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::parser::{MissingRange, Segment};

    fn chunk(start_ms: u64, duration_ms: u64) -> Chunk {
        Chunk {
            start_sample: 0,
            start_ms,
            duration_ms,
            sample_count: 0,
            mime_type: "audio/flac",
            flac_base64: String::new(),
        }
    }

    fn text(start: f64, end: f64, text: &str) -> Segment {
        Segment {
            start,
            end,
            text: text.to_string(),
            speaker: None,
        }
    }

    #[test]
    fn successive_chunks_concatenate_in_absolute_chronological_order() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![text(0.0, 1.0, "a"), text(1.0, 2.0, "b")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        builder.push_success(ChunkTranscript {
            segments: vec![text(300.0, 301.0, "c")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        let segments = builder.finish();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "a");
        assert_eq!(segments[2].start_sec, 300.0);
        for pair in segments.windows(2) {
            assert!(pair[1].start_sec >= pair[0].end_sec);
        }
    }

    #[test]
    fn unresolved_ranges_interleave_by_start_time_as_chunk_failed_gaps() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![text(0.0, 1.0, "a"), text(2.0, 3.0, "b")],
            unresolved: vec![MissingRange {
                start: 1.0,
                end: 2.0,
            }],
            confirmed_silence: false,
        });
        let segments = builder.finish();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1].kind, SegmentKind::Gap);
        assert_eq!(segments[1].gap_reason, Some(GapReason::ChunkFailed));
        assert_eq!((segments[1].start_sec, segments[1].end_sec), (1.0, 2.0));
    }

    #[test]
    fn a_fully_failed_chunk_becomes_one_gap_covering_its_whole_span() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![text(0.0, 1.0, "a")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        builder.push_failed_chunk(&chunk(300_000, 300_000));
        let segments = builder.finish();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].kind, SegmentKind::Gap);
        assert_eq!(segments[1].gap_reason, Some(GapReason::ChunkFailed));
        assert_eq!(segments[1].start_sec, 300.0);
        assert_eq!(segments[1].end_sec, 600.0);
    }

    #[test]
    fn an_out_of_order_or_overlapping_item_never_makes_time_go_backward() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![text(5.0, 10.0, "a")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        // A late/duplicated item claiming to start before the cursor must be
        // clamped forward, never rewind the transcript.
        builder.push_success(ChunkTranscript {
            segments: vec![text(2.0, 6.0, "overlap")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        let segments = builder.finish();
        for pair in segments.windows(2) {
            assert!(pair[1].start_sec >= pair[0].end_sec);
        }
    }

    #[test]
    fn confirmed_silence_with_no_segments_contributes_nothing() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![],
            unresolved: vec![],
            confirmed_silence: true,
        });
        assert!(builder.finish().is_empty());
    }

    fn draft(start: f64, end: f64, kind: SegmentKind, text: &str) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind,
            gap_reason: if kind == SegmentKind::Gap {
                Some(GapReason::ChunkFailed)
            } else {
                None
            },
            text: text.to_string(),
            speaker: None,
        }
    }

    #[test]
    fn splice_rerun_keeps_old_segments_outside_the_retried_ranges() {
        let old = vec![
            draft(0.0, 10.0, SegmentKind::Text, "trước"),
            draft(10.0, 20.0, SegmentKind::Gap, ""),
            draft(20.0, 30.0, SegmentKind::Text, "sau"),
        ];
        let ranges = [(10.0, 20.0)];
        let new_for_range = vec![vec![draft(10.0, 20.0, SegmentKind::Text, "đã vá")]];

        let merged = splice_rerun(old, &ranges, new_for_range);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].text, "trước");
        assert_eq!(merged[1].text, "đã vá");
        assert_eq!(merged[1].kind, SegmentKind::Text);
        assert_eq!(merged[2].text, "sau");
    }

    #[test]
    fn splice_rerun_discards_every_old_segment_for_the_all_scope() {
        let old = vec![draft(0.0, 30.0, SegmentKind::Text, "cũ")];
        let ranges = [(0.0, 30.0)];
        let new_for_range = vec![vec![draft(0.0, 30.0, SegmentKind::Text, "toàn bộ mới")]];

        let merged = splice_rerun(old, &ranges, new_for_range);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "toàn bộ mới");
    }

    #[test]
    fn splice_rerun_clamps_new_segments_to_their_own_range() {
        let old = vec![];
        let ranges = [(10.0, 20.0)];
        // A hallucinated timestamp that overruns the retried range on both
        // ends must be clamped, never allowed to swallow neighboring time.
        let new_for_range = vec![vec![draft(5.0, 25.0, SegmentKind::Text, "tràn vùng")]];

        let merged = splice_rerun(old, &ranges, new_for_range);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start_sec, 10.0);
        assert_eq!(merged[0].end_sec, 20.0);
    }

    #[test]
    fn splice_rerun_supports_two_disjoint_missing_ranges_independently() {
        let old = vec![
            draft(0.0, 5.0, SegmentKind::Text, "a"),
            draft(5.0, 10.0, SegmentKind::Gap, ""),
            draft(10.0, 15.0, SegmentKind::Text, "b"),
            draft(15.0, 20.0, SegmentKind::Gap, ""),
            draft(20.0, 25.0, SegmentKind::Text, "c"),
        ];
        let ranges = [(5.0, 10.0), (15.0, 20.0)];
        let new_for_range = vec![
            vec![draft(5.0, 10.0, SegmentKind::Text, "vá 1")],
            vec![draft(15.0, 20.0, SegmentKind::Text, "vá 2")],
        ];

        let merged = splice_rerun(old, &ranges, new_for_range);

        let texts: Vec<&str> = merged.iter().map(|segment| segment.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "vá 1", "b", "vá 2", "c"]);
    }
}
