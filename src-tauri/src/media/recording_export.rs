use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use uuid::Uuid;

use super::{decode_mono_16khz, flac::StreamingFlacWriter};

fn storage_error() -> crate::core::error::AppError {
    crate::core::error::AppError::new(
        crate::core::error::Code::Storage,
        "Could not save the Recording. Check folder permissions and free disk space, then try again.",
    )
}

const COPY_BLOCK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingExportFormat {
    Wav,
    Flac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingExportOutcome {
    Saved,
    Cancelled,
}

/// Copy a canonical Recording WAV or encode it as FLAC without retaining the
/// complete audio in memory. Output is built in a unique partial file and is
/// published only after the copy/encode and flush succeed. When `staging_dir`
/// (under the app data dir, swept at boot) is on the same volume as the
/// destination the partial lives there, so a crash cannot leave litter next to
/// the user's file; otherwise a sibling file is used. Every error path removes
/// the partial.
pub fn export_recording(
    source: &Path,
    destination: &Path,
    staging_dir: Option<&Path>,
    format: RecordingExportFormat,
    duration_seconds: f64,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<RecordingExportOutcome, crate::core::error::AppError> {
    if same_file_or_path(source, destination) {
        return Err(crate::core::error::AppError::new(
            crate::core::error::Code::Request,
            "The export destination cannot be the source Recording.",
        ));
    }
    if is_cancelled(cancelled) {
        return Ok(RecordingExportOutcome::Cancelled);
    }

    let source_file = File::open(source).map_err(|_| storage_error())?;
    let total_bytes = source_file.metadata().map_err(|_| storage_error())?.len();
    if total_bytes == 0 {
        return Err(storage_error());
    }
    let temporary_path = temporary_path_for(destination, staging_dir)?;
    let mut staging = StagingFile::create(temporary_path)?;
    progress(0.0);

    let outcome = match format {
        RecordingExportFormat::Wav => copy_wav(
            source_file,
            &mut staging,
            total_bytes,
            duration_seconds,
            cancelled,
            &mut progress,
        )?,
        RecordingExportFormat::Flac => encode_flac(
            source,
            &mut staging,
            duration_seconds,
            cancelled,
            &mut progress,
        )?,
    };
    if outcome == RecordingExportOutcome::Cancelled || is_cancelled(cancelled) {
        return Ok(RecordingExportOutcome::Cancelled);
    }

    publish(&mut staging, destination)?;
    progress(duration_seconds.max(0.0));
    Ok(RecordingExportOutcome::Saved)
}

fn copy_wav(
    mut source: File,
    staging: &mut StagingFile,
    total_bytes: u64,
    duration_seconds: f64,
    cancelled: &AtomicBool,
    progress: &mut impl FnMut(f64),
) -> Result<RecordingExportOutcome, crate::core::error::AppError> {
    let mut buffer = [0_u8; COPY_BLOCK_SIZE];
    let mut copied = 0_u64;
    loop {
        if is_cancelled(cancelled) {
            return Ok(RecordingExportOutcome::Cancelled);
        }
        let count = source.read(&mut buffer).map_err(|_| storage_error())?;
        if count == 0 {
            break;
        }
        staging
            .file_mut()
            .write_all(&buffer[..count])
            .map_err(|_| storage_error())?;
        copied = copied.saturating_add(count as u64);
        // The canonical source is fixed-rate PCM, so the byte fraction tracks
        // elapsed recording duration without loading or decoding the WAV.
        let fraction = copied as f64 / total_bytes as f64;
        progress((duration_seconds.max(0.0) * fraction).min(duration_seconds.max(0.0)));
    }
    if is_cancelled(cancelled) {
        return Ok(RecordingExportOutcome::Cancelled);
    }
    staging.file_mut().sync_all().map_err(|_| storage_error())?;
    Ok(RecordingExportOutcome::Saved)
}

fn encode_flac(
    source: &Path,
    staging: &mut StagingFile,
    duration_seconds: f64,
    cancelled: &AtomicBool,
    progress: &mut impl FnMut(f64),
) -> Result<RecordingExportOutcome, crate::core::error::AppError> {
    let file = staging.take_file();
    let mut encoder = Some(StreamingFlacWriter::new(file)?);
    let mut was_cancelled = false;
    let mut processed_frames = 0_u64;
    let total_frames = (duration_seconds.max(0.0) * f64::from(super::OUTPUT_SAMPLE_RATE)).round();
    let decoded = decode_mono_16khz(source, |samples| {
        if is_cancelled(cancelled) {
            was_cancelled = true;
            return Err(crate::core::error::AppError::new(
                crate::core::error::Code::Blocked,
                "Recording export was cancelled.",
            ));
        }
        encoder
            .as_mut()
            .expect("encoder remains available until decoding completes")
            .push(samples)?;
        processed_frames = processed_frames.saturating_add(samples.len() as u64);
        let elapsed = processed_frames as f64 / f64::from(super::OUTPUT_SAMPLE_RATE);
        progress(if total_frames > 0.0 {
            elapsed.min(duration_seconds.max(0.0))
        } else {
            elapsed
        });
        if is_cancelled(cancelled) {
            was_cancelled = true;
            return Err(crate::core::error::AppError::new(
                crate::core::error::Code::Blocked,
                "Recording export was cancelled.",
            ));
        }
        Ok(())
    });
    if was_cancelled || is_cancelled(cancelled) {
        return Ok(RecordingExportOutcome::Cancelled);
    }
    decoded?;

    let (file, _sample_count) = encoder
        .take()
        .expect("encoder remains available after decoding")
        .finish()?;
    file.sync_all().map_err(|_| storage_error())?;
    if is_cancelled(cancelled) {
        return Ok(RecordingExportOutcome::Cancelled);
    }

    // The staging owner was moved into the encoder while decoding. Recover its
    // path guard before returning so every failure still removes the partial.
    staging.set_file(file);
    Ok(RecordingExportOutcome::Saved)
}

/// The partial file location: inside `staging_dir` when it is on the
/// destination's volume, else a sibling of the destination.
fn temporary_path_for(
    destination: &Path,
    staging_dir: Option<&Path>,
) -> Result<PathBuf, crate::core::error::AppError> {
    if let Some(dir) = staging_dir {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if fs::create_dir_all(dir).is_ok() && same_volume(dir, parent) {
            return Ok(dir.join(format!("{}.partial", Uuid::now_v7())));
        }
    }
    temporary_sibling(destination)
}

#[cfg(unix)]
fn same_volume(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev(),
        _ => false,
    }
}

#[cfg(windows)]
fn same_volume(a: &Path, b: &Path) -> bool {
    use std::path::Component;
    let prefix = |path: &Path| {
        fs::canonicalize(path).ok().and_then(|canonical| {
            canonical
                .components()
                .next()
                .and_then(|component| match component {
                    Component::Prefix(prefix) => Some(prefix.as_os_str().to_ascii_lowercase()),
                    _ => None,
                })
        })
    };
    matches!((prefix(a), prefix(b)), (Some(a), Some(b)) if a == b)
}

#[cfg(not(any(unix, windows)))]
fn same_volume(_a: &Path, _b: &Path) -> bool {
    false
}

fn temporary_sibling(destination: &Path) -> Result<PathBuf, crate::core::error::AppError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(storage_error)?;
    Ok(parent.join(format!(".{name}.{}.partial", Uuid::now_v7())))
}

