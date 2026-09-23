# Media spike corpus and local benchmark

This is a development/test harness. `scripts/media-spike/generate-fixtures.sh`
uses the locally installed FFmpeg CLI to make deterministic synthetic tone/color
samples. The Rust application does not call, bundle, link, or require FFmpeg.
These files are compatibility fixtures, not real recordings or evidence about
all encoders used by customers.

The checked-in one-second corpus covers the eleven promised extensions. The
generator disables metadata copying and sets FFmpeg bitexact flags to avoid
volatile muxer metadata on a fixed FFmpeg build:

| File | Container / codec | Notes |
| --- | --- | --- |
| `sample.mp3` | MP3 | 48 kHz synthetic sine |
| `sample.m4a` | M4A / AAC | 48 kHz synthetic sine |
| `sample.mp4` | MP4 / H.264 + AAC | 48 kHz first audio track; 32 kHz second track |
| `sample.mov` | MOV / H.264 + AAC | 48 kHz first audio track; 32 kHz second track |
| `sample.mkv` | Matroska / H.264 + AAC | 48 kHz first audio track; 32 kHz second track |
| `sample.webm` | WebM / VP8 + Vorbis | 48 kHz first audio track; 32 kHz second track |
| `sample.wav` | WAV / PCM s16le | 48 kHz synthetic sine |
| `sample.flac` | FLAC | 48 kHz synthetic sine |
| `sample.ogg` | Ogg / Vorbis | 48 kHz synthetic sine |
| `sample.aiff` | AIFF / PCM s16be | 48 kHz synthetic sine |
| `sample.caf` | CAF / PCM s16le | 48 kHz synthetic sine |

Negative fixtures are `opus.webm`, `opus.mkv`, and `unsupported.avi`. The Opus
files exercise the explicit S1 rejection; AVI exercises the extension allowlist.
`benchmark-4worker.wav` is a separate configurable seeded white-noise PCM source
for throughput measurement.

## Generate and test

Requires a development FFmpeg build with AAC, FLAC, libmp3lame, libopus, Vorbis,
libx264, and libvpx encoders. The generator defaults to FFmpeg on `PATH`; set
`FFMPEG` to another development executable if needed.

```sh
bash scripts/media-spike/generate-fixtures.sh
(cd src-tauri/tests/fixtures/media && shasum -a 256 -c SHA256SUMS)
cargo test --manifest-path src-tauri/Cargo.toml --test media_corpus
cargo test --manifest-path src-tauri/Cargo.toml media::proxy::tests::proxy_handles_one_through_fifteen_output_samples
```

To regenerate small fixtures and a 60-second throughput input explicitly:

```sh
MEDIA_FIXTURE_SECONDS=1 BENCH_SECONDS=60 bash scripts/media-spike/generate-fixtures.sh
```

The integration tests read checked-in files only; they do not invoke FFmpeg.
Review `SHA256SUMS` after regeneration. Container metadata and codec output can
differ between FFmpeg versions/builds even with identical lavfi inputs, so
record `ffmpeg -version` and the resulting hashes when refreshing the corpus.

## Four-worker throughput run

Run the release-mode example against the generated 60-second WAV. Each run
uses four concurrent Rust worker threads, each decoding and cutting the same
source into FLAC chunks for the requested number of iterations; the output reports aggregate realtime factor
and each worker's factor. This is not CPU affinity or a hard four-core cap.

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml \
  --example media_throughput -- \
  src-tauri/tests/fixtures/media/benchmark-4worker.wav 3
```

For stable results, record OS/build, CPU model and physical/logical core counts,
Rust version, FFmpeg version, fixture hash/length, iteration count, worker
count, and all run outputs. Repeat at least five times after one warm-up run;
report median and range. A result from a machine with more than four physical
cores is local evidence only, not proof of the four-core acceptance threshold.

## 90-minute seek fixture

Generate a 90-minute 48 kHz mono PCM source outside the repository (about
518 MB), then use the development-only Rust proxy writer to make the standalone
FLAC under the scoped app-data media folder:

```sh
tmp_dir="$(mktemp -d)"
BENCH_SECONDS=5400 scripts/media-spike/generate-fixtures.sh "$tmp_dir"
media_dir="$HOME/Library/Application Support/com.transkun.app/media/seek-spike"
cargo run --release --manifest-path src-tauri/Cargo.toml --locked \
  --example create_media_proxy -- "$tmp_dir/benchmark-4worker.wav" "$media_dir"
```

On Windows use `"$env:APPDATA\com.transkun.app\media\seek-spike"` as the second
argument from PowerShell. The current repository has no player UI or IPC command
to launch playback, so follow the temporary development-view and measurement
steps in `../../_bmad-output/implementation-artifacts/adr-2-1-media-spikes.md`
on each target OS. Do not ship that view as product UI.
