//! Chạy lại (story 2.5): giải `scope` thành vùng thời gian tuyệt đối
//! (`RerunRange`) từ Segment đã lưu, rồi decode Proxy một lần duy nhất, đẩy
//! mẫu vào đúng `Chunker` của từng vùng (spec Design Notes: "Chunker của vùng
//! bắt đầu từ 0", dời `start_ms`/`start_sample` về tuyệt đối trước khi gửi).
//! Không chạm DB ghi ở đây -- chỉ đọc (`resolve_ranges` nhận `SegmentRow` đã
//! đọc sẵn) và decode/chunk thuần; `registry.rs` lắp các mảnh này vào pipeline
//! Job.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::db::repo::segments::{GapReason, SegmentKind, SegmentRow};
use crate::gemini::CancellationToken;
use crate::media::{self, Chunk, ChunkOptions, Chunker, OUTPUT_SAMPLE_RATE};

/// Vùng cần Chạy lại, ở cả hai đơn vị (mẫu cho việc định tuyến lúc decode, ms
/// cho tiến độ/độ lệch thời gian) để không phải quy đổi lặp lại và lệch làm
/// tròn giữa hai nơi dùng.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RerunRange {
    pub start_sample: u64,
    pub start_ms: u64,
    pub end_sample: u64,
    pub end_ms: u64,
}

impl RerunRange {
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// Phạm vi Chạy lại nhận từ `transcribe_rerun` (spec Approach: `scope ∈
/// {missing, all, gap(gap_id)}`). `gap_id` là `idx` của Segment gap trong
/// đúng `transcript_id` -- không phải một range do UI tự tính.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RerunScope {
    Missing,
    All,
    /// `gap_id` is a Segment `idx` (spec Always) — `i32` at the IPC boundary
    /// (specta-typescript forbids exporting BigInt-style `i64`, see
    /// `ipc/spike_channel.rs`); no transcript ever has anywhere close to
    /// `i32::MAX` segments, so the narrowing is lossless in practice.
    Gap {
        gap_id: i32,
    },
}

fn ms_to_sample(ms: u64) -> u64 {
    (u128::from(ms) * u128::from(OUTPUT_SAMPLE_RATE) / 1000) as u64
}

fn range_from_ms(start_ms: u64, end_ms: u64) -> RerunRange {
    let (start_ms, end_ms) = (start_ms.min(end_ms), start_ms.max(end_ms));
    RerunRange {
        start_ms,
        end_ms,
        start_sample: ms_to_sample(start_ms),
        end_sample: ms_to_sample(end_ms),
    }
}

fn range_from_sec(start_sec: f64, end_sec: f64) -> RerunRange {
    let start_ms = (start_sec * 1000.0).round().max(0.0) as u64;
    let end_ms = (end_sec * 1000.0).round().max(0.0) as u64;
    range_from_ms(start_ms, end_ms)
}

fn invalid_gap_error() -> AppError {
    AppError::new(
        Code::Request,
        "gap_id không tồn tại hoặc không phải gap chunk_failed của transcript này",
    )
}

/// Tolerance (ms) used to decide a range "reaches the end of the recording"
/// (spec Boundaries Always P0 review) -- `total_duration_ms` comes from
/// `probe`'s float `duration_seconds` rounded to ms, so the true tail can sit
/// up to about a second past it (container duration metadata is an estimate,
/// not exact sample count).
const TAIL_RANGE_TOLERANCE_MS: u64 = 1_000;

/// Marks the last range in `ranges` as open-ended (`end_sample = u64::MAX`,
/// `end_ms` unchanged) when it reaches the end of the recording, so decoding
/// routes every sample up to the Proxy's *true* end into it instead of
/// stopping at a possibly-short `total_duration_ms` estimate (spec Boundaries
/// Always P0 review: "the last range that reaches the end of the recording
/// ... is open-ended -- it consumes decoded samples to the true end of the
/// Proxy"). `end_ms` is left as-is: `duration_ms()` still needs a finite
/// value for progress estimates.
fn mark_tail_open_ended(mut ranges: Vec<RerunRange>, total_duration_ms: u64) -> Vec<RerunRange> {
    if let Some(last) = ranges.last_mut() {
        if last.end_ms + TAIL_RANGE_TOLERANCE_MS >= total_duration_ms {
            last.end_sample = u64::MAX;
        }
    }
    ranges
}