struct StagingFile {
    path: PathBuf,
    file: Option<File>,
    published: bool,
}

impl StagingFile {
    fn create(path: PathBuf) -> Result<Self, crate::core::error::AppError> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| storage_error())?;
        Ok(Self {
            path,
            file: Some(file),
            published: false,
        })
    }

    fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("staging file is open")
    }

    fn take_file(&mut self) -> File {
        self.file.take().expect("staging file is open")
    }

    fn set_file(&mut self, file: File) {
        self.file = Some(file);
    }
}

impl Drop for StagingFile {
    fn drop(&mut self) {
        self.file.take();
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn publish(
    staging: &mut StagingFile,
    destination: &Path,
) -> Result<(), crate::core::error::AppError> {
    staging.file.take();
    replace_file(&staging.path, destination).map_err(|_| storage_error())?;
    staging.published = true;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing_name: *const u16, new_name: *const u16, flags: u32) -> i32;
    }

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().chain([0]).collect();
    // Both paths are siblings, so this stays on one volume. MOVEFILE_REPLACE_EXISTING
    // atomically replaces a prior target after the completed staging file exists.
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn same_file_or_path(source: &Path, destination: &Path) -> bool {
    if source == destination {
        return true;
    }
    if let (Ok(source), Ok(destination)) = (fs::canonicalize(source), fs::canonicalize(destination))
    {
        return source == destination;
    }
    false
}

fn is_cancelled(cancelled: &AtomicBool) -> bool {
    cancelled.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Read, sync::atomic::AtomicBool};

    use super::*;

    fn make_wav(path: &Path, sample_count: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: super::super::OUTPUT_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for index in 0..sample_count {
            writer
                .write_sample(((index % 257) as i16).wrapping_mul(100))
                .unwrap();
        }
        writer.finalize().unwrap();
    }

