# ADR S4: macOS system audio through a Core Audio process tap

- Status: implemented; device and distribution acceptance pending
- Date: 2026-09-28
- Related story: 4.2

## Decision

On macOS 14.4 and later, capture system output with a global `CATapDescription` process tap. Resolve the running app's PID to its Core Audio process `AudioObjectID`, exclude that object from a stereo global tap, and add the tap to a private aggregate device. CPAL opens the aggregate as an input stream and sends its samples and format to the existing 4.1 capture port. Mixed capture uses the same system input plus the selected microphone input, which the existing capture pipeline mixes on its output clock.

The Core Audio bindings are pinned in `src-tauri/Cargo.toml`. The tap and aggregate are owned by the prepared stream and destroyed when that stream is dropped. A tap remains open while its source is retained by the existing capture controller, so switching from `system` to `mixed:<mic>` can reuse it without restarting the source. Creating and starting the aggregate provides the first-use System Audio Recording consent prompt. A silence-only stream remains valid audio and is never treated as evidence of a permission denial.

## Permission and packaging

The app bundle declares `NSAudioCaptureUsageDescription` and `NSMicrophoneUsageDescription` in `src-tauri/Info.plist`; its minimum macOS version is 14.4. A Core Audio tap or stream failure returns `AppError` category `permission` with the System Settings path and permits another source-selection attempt. The app does not request Screen Recording permission or use ScreenCaptureKit.

Development macOS builds must use a suitable signing identity for team `B2U85XPU55` so the app identity stays stable across rebuilds. See [macOS system-audio development](../macos-system-audio.md) for signing, retry, and acceptance instructions.

## Verification record

| Check | Result |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml audio` | Pass: 19 selected tests on the local macOS machine |
| `cargo check --manifest-path src-tauri/Cargo.toml` | Pass on the local macOS machine |
| `npm run check` | Pass: 0 errors and 0 warnings |
| Signed bundle and privacy metadata | Pass: local app bundle verified with team `B2U85XPU55`; Info.plist reports minimum macOS 14.4 and both usage descriptions |
| First-use TCC prompt and retry after changing System Audio Recording access | Not run |
| Zoom, Teams, and Chrome Meet on three macOS machines | Not run |
| Sandboxed/TestFlight build | Not run; Story 1.11 distribution work is still pending |

The signed-bundle metadata check passed, but TCC behavior, the three-machine app matrix, and TestFlight remain unverified.