/// Giải `scope` thành danh sách vùng tuyệt đối, tăng dần, không chồng lấp
/// (spec Always: "`gap_id` = `idx` của Segment gap `chunk_failed` trong đúng
/// `transcript_id`; Rust tự tra range từ DB"). `segments` phải là toàn bộ
/// Segment của đúng transcript đang nhắm tới, theo thứ tự `idx`
/// (`segments::list_for_transcript`). Rỗng cho `Missing` nghĩa là "không có
/// gì để Chạy lại" -- caller quyết định `NothingToRerun`, hàm này không tự
/// suy ra outcome đó.
pub fn resolve_ranges(
    scope: RerunScope,
    segments: &[SegmentRow],
    total_duration_ms: u64,
) -> Result<Vec<RerunRange>, AppError> {
    match scope {
        RerunScope::All => Ok(mark_tail_open_ended(
            vec![range_from_ms(0, total_duration_ms)],
            total_duration_ms,
        )),
        RerunScope::Missing => {
            let ranges = segments
                .iter()
                .filter(|row| {
                    row.kind == SegmentKind::Gap && row.gap_reason == Some(GapReason::ChunkFailed)
                })
                .map(|row| range_from_sec(row.start_sec, row.end_sec))
                .collect();
            Ok(mark_tail_open_ended(ranges, total_duration_ms))
        }
        RerunScope::Gap { gap_id } => {
            let row = segments
                .iter()
                .find(|row| row.idx == i64::from(gap_id))
                .ok_or_else(invalid_gap_error)?;
            if row.kind != SegmentKind::Gap || row.gap_reason != Some(GapReason::ChunkFailed) {
                return Err(invalid_gap_error());
            }
            let ranges = vec![range_from_sec(row.start_sec, row.end_sec)];
            Ok(mark_tail_open_ended(ranges, total_duration_ms))
        }
    }
}

fn cancelled_pipeline_error() -> AppError {
    AppError::new(Code::Blocked, "transcribe job was cancelled")
}

