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
    /// Count of Text segments `push_forward` had to clamp away entirely
    /// (start pushed to >= end). Logged as a bare count, never with the text
    /// itself (spec Boundaries Always: "Log only a count (content-free
    /// logging)").
    clamped_text_count: u32,
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

    /// Chèn một mục, kẹp `start` không bao giờ lùi trước con trỏ hiện tại. Một
    /// mục suy biến sau khi kẹp (`start >= end`) không còn bị bỏ âm thầm nếu
    /// nó là text đã transcribe được (spec Boundaries Always P0 review:
    /// "không im lặng bỏ text; gộp vào Text trước đó, hoặc giữ placeholder
    /// rỗng tại con trỏ nếu không có") -- chỉ gap suy biến (không mang nội
    /// dung) mới thật sự bị bỏ, vì không có gì để mất.
    fn push_forward(&mut self, mut start: f64, end: f64, mut draft: SegmentDraft) {
        if start < self.cursor {
            start = self.cursor;
        }
        if start >= end {
            if draft.kind == SegmentKind::Text && !draft.text.is_empty() {
                self.absorb_clamped_text(draft);
            }
            return;
        }
        draft.start_sec = start;
        draft.end_sec = end;
        self.cursor = end;
        self.segments.push(draft);
    }

    /// A Text segment `push_forward` just clamped down to zero length (its
    /// whole span already covered by the monotonic cursor). Its text is
    /// appended to the immediately preceding segment when that segment is
    /// itself Text (the common case: two chunk-boundary segments describing
    /// the same instant); otherwise there is no previous Text segment to
    /// merge into, so the text is kept as a zero-length Text segment pinned
    /// at the cursor instead of disappearing.
    fn absorb_clamped_text(&mut self, clamped: SegmentDraft) {
        match self.segments.last_mut() {
            Some(previous) if previous.kind == SegmentKind::Text => {
                if !previous.text.is_empty() && !clamped.text.is_empty() {
                    previous.text.push(' ');
                }
                previous.text.push_str(&clamped.text);
            }
            _ => {
                self.segments.push(SegmentDraft {
                    start_sec: self.cursor,
                    end_sec: self.cursor,
                    kind: SegmentKind::Text,
                    gap_reason: None,
                    text: clamped.text,
                    speaker: clamped.speaker,
                });
            }
        }
        self.clamped_text_count += 1;
        tracing::warn!(
            count = self.clamped_text_count,
            "một đoạn text transcribe được bị kẹp trùng thời gian bởi push_forward, đã gộp vào đoạn trước hoặc giữ placeholder tại con trỏ"
        );
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

/// 1 ms tolerance used by [`trim_outside_ranges`] to decide whether an old
/// segment lies *within* a retried range (spec Boundaries Always P0 review:
/// "drops an old segment only when it lies within a range (1 ms
/// tolerance)") -- absorbs the float rounding `range_from_sec`/`ms_to_sample`
/// round-tripping can introduce at a range boundary.
const RANGE_CONTAINMENT_TOLERANCE_SEC: f64 = 0.001;

/// Trims `segment` to the parts of its span that fall outside every retried
/// range, dropping it entirely only when it lies (within 1 ms tolerance)
/// inside a single range (spec Boundaries Always P0 review: "`splice_rerun`
/// drops an old segment only when it lies within a range (1 ms tolerance); an
/// old segment that partially overlaps a range is trimmed to the part outside
/// the range and kept"). Ranges are assumed non-overlapping, so at most one
/// edge of `segment` is trimmed per range; a segment straddling clean through
/// the middle of a range keeps only its leading part outside that range --
/// not reachable via `resolve_ranges` (a retried range is always at least the
/// old gap it replaces), so this simpler edge-trim is sufficient here without
/// needing to split one draft into two.
fn trim_outside_ranges(mut segment: SegmentDraft, ranges: &[(f64, f64)]) -> Option<SegmentDraft> {
    for &(range_start, range_end) in ranges {
        if segment.start_sec >= range_start - RANGE_CONTAINMENT_TOLERANCE_SEC
            && segment.end_sec <= range_end + RANGE_CONTAINMENT_TOLERANCE_SEC
        {
            return None;
        }
        if segment.end_sec > range_start && segment.start_sec < range_start {
            segment.end_sec = segment.end_sec.min(range_start);
        } else if segment.start_sec < range_end && segment.end_sec > range_end {
            segment.start_sec = segment.start_sec.max(range_end);
        }
    }
    if segment.start_sec >= segment.end_sec {
        return None;
    }
    Some(segment)
}

/// Hợp nhất kết quả Chạy lại (2.5): Segment cũ nằm hẳn ngoài mọi vùng retry
/// giữ nguyên, Segment cũ chỉ chồng lấp một phần (sai số làm tròn ở biên
/// vùng) được kẹp còn lại phần ngoài vùng thay vì bị bỏ hẳn (spec Boundaries
/// Always P0 review); Segment mới của mỗi vùng (đã qua `MergeBuilder` riêng
/// của vùng đó) được kẹp trong đúng vùng rồi chèn, tất cả sắp lại theo thời
/// gian bắt đầu (spec Tasks: "hàm hợp nhất ... kẹp trong vùng ... đơn điệu,
/// không trùng text/gap"). `old_segments` rỗng cho scope `all` (spec Always:
/// "bỏ Segment cũ").
pub fn splice_rerun(
    old_segments: Vec<SegmentDraft>,
    ranges: &[(f64, f64)],
    per_range_new: Vec<Vec<SegmentDraft>>,
) -> Vec<SegmentDraft> {
    let mut items: Vec<SegmentDraft> = old_segments
        .into_iter()
        .filter_map(|segment| trim_outside_ranges(segment, ranges))
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

    // P0 review fix: `push_forward` no longer silently drops text it clamps
    // away entirely (spec I/O Matrix "Clamped text").

    #[test]
    fn a_clamped_text_segment_is_appended_to_the_previous_text_segment() {
        let mut builder = MergeBuilder::new();
        builder.push_success(ChunkTranscript {
            segments: vec![text(0.0, 5.0, "a")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        // Cursor is now 5.0 -- this segment starts before it and, once
        // clamped to `[5.0, 4.0)`, is fully degenerate (spec I/O Matrix:
        // "segment [3,4] 'b' after cursor 5 with previous Text 'a'").
        builder.push_success(ChunkTranscript {
            segments: vec![text(3.0, 4.0, "b")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        let segments = builder.finish();
        assert_eq!(segments.len(), 1, "text bị kẹp phải gộp vào, không tạo segment mới");
        assert_eq!(segments[0].text, "a b");
    }

    #[test]
    fn a_clamped_text_segment_with_no_previous_text_is_kept_as_a_zero_length_placeholder() {
        let mut builder = MergeBuilder::new();
        // A gap immediately precedes the clamped text -- there is no
        // previous Text segment to merge into.
        builder.push_failed_chunk(&chunk(0, 5_000));
        builder.push_success(ChunkTranscript {
            segments: vec![text(2.0, 4.0, "b")],
            unresolved: vec![],
            confirmed_silence: false,
        });
        let segments = builder.finish();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].kind, SegmentKind::Text);
        assert_eq!(segments[1].text, "b");
        assert_eq!(segments[1].start_sec, segments[1].end_sec);
        assert_eq!(segments[1].start_sec, 5.0, "placeholder phải ghim tại con trỏ");
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

    // P0 review fix: an old segment that only barely (rounding-error) pokes
    // into a retried range is trimmed, not silently dropped whole (spec I/O
    // Matrix "Rounded neighbor").

    #[test]
    fn splice_rerun_trims_an_old_segment_that_only_barely_overlaps_a_range() {
        let old = vec![draft(0.0, 10.0004, SegmentKind::Text, "trước")];
        let ranges = [(10.0, 20.0)];
        let new_for_range = vec![vec![draft(10.0, 20.0, SegmentKind::Text, "đã vá")]];

        let merged = splice_rerun(old, &ranges, new_for_range);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].text, "trước");
        assert_eq!(merged[0].start_sec, 0.0);
        assert_eq!(merged[0].end_sec, 10.0);
        assert_eq!(merged[1].text, "đã vá");
    }

    #[test]
    fn splice_rerun_still_drops_an_old_segment_within_tolerance_of_a_range() {
        // Starts 0.4 ms after the nominal range start -- within the 1 ms
        // containment tolerance, so this is still "lies within a range", not
        // a partial overlap to trim.
        let old = vec![draft(10.0004, 20.0, SegmentKind::Gap, "")];
        let ranges = [(10.0, 20.0)];
        let new_for_range = vec![vec![draft(10.0, 20.0, SegmentKind::Text, "đã vá")]];

        let merged = splice_rerun(old, &ranges, new_for_range);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "đã vá");
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
