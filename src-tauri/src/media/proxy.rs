use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use uuid::Uuid;

use super::{
    decode::{decode_mono_16khz, DecodeStats},
    flac::StreamingFlacWriter,
    hash::sha256_file,
    storage_error,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyInfo {
    pub path: PathBuf,
    pub source_sha256: String,
    pub sample_count: u64,
    pub duration_seconds: f64,
}

/// Creates a standalone 16 kHz mono FLAC proxy in the caller-provided session
/// media directory. The caller uses `core::paths::media_dir` to place it under
/// `<app-data>/media/<session-id>`; this module does not know about sessions.
/// Only the current encoded FLAC frame is held while writing the proxy.
pub fn create_proxy(
    media_directory: &Path,
    source_path: &Path,
) -> Result<ProxyInfo, crate::core::error::AppError> {
    let source_sha256 = sha256_file(source_path)?;
    fs::create_dir_all(media_directory).map_err(|_| storage_error())?;

    let unique_id = Uuid::now_v7();
    let final_path = media_directory.join(format!("proxy-{unique_id}.flac"));
    let temporary_path = media_directory.join(format!("proxy-{unique_id}.flac.partial"));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|_| storage_error())?;

    let result = write_proxy(file, source_path);
    let (sample_count, duration_seconds) = match result {
        Ok(stats) => stats,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
    };

    fs::rename(&temporary_path, &final_path).map_err(|_| {
        let _ = fs::remove_file(&temporary_path);
        storage_error()
    })?;

    Ok(ProxyInfo {
        path: final_path,
        source_sha256,
        sample_count,
        duration_seconds,
    })
}

fn write_proxy(file: File, source_path: &Path) -> Result<(u64, f64), crate::core::error::AppError> {
    let mut encoder = StreamingFlacWriter::new(file)?;
    let stats: DecodeStats = decode_mono_16khz(source_path, |samples| encoder.push(samples))?;
    let (_file, sample_count) = encoder.finish()?;
    Ok((sample_count, stats.duration_seconds))
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use crate::core::{id::SessionId, paths::media_dir};

    use super::*;

    #[test]
    fn proxy_is_a_standalone_flac_under_the_session_media_directory() {
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir.path().join("source.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&source_path, spec).unwrap();
        for frame in 0..48_000_i32 {
            let sample = ((frame as f32 * 0.01).sin() * i16::MAX as f32) as i16;
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        let source_info = crate::media::probe(&source_path).unwrap();
        assert_eq!(source_info.sample_rate, 48_000);
        assert_eq!(source_info.channels, 2);
        assert_eq!(source_info.frame_count, 48_000);
        assert!((source_info.duration_seconds - 1.0).abs() < 0.01);

        let app_data_root = source_dir.path().join("app-data");
        let media_root = media_dir(&app_data_root, SessionId::new());
        let proxy = create_proxy(&media_root, &source_path).unwrap();
        assert!(proxy.path.starts_with(&media_root));
        assert_eq!(proxy.sample_count, 16_000);
        assert!((proxy.duration_seconds - 1.0).abs() < 0.01);
        assert_eq!(proxy.source_sha256.len(), 64);

        let mut file = File::open(&proxy.path).unwrap();
        let mut header = [0_u8; 4];
        file.read_exact(&mut header).unwrap();
        assert_eq!(&header, b"fLaC");
        let proxy_info = crate::media::probe(&proxy.path).unwrap();
        assert_eq!(proxy_info.sample_rate, 16_000);
        assert_eq!(proxy_info.channels, 1);
        assert_eq!(proxy_info.frame_count, 16_000);
        let decoded = crate::media::decode_mono_16khz(&proxy.path, |_| Ok(())).unwrap();
        assert_eq!(decoded.output_frame_count, 16_000);
    }

    #[test]
    fn proxy_handles_one_through_fifteen_output_samples() {
        let root = tempfile::tempdir().unwrap();

        for expected_samples in 1..=15_u64 {
            let source_path = root.path().join(format!("short-{expected_samples}.wav"));
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(&source_path, spec).unwrap();
            for frame in 0..expected_samples * 3 {
                let sample = if frame % 2 == 0 {
                    10_000_i16
                } else {
                    -10_000_i16
                };
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();

            let media_directory = root.path().join("media");
            let proxy = create_proxy(&media_directory, &source_path).unwrap();
            assert_eq!(
                proxy.sample_count, expected_samples,
                "proxy sample count for {expected_samples}-sample input"
            );
            let info = crate::media::probe(&proxy.path).unwrap();
            assert_eq!(
                info.frame_count, expected_samples,
                "FLAC STREAMINFO count for {expected_samples}-sample input"
            );
            let decoded = crate::media::decode_mono_16khz(&proxy.path, |_| Ok(())).unwrap();
            assert_eq!(
                decoded.output_frame_count, expected_samples,
                "decoded count for {expected_samples}-sample input"
            );
        }
    }
}
