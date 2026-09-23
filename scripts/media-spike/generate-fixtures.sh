#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"
output_dir="${1:-$repo_root/src-tauri/tests/fixtures/media}"
ffmpeg_bin="${FFMPEG:-ffmpeg}"
fixture_seconds="${MEDIA_FIXTURE_SECONDS:-1}"
benchmark_seconds="${BENCH_SECONDS:-60}"

if ! command -v "$ffmpeg_bin" >/dev/null 2>&1; then
  echo "ffmpeg executable not found: $ffmpeg_bin" >&2
  exit 1
fi
mkdir -p "$output_dir"
echo "Generating synthetic media fixtures with: $($ffmpeg_bin -hide_banner -version | head -n 1)"
echo "Fixture duration: ${fixture_seconds}s; benchmark duration: ${benchmark_seconds}s"
echo "Output: $output_dir"

audio="sine=frequency=440:sample_rate=48000:duration=$fixture_seconds"
video="color=c=black:s=64x64:r=4:d=$fixture_seconds"
secondary_audio="sine=frequency=880:sample_rate=32000:duration=$fixture_seconds"
common=(-hide_banner -loglevel error -nostdin -y -threads 1)

# Synthetic, one-second-by-default fixtures. These are compatibility samples,
# not recordings. FFmpeg is a development-time generator only; the Rust app
# does not invoke or bundle it.
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a libmp3lame -q:a 4 -write_xing 0 "$output_dir/sample.mp3"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a aac -b:a 96k -movflags +faststart "$output_dir/sample.m4a"

# Video fixtures put the 48 kHz audio track first and a 32 kHz track second.
# The probe/decode test asserts that the first audio track is selected.
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" -f lavfi -i "$secondary_audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -map 2:a:0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
  -c:a aac -b:a 96k -shortest -movflags +faststart "$output_dir/sample.mp4"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" -f lavfi -i "$secondary_audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -map 2:a:0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
  -c:a aac -b:a 96k -shortest "$output_dir/sample.mov"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" -f lavfi -i "$secondary_audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -map 2:a:0 -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
  -c:a aac -b:a 96k -shortest "$output_dir/sample.mkv"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" -f lavfi -i "$secondary_audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -map 2:a:0 -c:v libvpx -deadline realtime -cpu-used 8 -b:v 40k \
  -strict -2 -ac 2 -c:a vorbis -q:a 3 -shortest "$output_dir/sample.webm"

"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a pcm_s16le "$output_dir/sample.wav"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a flac -sample_fmt s16 "$output_dir/sample.flac"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -strict -2 -ac 2 -c:a vorbis -q:a 3 -serial_offset 0 "$output_dir/sample.ogg"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a pcm_s16be "$output_dir/sample.aiff"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a pcm_s16le "$output_dir/sample.caf"

# Negative fixtures: Opus is explicitly rejected for S1; AVI is not allowlisted.
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -c:v libvpx -deadline realtime -cpu-used 8 -b:v 40k \
  -c:a libopus -b:a 32k -shortest "$output_dir/opus.webm"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -c:v mpeg4 -c:a libopus -b:a 32k -shortest "$output_dir/opus.mkv"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$video" -f lavfi -i "$audio" \
  -map_metadata -1 -fflags +bitexact -flags:v +bitexact -flags:a +bitexact -map 0:v:0 -map 1:a:0 -c:v mpeg4 -c:a pcm_s16le -shortest "$output_dir/unsupported.avi"

# The benchmark source is kept separate from the tiny correctness fixtures.
benchmark_audio="anoisesrc=color=white:sample_rate=48000:duration=$benchmark_seconds:seed=20260923"
"$ffmpeg_bin" "${common[@]}" -f lavfi -i "$benchmark_audio" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -map 0:a:0 -c:a pcm_s16le \
  "$output_dir/benchmark-4worker.wav"

(
  cd "$output_dir"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 sample.mp3 sample.m4a sample.mp4 sample.mov sample.mkv sample.webm \
      sample.wav sample.flac sample.ogg sample.aiff sample.caf opus.webm opus.mkv \
      unsupported.avi benchmark-4worker.wav > SHA256SUMS
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum sample.mp3 sample.m4a sample.mp4 sample.mov sample.mkv sample.webm \
      sample.wav sample.flac sample.ogg sample.aiff sample.caf opus.webm opus.mkv \
      unsupported.avi benchmark-4worker.wav > SHA256SUMS
  else
    echo "warning: shasum/sha256sum not found; not writing the fixture hash manifest" >&2
  fi
)
echo "Wrote 11 supported-format fixtures, 2 Opus fixtures, 1 unsupported fixture, and benchmark WAV."
