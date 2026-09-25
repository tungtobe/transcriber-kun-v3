use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;

use super::{flac::encode_flac, format_error, OUTPUT_SAMPLE_RATE};
use crate::gemini::params::MAX_TRANSCRIBE_REQUEST_BYTES;

const DEFAULT_CHUNK_SECONDS: u64 = 5 * 60;
// Leave room for the method, schema, prompt, and content framing added by the
// Gemini adapter. The final request builder still checks its exact JSON size.
const TRANSCRIBE_REQUEST_FRAMING_RESERVE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkBudget {
    pub max_flac_bytes: usize,
    pub max_serialized_payload_bytes: usize,
}

impl Default for ChunkBudget {
    fn default() -> Self {
        Self {
            max_flac_bytes: 14 * 1024 * 1024,
            // The serialized Chunk contributes most of the inline request.
            // Reserve framing space here, then check the actual request JSON
            // again at the adapter boundary.
            max_serialized_payload_bytes: MAX_TRANSCRIBE_REQUEST_BYTES
                - TRANSCRIBE_REQUEST_FRAMING_RESERVE_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkOptions {
    pub max_duration_seconds: u64,
    pub budget: ChunkBudget,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_duration_seconds: DEFAULT_CHUNK_SECONDS,
            budget: ChunkBudget::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub start_sample: u64,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub sample_count: u64,
    pub mime_type: &'static str,
    pub flac_base64: String,
}

/// Serialize the exact JSON request body and enforce the Gemini inline limit.
/// Use this after inserting a [`Chunk`] into its final model-specific request;
/// measuring the Chunk alone omits the schema and prompt framing.
pub fn serialize_transcribe_request<T: Serialize>(
    request: &T,
) -> Result<String, crate::core::error::AppError> {
    let body = serde_json::to_string(request)
        .map_err(|_| format_error("The audio request could not be validated."))?;
    if body.len() >= MAX_TRANSCRIBE_REQUEST_BYTES {
        return Err(format_error(
            "The serialized audio request exceeds Gemini's inline size limit.",
        ));
    }
    Ok(body)
}

/// Bounds working PCM to at most one configured chunk while emitting FLAC
/// payloads as soon as they are ready.
pub struct Chunker {
    options: ChunkOptions,
    max_samples: usize,
    next_start_sample: u64,
    pending: Vec<f32>,
}

impl Chunker {
    pub fn new(options: ChunkOptions) -> Result<Self, crate::core::error::AppError> {
        if options.max_duration_seconds == 0
            || options.budget.max_flac_bytes == 0
            || options.budget.max_serialized_payload_bytes == 0
        {
            return Err(format_error("The FLAC chunk size limit is invalid."));
        }
        let max_samples = options
            .max_duration_seconds
            .saturating_mul(u64::from(OUTPUT_SAMPLE_RATE))
            .min(usize::MAX as u64) as usize;
        if max_samples == 0 {
            return Err(format_error("The FLAC chunk size limit is invalid."));
        }
        Ok(Self {
            options,
            max_samples,
            next_start_sample: 0,
            pending: Vec::with_capacity(initial_capacity(max_samples)),
        })
    }

    pub fn push<F>(
        &mut self,
        samples: &[f32],
        emit: &mut F,
    ) -> Result<(), crate::core::error::AppError>
    where
        F: FnMut(Chunk) -> Result<(), crate::core::error::AppError>,
    {
        let mut offset = 0;
        while offset < samples.len() {
            let remaining = self.max_samples - self.pending.len();
            let take = remaining.min(samples.len() - offset);
            self.pending
                .extend_from_slice(&samples[offset..offset + take]);
            offset += take;
            if self.pending.len() == self.max_samples {
                self.flush(emit)?;
            }
        }
        Ok(())
    }

    pub fn finish<F>(&mut self, emit: &mut F) -> Result<(), crate::core::error::AppError>
    where
        F: FnMut(Chunk) -> Result<(), crate::core::error::AppError>,
    {
        if !self.pending.is_empty() {
            self.flush(emit)?;
        }
        Ok(())
    }

    fn flush<F>(&mut self, emit: &mut F) -> Result<(), crate::core::error::AppError>
    where
        F: FnMut(Chunk) -> Result<(), crate::core::error::AppError>,
    {
        let samples = std::mem::take(&mut self.pending);
        let start_sample = self.next_start_sample;
        let sample_count = samples.len() as u64;
        emit_bounded_chunks(&samples, start_sample, self.options.budget, emit)?;
        self.next_start_sample = self.next_start_sample.saturating_add(sample_count);
        self.pending = Vec::with_capacity(initial_capacity(self.max_samples));
        Ok(())
    }
}

/// Caps the up-front allocation for `pending` at one default chunk's worth of
/// samples (300 s x 16 kHz), regardless of how large `max_samples` actually
/// is (spec Boundaries Always P0 review: "`Chunker::new` never pre-allocates
/// more than the default chunk of capacity, whatever `max_duration_seconds`
/// is"). `max_samples` itself is untouched -- this only bounds the initial
/// `Vec` capacity; `push`/`extend_from_slice` still grow it on demand for a
/// genuinely large configured chunk. Defense in depth: `chunkMinutes` is
/// already validated to `1..=60` by `settings::save`/`load`, but this keeps a
/// corrupted or future caller from turning a huge duration into an
/// OOM-aborting allocation here.
fn initial_capacity(max_samples: usize) -> usize {
    let default_chunk_samples = (DEFAULT_CHUNK_SECONDS as usize)
        .saturating_mul(OUTPUT_SAMPLE_RATE as usize);
    max_samples.min(default_chunk_samples)
}

fn emit_bounded_chunks<F>(
    samples: &[f32],
    start_sample: u64,
    budget: ChunkBudget,
    emit: &mut F,
) -> Result<(), crate::core::error::AppError>
where
    F: FnMut(Chunk) -> Result<(), crate::core::error::AppError>,
{
    if samples.is_empty() {
        return Ok(());
    }
    let flac = encode_flac(samples)?;
    let chunk = make_chunk(start_sample, samples.len() as u64, &flac);
    let json_bytes = serde_json::to_vec(&chunk)
        .map_err(|_| format_error("The audio payload could not be validated."))?
        .len();
    if flac.len() <= budget.max_flac_bytes && json_bytes <= budget.max_serialized_payload_bytes {
        return emit(chunk);
    }
    if samples.len() <= 1 {
        return Err(format_error(
            "The audio payload cannot be split within the configured size limit.",
        ));
    }

    let split_at = samples.len() / 2;
    emit_bounded_chunks(&samples[..split_at], start_sample, budget, emit)?;
    emit_bounded_chunks(
        &samples[split_at..],
        start_sample.saturating_add(split_at as u64),
        budget,
        emit,
    )
}

fn make_chunk(start_sample: u64, sample_count: u64, flac: &[u8]) -> Chunk {
    let start_ms = sample_to_milliseconds(start_sample);
    let end_ms = sample_to_milliseconds(start_sample.saturating_add(sample_count));
    Chunk {
        start_sample,
        start_ms,
        duration_ms: end_ms.saturating_sub(start_ms),
        sample_count,
        mime_type: "audio/flac",
        flac_base64: STANDARD.encode(flac),
    }
}

fn sample_to_milliseconds(sample: u64) -> u64 {
    (u128::from(sample) * 1000 / u128::from(OUTPUT_SAMPLE_RATE)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_use_absolute_contiguous_offsets_and_default_to_five_minutes() {
        assert_eq!(ChunkOptions::default().max_duration_seconds, 300);
        let mut chunker = Chunker::new(ChunkOptions {
            max_duration_seconds: 1,
            budget: ChunkBudget::default(),
        })
        .unwrap();
        let input = vec![0.1_f32; 16_001];
        let mut chunks = Vec::new();
        let mut emit = |chunk| {
            chunks.push(chunk);
            Ok(())
        };
        chunker.push(&input, &mut emit).unwrap();
        chunker.finish(&mut emit).unwrap();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start_sample, 0);
        assert_eq!(chunks[0].sample_count, 16_000);
        assert_eq!(chunks[1].start_sample, 16_000);
        assert_eq!(chunks[1].start_ms, 1_000);
        assert_eq!(chunks[1].sample_count, 1);
    }

    #[test]
    fn encoded_chunks_respect_both_byte_budgets_after_base64_and_json() {
        let samples = (0..8192)
            .map(|index| if index % 2 == 0 { -0.91 } else { 0.87 })
            .collect::<Vec<_>>();
        let budget = ChunkBudget {
            max_flac_bytes: 1800,
            max_serialized_payload_bytes: 2600,
        };
        let mut chunks = Vec::new();
        emit_bounded_chunks(&samples, 32_000, budget, &mut |chunk| {
            assert!(
                base64::engine::general_purpose::STANDARD
                    .decode(&chunk.flac_base64)
                    .unwrap()
                    .len()
                    <= budget.max_flac_bytes
            );
            assert!(
                serde_json::to_vec(&chunk).unwrap().len() <= budget.max_serialized_payload_bytes
            );
            chunks.push(chunk);
            Ok(())
        })
        .unwrap();
        assert!(chunks.len() > 1);
        assert_eq!(chunks.first().unwrap().start_sample, 32_000);
        for pair in chunks.windows(2) {
            assert_eq!(
                pair[0].start_sample + pair[0].sample_count,
                pair[1].start_sample
            );
        }
    }

    /// P0 review fix: a huge `max_duration_seconds` (e.g. a corrupted
    /// `chunkMinutes` that bypassed `settings` validation) must not make
    /// `Chunker::new` try to pre-allocate that many samples up front -- it
    /// must succeed cheaply instead of aborting the process on an OOM.
    #[test]
    fn a_huge_max_duration_does_not_try_to_preallocate_it_all() {
        let chunker = Chunker::new(ChunkOptions {
            max_duration_seconds: 10_000_000_000,
            budget: ChunkBudget::default(),
        })
        .unwrap();
        assert_eq!(
            chunker.pending.capacity(),
            DEFAULT_CHUNK_SECONDS as usize * OUTPUT_SAMPLE_RATE as usize
        );
    }

    #[test]
    fn impossible_budget_returns_format_error_instead_of_oversize_payload() {
        let result = emit_bounded_chunks(
            &[0.0],
            0,
            ChunkBudget {
                max_flac_bytes: 1,
                max_serialized_payload_bytes: usize::MAX,
            },
            &mut |_| Ok(()),
        );
        assert_eq!(
            result.unwrap_err().category,
            crate::core::error::Category::Format
        );
    }
}
