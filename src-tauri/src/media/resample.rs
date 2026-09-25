use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, FixedSync, Resampler};

use super::{format_error, OUTPUT_SAMPLE_RATE};

const RESAMPLER_INPUT_CHUNK: usize = 4096;

/// Bound on the zero-padded flush loop in [`MonoResampler::finish`] (spec
/// Boundaries Always P0 review: "flush loop is bounded; if the bound is hit
/// without reaching `expected`, return `format_error`, never loop forever").
/// Each flush block can, in principle, emit zero usable output frames (every
/// frame trimmed away by `delay_remaining`/`allowed`); without a bound that
/// would hang the Job pipeline forever instead of failing it. 64 zero blocks
/// is already many times more than any real sample-rate ratio should ever
/// need to drain the resampler's internal delay line.
const MAX_FLUSH_BLOCKS: u32 = 64;

/// Stateful mono resampler. It retains only one input block and Rubato's
/// bounded filter state, then emits samples after trimming startup delay.
pub(super) struct MonoResampler {
    input_rate: u32,
    resampler: Option<Fft<f32>>,
    pending_input: Vec<f32>,
    input_frames_seen: u64,
    output_frames_emitted: u64,
    delay_remaining: u64,
}

impl MonoResampler {
    pub(super) fn new(input_rate: u32) -> Result<Self, crate::core::error::AppError> {
        if input_rate == 0 {
            return Err(format_error("The audio sample rate is missing."));
        }

        let resampler = if input_rate == OUTPUT_SAMPLE_RATE {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    input_rate as usize,
                    OUTPUT_SAMPLE_RATE as usize,
                    RESAMPLER_INPUT_CHUNK,
                    1,
                    FixedSync::Input,
                )
                .map_err(|_| format_error("The audio sample rate cannot be resampled."))?,
            )
        };
        let delay_remaining = resampler
            .as_ref()
            .map_or(0, |value| value.output_delay() as u64);

        Ok(Self {
            input_rate,
            resampler,
            pending_input: Vec::new(),
            input_frames_seen: 0,
            output_frames_emitted: 0,
            delay_remaining,
        })
    }

    pub(super) fn push<F>(
        &mut self,
        input: &[f32],
        emit: &mut F,
    ) -> Result<(), crate::core::error::AppError>
    where
        F: FnMut(&[f32]) -> Result<(), crate::core::error::AppError>,
    {
        if input.is_empty() {
            return Ok(());
        }
        self.input_frames_seen = self.input_frames_seen.saturating_add(input.len() as u64);

        if self.resampler.is_none() {
            self.output_frames_emitted = self
                .output_frames_emitted
                .saturating_add(input.len() as u64);
            return emit(input);
        }

        self.pending_input.extend_from_slice(input);
        loop {
            let block_size = self
                .resampler
                .as_ref()
                .expect("resampler checked above")
                .input_frames_next();
            if self.pending_input.len() < block_size {
                break;
            }
            let block = self.pending_input.drain(..block_size).collect::<Vec<_>>();
            self.process_block(&block, None, emit)?;
        }
        Ok(())
    }

    pub(super) fn finish<F>(&mut self, emit: &mut F) -> Result<u64, crate::core::error::AppError>
    where
        F: FnMut(&[f32]) -> Result<(), crate::core::error::AppError>,
    {
        if self.input_frames_seen == 0 {
            return Err(format_error("No playable audio frames were found."));
        }

        let Some(resampler) = self.resampler.as_ref() else {
            return Ok(self.output_frames_emitted);
        };

        let block_size = resampler.input_frames_next();
        if !self.pending_input.is_empty() {
            let partial_len = self.pending_input.len();
            let mut block = vec![0.0; block_size];
            block[..partial_len].copy_from_slice(&self.pending_input);
            self.process_block(&block, Some(partial_len), emit)?;
            self.pending_input.clear();
        }

        let expected = ((u128::from(self.input_frames_seen) * u128::from(OUTPUT_SAMPLE_RATE))
            .div_ceil(u128::from(self.input_rate))) as u64;
        let mut flush_blocks = 0u32;
        while self.output_frames_emitted < expected {
            if flush_blocks >= MAX_FLUSH_BLOCKS {
                return Err(format_error(
                    "The audio resampler did not converge while flushing.",
                ));
            }
            flush_blocks += 1;
            let zeros = vec![0.0; block_size];
            self.process_block(&zeros, Some(0), emit)?;
        }
        Ok(self.output_frames_emitted)
    }

    fn process_block<F>(
        &mut self,
        block: &[f32],
        partial_len: Option<usize>,
        emit: &mut F,
    ) -> Result<(), crate::core::error::AppError>
    where
        F: FnMut(&[f32]) -> Result<(), crate::core::error::AppError>,
    {
        let adapter = InterleavedSlice::new(block, 1, block.len())
            .map_err(|_| format_error("The audio stream could not be resampled."))?;
        let indexing = partial_len.map(|len| rubato::Indexing::new().partial_len(len));
        let output = self
            .resampler
            .as_mut()
            .expect("process_block requires a resampler")
            .process(&adapter, indexing.as_ref())
            .map_err(|_| format_error("The audio stream could not be resampled."))?
            .take_data();

        let skip = self.delay_remaining.min(output.len() as u64) as usize;
        self.delay_remaining -= skip as u64;
        let expected_so_far = ((u128::from(self.input_frames_seen)
            * u128::from(OUTPUT_SAMPLE_RATE))
        .div_ceil(u128::from(self.input_rate))) as u64;
        let allowed = expected_so_far.saturating_sub(self.output_frames_emitted) as usize;
        let usable = &output[skip..skip.saturating_add(allowed).min(output.len())];
        if !usable.is_empty() {
            self.output_frames_emitted = self
                .output_frames_emitted
                .saturating_add(usable.len() as u64);
            emit(usable)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // P0 review fix: `finish`'s zero-padded flush loop is now bounded by
    // `MAX_FLUSH_BLOCKS` instead of looping forever if the resampler never
    // reaches `expected` (spec Tasks: "bounded flush, plus a unit test if
    // feasible (otherwise document)"). A unit test that reliably drives a
    // real `rubato::Fft` resampler past 64 zero-padded blocks without ever
    // converging isn't feasible here without adding a second, injectable
    // resampler seam purely for the test (out of scope for this
    // smallest-local-change fix, and rubato's own filter design makes such a
    // non-convergent ratio hard to construct deterministically). The two
    // tests below instead cover that the bound doesn't change behavior for
    // any real, converging ratio (pass-through and a resampled ratio both
    // still finish and emit the expected frame count); the bound itself was
    // verified manually by lowering `MAX_FLUSH_BLOCKS` to `0` locally and
    // confirming `finish` then returns a `format_error` instead of hanging.
    #[test]
    fn pass_through_at_target_rate_preserves_samples_and_duration() {
        let input = vec![0.25_f32; 1024];
        let mut output = Vec::new();
        let mut emit = |samples: &[f32]| {
            output.extend_from_slice(samples);
            Ok(())
        };
        let mut resampler = MonoResampler::new(OUTPUT_SAMPLE_RATE).unwrap();
        resampler.push(&input, &mut emit).unwrap();
        assert_eq!(resampler.finish(&mut emit).unwrap(), input.len() as u64);
        assert_eq!(output, input);
    }

    #[test]
    fn resampling_trims_startup_delay_and_emits_expected_frame_count() {
        let input = vec![0.0_f32; 44_100];
        let mut output_len = 0_usize;
        let mut emit = |samples: &[f32]| {
            output_len += samples.len();
            Ok(())
        };
        let mut resampler = MonoResampler::new(44_100).unwrap();
        for block in input.chunks(997) {
            resampler.push(block, &mut emit).unwrap();
        }
        let emitted = resampler.finish(&mut emit).unwrap();
        assert_eq!(emitted, 16_000);
        assert_eq!(output_len, 16_000);
    }
}
