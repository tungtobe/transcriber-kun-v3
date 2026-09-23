use std::io::{Seek, SeekFrom, Write};

use flacenc::{
    bitsink::ByteSink,
    component::{BitRepr, Frame, Stream, StreamInfo},
    config::Encoder,
    error::Verify,
    source::{Context, Fill, FrameBuf, MemSource},
};

use super::{format_error, storage_error, OUTPUT_SAMPLE_RATE};

const FLAC_BITS_PER_SAMPLE: usize = 16;
const FLAC_BLOCK_SIZE: usize = 4096;
const FLAC_MIN_BLOCK_SIZE: usize = 16;

pub(super) fn encode_flac(samples: &[f32]) -> Result<Vec<u8>, crate::core::error::AppError> {
    if samples.is_empty() {
        return Err(format_error("No audio samples are available to encode."));
    }
    let quantized = samples
        .iter()
        .copied()
        .map(quantize_f32_to_i16)
        .collect::<Vec<_>>();
    let mut config = Encoder::default();
    config.multithread = false;
    let block_size = config.block_size;
    let config = config
        .into_verified()
        .map_err(|_| format_error("The FLAC encoder configuration is invalid."))?;
    let source = MemSource::from_samples(
        &quantized,
        1,
        FLAC_BITS_PER_SAMPLE,
        OUTPUT_SAMPLE_RATE as usize,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, block_size)
        .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
    // Fixed-block streams describe the configured block size in STREAMINFO even
    // when the final frame is shorter than that size.
    stream
        .stream_info_mut()
        .set_block_sizes(block_size, block_size)
        .map_err(|_| format_error("The FLAC stream metadata is invalid."))?;
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
    Ok(sink.as_slice().to_vec())
}

pub(super) fn quantize_f32_to_i16(sample: f32) -> i32 {
    let sample = if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    if sample <= -1.0 {
        i16::MIN as i32
    } else {
        (sample * i16::MAX as f32).round() as i32
    }
}

/// Writes one continuous FLAC stream while retaining only one encoder block.
/// STREAMINFO is initially written with unknown totals, then finalized in place
/// after the sample count and FLAC MD5 are known.
pub(super) struct StreamingFlacWriter<W: Write + Seek> {
    writer: W,
    config: flacenc::error::Verified<Encoder>,
    stream_info: StreamInfo,
    context: Context,
    pending: Vec<i32>,
    sample_count: u64,
    frame_count: usize,
}

impl<W: Write + Seek> StreamingFlacWriter<W> {
    pub(super) fn new(mut writer: W) -> Result<Self, crate::core::error::AppError> {
        let mut encoder = Encoder::default();
        encoder.multithread = false;
        encoder.block_size = FLAC_BLOCK_SIZE;
        let config = encoder
            .into_verified()
            .map_err(|_| format_error("The FLAC encoder configuration is invalid."))?;
        let stream_info = StreamInfo::new(OUTPUT_SAMPLE_RATE as usize, 1, FLAC_BITS_PER_SAMPLE)
            .map_err(|_| format_error("The FLAC stream configuration is invalid."))?;
        write_stream_header(&mut writer, &stream_info)?;

        Ok(Self {
            writer,
            config,
            stream_info,
            context: Context::new(FLAC_BITS_PER_SAMPLE, 1),
            pending: Vec::with_capacity(FLAC_BLOCK_SIZE),
            sample_count: 0,
            frame_count: 0,
        })
    }