    fn partial_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "partial"))
            .collect()
    }

    #[test]
    fn wav_export_streams_exact_source_bytes_and_replaces_only_when_complete() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("recording.wav");
        let target = directory.path().join("saved.wav");
        make_wav(&source, 1_200_000);
        fs::write(&target, b"old destination").unwrap();
        let source_bytes = fs::read(&source).unwrap();
        let progress = AtomicBool::new(false);

        let outcome = export_recording(
            &source,
            &target,
            None,
            RecordingExportFormat::Wav,
            75.0,
            &progress,
            |_| {},
        )
        .unwrap();

        assert_eq!(outcome, RecordingExportOutcome::Saved);
        assert_eq!(fs::read(&target).unwrap(), source_bytes);
        assert!(partial_files(directory.path()).is_empty());
    }

    #[test]
    fn cancelling_copy_preserves_existing_destination_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("recording.wav");
        let target = directory.path().join("saved.wav");
        make_wav(&source, 1_200_000);
        fs::write(&target, b"previous bytes").unwrap();
        let cancelled = AtomicBool::new(false);

        let outcome = export_recording(
            &source,
            &target,
            None,
            RecordingExportFormat::Wav,
            75.0,
            &cancelled,
            |processed| {
                if processed > 0.0 {
                    cancelled.store(true, Ordering::Release);
                }
            },
        )
        .unwrap();

        assert_eq!(outcome, RecordingExportOutcome::Cancelled);
        assert_eq!(fs::read(&target).unwrap(), b"previous bytes");
        assert!(partial_files(directory.path()).is_empty());
    }

    #[test]
    fn flac_encode_failure_preserves_existing_destination_and_cleans_staging() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("broken.wav");
        let target = directory.path().join("saved.flac");
        fs::write(&source, b"not a wav file").unwrap();
        fs::write(&target, b"previous bytes").unwrap();
        let cancelled = AtomicBool::new(false);

        let result = export_recording(
            &source,
            &target,
            None,
            RecordingExportFormat::Flac,
            1.0,
            &cancelled,
            |_| {},
        );

        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"previous bytes");
        assert!(partial_files(directory.path()).is_empty());
    }

    #[test]
    fn flac_export_stays_readable_for_large_wav_input() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("recording.wav");
        let target = directory.path().join("saved.flac");
        make_wav(&source, 1_200_000);
        let cancelled = AtomicBool::new(false);
        let mut last_progress = 0.0;

        let outcome = export_recording(
            &source,
            &target,
            None,
            RecordingExportFormat::Flac,
            75.0,
            &cancelled,
            |processed| last_progress = processed,
        )
        .unwrap();

        assert_eq!(outcome, RecordingExportOutcome::Saved);
        assert_eq!(last_progress, 75.0);
        let mut file = File::open(&target).unwrap();
        let mut header = [0_u8; 4];
        file.read_exact(&mut header).unwrap();
        assert_eq!(&header, b"fLaC");
        assert_eq!(super::super::probe(&target).unwrap().frame_count, 1_200_000);
        assert!(partial_files(directory.path()).is_empty());
    }

    #[test]
    fn unwritable_destination_returns_storage_error_and_keeps_source() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("recording.wav");
        make_wav(&source, 16_000);
        let original = fs::read(&source).unwrap();
        let destination = directory.path().join("missing-parent").join("saved.wav");
        let cancelled = AtomicBool::new(false);
        let error = export_recording(
            &source,
            &destination,
            None,
            RecordingExportFormat::Wav,
            1.0,
            &cancelled,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(error.category, crate::core::error::Category::Storage);
        assert!(error.detail_redacted.contains("free disk space"));
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(partial_files(directory.path()).is_empty());
    }

    #[test]
    fn partial_lives_in_the_staging_dir_on_the_same_volume_and_is_removed_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let staging = directory.path().join("app-staging");
        let destination = directory.path().join("out").join("saved.flac");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();

        let temporary = temporary_path_for(&destination, Some(&staging)).unwrap();
        assert!(temporary.starts_with(&staging));
        assert_ne!(temporary.parent(), destination.parent());

        // Without a staging dir the sibling location is used.
        let sibling = temporary_path_for(&destination, None).unwrap();
        assert_eq!(sibling.parent(), destination.parent());

        // A failed export leaves no partial in either place.
        let source = directory.path().join("broken.wav");
        fs::write(&source, b"not a wav file").unwrap();
        let cancelled = AtomicBool::new(false);
        let result = export_recording(
            &source,
            &destination,
            Some(&staging),
            RecordingExportFormat::Flac,
            1.0,
            &cancelled,
            |_| {},
        );
        assert!(result.is_err());
        assert!(partial_files(&staging).is_empty());
        assert!(partial_files(destination.parent().unwrap()).is_empty());
    }
}
