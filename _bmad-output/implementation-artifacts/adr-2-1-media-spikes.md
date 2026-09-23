# ADR 2.1 — Opus handling and FLAC playback evidence

Date: 2026-09-23

## Status

- S1 (Opus): **decided — reject explicitly in this spike**.
- S8 (proxy playback format): **pending target-platform seek measurements**. FLAC remains the current implementation behind `media/proxy`; this is not evidence that FLAC meets the playback requirement or a final decision to keep it.

## Context

Story 2.1 requires a pure-Rust ingest path, a 16 kHz mono FLAC proxy, correct duration and bounded memory, and evidence for Opus support and FLAC random-seek behavior in WKWebView and WebView2. It prohibits a runtime FFmpeg/sidecar. This repository has no finished playback UI or IPC path; the testable Rust proxy API and a development-only proxy writer are available.

## S1 — Opus

**Decision:** return `AppError` category `format` for Opus in WebM or Matroska. Do not add a native-library or FFmpeg fallback in this story. The user-facing detail recommends exporting as MP4, M4A, or MP3.

**Evidence:** the pinned Symphonia 0.6.1 dependency has no Opus codec feature. A generated WebM/Opus fixture and a generated Matroska/Opus fixture are both covered by integration tests; probe and decode reject each with category `format`. The fixture tests also exercise the supported WebM/Vorbis path, so the decision is codec-specific rather than a blanket WebM rejection.

**Consequence:** FR-9 should represent Opus as explicitly unsupported. Reconsider only if product requirements prioritize Opus enough to select and validate a pure-Rust decoder.

## S8 — FLAC proxy and seek

**Decision:** retain the existing FLAC proxy implementation for now, but leave the playback-format decision open. `media/proxy` remains the seam for a later AAC-native or Rust-player alternative. Do not advertise seek compliance until both target WebViews have been measured.

**Local evidence:** the synthetic corpus and Rust tests establish that generated FLAC proxies are standalone and readable by the same decoder. Proxies with 1–15 true samples preserve the STREAMINFO sample count and decode to exactly that many samples. This does not exercise browser range requests, a 90-minute file, or a real recording.

## Reproducible synthetic corpus

Generate using `scripts/media-spike/generate-fixtures.sh`. Tested generator: FFmpeg 8.0.1. The checked-in fixtures are synthetic sine/color and seeded white-noise signals; they are not real recordings. The eleven supported-extension integration cases pass, including first-audio-track selection in video containers. Negative fixtures cover WebM/Opus, Matroska/Opus, and unsupported AVI. `src-tauri/tests/fixtures/media/SHA256SUMS` records file hashes.

The local suite does not establish compatibility for real-world encoder variants, unusual metadata, variable-frame-rate video, corruption, or all files carrying these extensions. Repeat the checks with legally shareable real recordings before broad format claims.

## Local four-worker throughput run

Command, after one discarded warm-up and followed by five measured invocations:

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml --locked \
  --example media_throughput -- \
  src-tauri/tests/fixtures/media/benchmark-4worker.wav 3