    pub(super) fn push(&mut self, samples: &[f32]) -> Result<(), crate::core::error::AppError> {
        for sample in samples {
            self.pending.push(quantize_f32_to_i16(*sample));
            self.sample_count = self.sample_count.saturating_add(1);
            if self.pending.len() == FLAC_BLOCK_SIZE {
                self.write_frame(false)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(W, u64), crate::core::error::AppError> {
        if self.sample_count == 0 {
            return Err(format_error("No playable audio frames were found."));
        }
        if !self.pending.is_empty() {
            self.write_frame(true)?;
        }

        self.stream_info
            .set_total_samples(self.sample_count as usize);
        self.stream_info.set_md5_digest(&self.context.md5_digest());
        self.stream_info
            .set_block_sizes(FLAC_BLOCK_SIZE, FLAC_BLOCK_SIZE)
            .map_err(|_| format_error("The FLAC stream metadata is invalid."))?;
        let end = self
            .writer
            .seek(SeekFrom::End(0))
            .map_err(|_| storage_error())?;
        self.writer
            .seek(SeekFrom::Start(0))
            .map_err(|_| storage_error())?;
        write_stream_header(&mut self.writer, &self.stream_info)?;
        self.writer
            .seek(SeekFrom::Start(end))
            .map_err(|_| storage_error())?;
        self.writer.flush().map_err(|_| storage_error())?;
        Ok((self.writer, self.sample_count))
    }

    fn write_frame(&mut self, final_frame: bool) -> Result<(), crate::core::error::AppError> {
        let actual_len = self.pending.len();
        let mut frame_samples = std::mem::take(&mut self.pending);
        if final_frame && frame_samples.len() < FLAC_MIN_BLOCK_SIZE {
            frame_samples.resize(FLAC_MIN_BLOCK_SIZE, 0);
        }
        let mut frame_buffer = FrameBuf::with_size(1, FLAC_BLOCK_SIZE)
            .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
        frame_buffer
            .fill_interleaved(&frame_samples)
            .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
        let actual_samples = &frame_samples[..actual_len];
        self.context
            .fill_interleaved(actual_samples)
            .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
        let frame_number = self.context.current_frame_number().unwrap_or(0);
        let frame: Frame = flacenc::encode_fixed_size_frame(
            &self.config,
            &frame_buffer,
            frame_number,
            &self.stream_info,
        )
        .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
        self.stream_info.update_frame_info(&frame);
        let mut sink = ByteSink::new();
        frame
            .write(&mut sink)
            .map_err(|_| format_error("The audio could not be encoded as FLAC."))?;
        self.writer
            .write_all(sink.as_slice())
            .map_err(|_| storage_error())?;
        self.frame_count += 1;
        self.pending = Vec::with_capacity(FLAC_BLOCK_SIZE);
        Ok(())
    }
}

fn write_stream_header<W: Write>(
    writer: &mut W,
    stream_info: &StreamInfo,
) -> Result<(), crate::core::error::AppError> {
    let stream = Stream::with_stream_info(stream_info.clone());
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|_| format_error("The FLAC stream header could not be written."))?;
    writer
        .write_all(sink.as_slice())
        .map_err(|_| storage_error())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn quantizer_clamps_and_replaces_non_finite_samples() {
        assert_eq!(quantize_f32_to_i16(-2.0), i16::MIN as i32);
        assert_eq!(quantize_f32_to_i16(2.0), i16::MAX as i32);
        assert_eq!(quantize_f32_to_i16(f32::NAN), 0);
    }

    #[test]
    fn streaming_encoder_writes_a_flac_header_and_frame_data() {
        let writer = Cursor::new(Vec::new());
        let mut encoder = StreamingFlacWriter::new(writer).unwrap();
        encoder.push(&vec![0.0; 997]).unwrap();
        let (writer, sample_count) = encoder.finish().unwrap();
        assert_eq!(sample_count, 997);
        assert!(writer.get_ref().starts_with(b"fLaC"));
        assert!(writer.get_ref().len() > 42);
    }

    #[test]
    fn chunk_flac_is_readable_by_the_streaming_decoder() {
        let bytes = encode_flac(&vec![0.0; 16_000]).unwrap();
        let mut file = tempfile::NamedTempFile::with_suffix(".flac").unwrap();
        file.write_all(&bytes).unwrap();
        let info = crate::media::probe(file.path()).unwrap();
        assert_eq!(info.sample_rate, OUTPUT_SAMPLE_RATE);
        assert_eq!(info.frame_count, 16_000);
    }
}