/// Decode toàn bộ Proxy một lần, chỉ đẩy mẫu thuộc từng `ranges[i]` vào
/// `Chunker` riêng của vùng đó (bắt đầu từ 0 -- spec Design Notes), gọi
/// `emit(i, chunk)` cho mỗi Chunk phát ra. `ranges` phải tăng dần và không
/// chồng lấp (đúng như `resolve_ranges` trả về). Mẫu nằm ngoài mọi vùng bị
/// bỏ qua, không decode dư ra ngoài việc phải đọc tuần tự qua chúng.
pub fn decode_ranges_and_chunk(
    proxy_path: &Path,
    ranges: &[RerunRange],
    options: ChunkOptions,
    cancel: &CancellationToken,
    mut emit: impl FnMut(usize, Chunk) -> Result<(), AppError>,
) -> Result<(), AppError> {
    if ranges.is_empty() {
        return Ok(());
    }
    let mut chunkers: Vec<Chunker> = ranges
        .iter()
        .map(|_| Chunker::new(options))
        .collect::<Result<_, _>>()?;
    let mut global_sample: u64 = 0;
    let mut active = 0usize;

    media::decode_mono_16khz(proxy_path, |samples| {
        if cancel.is_cancelled() {
            return Err(cancelled_pipeline_error());
        }
        let mut offset = 0usize;
        while offset < samples.len() {
            while active < ranges.len()
                && global_sample + offset as u64 >= ranges[active].end_sample
            {
                let index = active;
                chunkers[index].finish(&mut |chunk| emit(index, chunk))?;
                active += 1;
            }
            if active >= ranges.len() {
                break;
            }
            let range = ranges[active];
            let position = global_sample + offset as u64;
            if position < range.start_sample {
                let skip = ((range.start_sample - position) as usize).min(samples.len() - offset);
                offset += skip;
                continue;
            }
            // `range.end_sample` can be `u64::MAX` for an open-ended tail
            // range (spec Boundaries Always) -- subtract with saturation and
            // only narrow to `usize` after clamping to the buffer length still
            // remaining, so this never overflows regardless of pointer width.
            let remaining_in_range = range.end_sample.saturating_sub(position);
            let remaining_in_buffer = (samples.len() - offset) as u64;
            let take = remaining_in_range.min(remaining_in_buffer) as usize;
            if take == 0 {
                active += 1;
                continue;
            }
            let index = active;
            chunkers[index].push(&samples[offset..offset + take], &mut |chunk| {
                emit(index, chunk)
            })?;
            offset += take;
        }
        global_sample += samples.len() as u64;
        Ok(())
    })
    .map(|_stats| ())?;

    while active < ranges.len() {
        let index = active;
        chunkers[index].finish(&mut |chunk| emit(index, chunk))?;
        active += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::segments::SegmentKind;

    fn gap_row(idx: i64, start: f64, end: f64) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Gap,
            gap_reason: Some(GapReason::ChunkFailed),
            text: String::new(),
            speaker: None,
        }
    }

    fn text_row(idx: i64, start: f64, end: f64) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: "hi".to_string(),
            speaker: None,
        }
    }

    #[test]
    fn resolve_ranges_all_covers_the_whole_proxy_regardless_of_segments() {
        let segments = vec![text_row(0, 0.0, 5.0)];
        let ranges = resolve_ranges(RerunScope::All, &segments, 90_000).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_ms, 0);
        assert_eq!(ranges[0].end_ms, 90_000);
        // `all` always reaches the end of the recording by definition -- it
        // must always come back open-ended (spec Boundaries Always).
        assert_eq!(ranges[0].end_sample, u64::MAX);
    }

    #[test]
    fn resolve_ranges_missing_collects_only_chunk_failed_gaps_in_order() {
        let segments = vec![
            text_row(0, 0.0, 10.0),
            gap_row(1, 10.0, 20.0),
            text_row(2, 20.0, 30.0),
            gap_row(3, 30.0, 40.0),
        ];
        let ranges = resolve_ranges(RerunScope::Missing, &segments, 40_000).unwrap();
        assert_eq!(ranges.len(), 2);
        assert_eq!((ranges[0].start_ms, ranges[0].end_ms), (10_000, 20_000));
        assert_eq!((ranges[1].start_ms, ranges[1].end_ms), (30_000, 40_000));
    }

    #[test]
    fn resolve_ranges_missing_marks_only_the_tail_range_open_ended() {
        let segments = vec![
            gap_row(0, 0.0, 10.0),
            text_row(1, 10.0, 20.0),
            gap_row(2, 20.0, 30.0),
        ];
        let ranges = resolve_ranges(RerunScope::Missing, &segments, 30_000).unwrap();
        assert_eq!(ranges.len(), 2);
        assert_ne!(
            ranges[0].end_sample,
            u64::MAX,
            "vùng không chạm cuối bản ghi phải giữ end_sample hữu hạn"
        );
        assert_eq!(
            ranges[1].end_sample,
            u64::MAX,
            "vùng cuối cùng chạm hết bản ghi phải open-ended"
        );
    }

    #[test]
    fn resolve_ranges_missing_on_a_complete_transcript_is_empty() {
        let segments = vec![text_row(0, 0.0, 10.0)];
        assert!(resolve_ranges(RerunScope::Missing, &segments, 10_000)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn resolve_ranges_gap_finds_the_exact_idx() {
        let segments = vec![text_row(0, 0.0, 10.0), gap_row(1, 10.0, 20.0)];
        let ranges = resolve_ranges(RerunScope::Gap { gap_id: 1 }, &segments, 20_000).unwrap();
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start_ms, ranges[0].end_ms), (10_000, 20_000));
        // This gap reaches the end of the (20 s) recording -- open-ended.
        assert_eq!(ranges[0].end_sample, u64::MAX);
    }

    #[test]
    fn resolve_ranges_gap_not_reaching_the_end_stays_closed() {
        let segments = vec![
            text_row(0, 0.0, 10.0),
            gap_row(1, 10.0, 20.0),
            text_row(2, 20.0, 30.0),
        ];
        let ranges = resolve_ranges(RerunScope::Gap { gap_id: 1 }, &segments, 30_000).unwrap();
        assert_eq!(ranges, vec![range_from_sec(10.0, 20.0)]);
    }

    #[test]
    fn resolve_ranges_gap_rejects_a_missing_or_non_gap_or_disconnected_idx() {
        let segments = vec![
            text_row(0, 0.0, 10.0),
            SegmentRow {
                idx: 1,
                start_sec: 10.0,
                end_sec: 20.0,
                kind: SegmentKind::Gap,
                gap_reason: Some(GapReason::Disconnected),
                text: String::new(),
                speaker: None,
            },
        ];
        assert_eq!(
            resolve_ranges(RerunScope::Gap { gap_id: 0 }, &segments, 20_000)
                .unwrap_err()
                .code,
            Code::Request
        );
        assert_eq!(
            resolve_ranges(RerunScope::Gap { gap_id: 1 }, &segments, 20_000)
                .unwrap_err()
                .code,
            Code::Request,
            "gap `disconnected` không phải mục tiêu Chạy lại hợp lệ"
        );
        assert_eq!(
            resolve_ranges(RerunScope::Gap { gap_id: 99 }, &segments, 20_000)
                .unwrap_err()
                .code,
            Code::Request
        );
    }

    fn wav_fixture(dir: &Path, name: &str, seconds: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..(16_000 * seconds) {
            let sample = ((frame as f32 * 0.05).sin() * 5_000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    #[test]
    fn decode_ranges_and_chunk_routes_samples_into_the_right_range_starting_at_zero() {
        let dir = tempfile::tempdir().unwrap();
        let source = wav_fixture(dir.path(), "a.wav", 3);

        let ranges = vec![range_from_ms(0, 1_000), range_from_ms(2_000, 3_000)];
        let mut seen: Vec<(usize, u64, u64)> = Vec::new();
        decode_ranges_and_chunk(
            &source,
            &ranges,
            ChunkOptions::default(),
            &CancellationToken::new(),
            |range_index, chunk| {
                seen.push((range_index, chunk.start_sample, chunk.sample_count));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(seen.len(), 2, "mỗi vùng 1 giây phải ra đúng một Chunk");
        assert_eq!(seen[0].0, 0);
        assert_eq!(
            seen[0].1, 0,
            "Chunker của vùng phải bắt đầu từ 0, chưa dời tuyệt đối"
        );
        assert_eq!(seen[0].2, 16_000);
        assert_eq!(seen[1].0, 1);
        assert_eq!(seen[1].1, 0);
        assert_eq!(seen[1].2, 16_000);
    }

    #[test]
    fn decode_ranges_and_chunk_on_the_whole_file_emits_one_chunk_per_range_of_all() {
        let dir = tempfile::tempdir().unwrap();
        let source = wav_fixture(dir.path(), "a.wav", 2);

        let ranges = vec![range_from_ms(0, 2_000)];
        let mut total_samples = 0u64;
        decode_ranges_and_chunk(
            &source,
            &ranges,
            ChunkOptions::default(),
            &CancellationToken::new(),
            |range_index, chunk| {
                assert_eq!(range_index, 0);
                total_samples += chunk.sample_count;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(total_samples, 32_000);
    }

    fn wav_fixture_samples(dir: &Path, name: &str, sample_count: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for frame in 0..sample_count {
            let sample = ((frame as f32 * 0.05).sin() * 5_000.0) as i16;
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        path
    }

    /// spec I/O Matrix "Rerun all, proxy longer": the range's nominal end (2 s
    /// / 2000 ms, as `total_duration_ms` reported it) is shorter than the
    /// Proxy's real decoded length (2.5 s) -- an open-ended range
    /// (`end_sample = u64::MAX`) must still consume every real sample to the
    /// Proxy's true end, not stop short at the nominal boundary.
    #[test]
    fn decode_ranges_and_chunk_routes_every_sample_to_an_open_ended_tail_range_past_its_nominal_end(
    ) {
        let dir = tempfile::tempdir().unwrap();
        // 2.5 s of real audio (40_000 samples at 16 kHz).
        let source = wav_fixture_samples(dir.path(), "a.wav", 40_000);

        let mut declared = range_from_ms(0, 2_000);
        declared.end_sample = u64::MAX;
        let ranges = vec![declared];

        let mut total_samples = 0u64;
        decode_ranges_and_chunk(
            &source,
            &ranges,
            ChunkOptions::default(),
            &CancellationToken::new(),
            |range_index, chunk| {
                assert_eq!(range_index, 0);
                total_samples += chunk.sample_count;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            total_samples, 40_000,
            "vùng open-ended phải nhận hết mẫu thật tới cuối Proxy, kể cả vượt end_ms danh nghĩa"
        );
    }
}