```

Environment: macOS 26.6.2 (build 25G83), Apple M4 Pro, 12 physical / 12 logical CPUs, arm64, Rust/Cargo 1.98.1, FFmpeg 8.0.1. The input is a seeded 60-second white-noise WAV, SHA-256 `e9b888c75a767d712d03c3bd0cd52d148960f2c3e3d0ebaa46b60dbffcf3e182`. The optimized harness decoded and cut 720 media-seconds into 12 FLAC chunks per measured run using four concurrent worker threads. Aggregate throughput was **3,155× realtime median**, with a **3,131×–3,310×** observed range (wall time 0.218–0.230 s). Per-worker runs were approximately 783×–833× realtime. One warm-up preceded the five measured runs.

This is local evidence on a 12-core host, not a constrained four-core result. Four threads are used, but the harness does not pin threads or cap CPU availability. The synthetic WAV is also not representative of codec/container diversity. The ≥20× acceptance criterion on a four-core machine remains unverified.

## Practical 90-minute WKWebView/WebView2 seek procedure

Run this procedure independently on macOS and Windows. It is intentionally a manual target-platform test: no result is recorded yet.

1. Capture the target system and runtime versions. On macOS record `sw_vers`, `sysctl -n hw.physicalcpu`, `sysctl -n hw.logicalcpu`, CPU model, and `navigator.userAgent` from the app WebView. On Windows record `Get-ComputerInfo` OS/build fields, CPU model/core counts using `Get-CimInstance Win32_Processor`, and the WebView2 runtime version shown by the WebView user agent or runtime diagnostics.
2. Generate a deterministic 90-minute, high-entropy source outside the repository (the generator writes a 48 kHz white-noise WAV of about 518 MB):

   ```sh
   tmp_dir="$(mktemp -d)"
   BENCH_SECONDS=5400 scripts/media-spike/generate-fixtures.sh "$tmp_dir"
   ```

   On Windows, run generation and proxy creation in the same Git Bash session with FFmpeg 8.x; keep the temp path for the next step. Record the source SHA-256 and FFmpeg version.
3. Create a proxy in the app's scoped media area with the development-only Rust example. On macOS:

   ```sh
   media_dir="$HOME/Library/Application Support/com.transkun.app/media/seek-spike"
   cargo run --release --manifest-path src-tauri/Cargo.toml --locked \
     --example create_media_proxy -- "$tmp_dir/benchmark-4worker.wav" "$media_dir"
   ```

   On Windows Git Bash, convert the app-data path and run the same example:

   ```sh
   media_dir="$(cygpath -u "$APPDATA")/com.transkun.app/media/seek-spike"
   cargo run --release --manifest-path src-tauri/Cargo.toml --locked \
     --example create_media_proxy -- "$tmp_dir/benchmark-4worker.wav" "$media_dir"
   ```

   Record proxy path, bytes, and hash from the command output. The generated proxy must remain under `$APPDATA/media/**` for the test.
4. In a development-only playback view, set the `<audio>` element's source from the proxy path using `convertFileSrc` from `@tauri-apps/api/core`, with `preload="metadata"`. The current app has no player UI, so this temporary view is a prerequisite; do not ship it as part of the product. Confirm the scoped asset request succeeds and use the Web Inspector/Edge DevTools Network panel to confirm range requests. Also attempt a file outside the media scope and verify no bytes are returned.
5. After metadata is loaded and playback has been started by a user gesture, seek to 30 s, 2,700 s, and 5,370 s. Measure from assigning `audio.currentTime` until the corresponding `seeked` event using `performance.now()`. Run one warm-up seek, then five measured seeks at each position. Preserve per-attempt times and the Network panel range/status evidence. Every measured seek must be ≤500 ms.
6. Record OS/runtime, CPU, fixture/proxy hashes and byte sizes, build mode, all 15 timings, median/range by position, range-request evidence, and the out-of-scope denial result below. Repeat from a clean app launch at least once. If any target position fails the 500 ms threshold, use this evidence to decide whether to switch the proxy to native AAC or introduce a Rust player; do not infer a result from macOS for Windows or vice versa.

| Target | OS/build | WebView runtime | Proxy bytes/hash | 30 s timings | 2,700 s timings | 5,370 s timings | Range + scope result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| macOS / WKWebView | **not run** | **not run** | — | — | — | — | — |
| Windows / WebView2 | **not run** | **not run** | — | — | — | — | — |

## Remaining evidence

- Real recording corpus and 20× results on a four-core target machine.
- 90-minute proxy seek/range and out-of-scope checks on macOS WKWebView.
- 90-minute proxy seek/range and out-of-scope checks on Windows WebView2.
- S8 final keep-FLAC/switch-format decision after both platform result rows are complete.
