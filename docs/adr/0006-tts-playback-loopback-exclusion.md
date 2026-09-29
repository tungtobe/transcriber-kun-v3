# ADR 0006: In-process TTS playback and keeping it out of the loopback capture

## Status

Implementation complete on macOS (verified by unit tests only; no listening
test on real hardware was run in this change). The Windows exclusion mechanism
is **not verified**: it needs the Windows Phase 0 spike on Windows 10 1809 and
Windows 11 before Story 5.3 can be accepted there. The S3 spike on the real
Live model is also still outstanding (see "Interruption").

## Decision

- The only voice source is the Live Translate model's own output audio
  (PCM16, 24 kHz, mono, little-endian, base64 in `modelTurn.parts[].inlineData`).
  No local, OS or cloud TTS is added; `npm run check:deps` rejects known TTS
  crates in `Cargo.toml`/`Cargo.lock`.
- The gateway decodes the audio and hands it to `audio::playback` inside the
  Rust process (cpal output stream). Audio never crosses IPC and is never
  logged (`Sensitive`); the UI only receives `speaking: bool` and the toggle
  state. Invalid base64 or an odd byte count drops that chunk only.
- The speaker toggle off, or target `none`, disables the buffer: audio is still
  received but not queued or played. This does not reduce translation tokens
  (the tooltip says so).
- `interrupted` / `audio_interrupted` clears the buffer and stops playback at
  once. A generation swap (Target change, re-detect), Stop and toggle-off also
  clear it; events of a retired generation are already dropped by the actor.
- A supervisor thread owns the cpal stream (streams are not `Send`), polls the
  default output device every 1 s and reopens on a change (playback resumes
  within about 2 s), on a stream error, or when queued audio sees no device
  callback for 2.5 s (dead device; a Bluetooth device switching off is handled
  within 3 s). With no usable device it drops queued audio, reports
  `speaking = false` and the session keeps running. The supervisor is a
  clock-injected state machine behind a `PlaybackBackend` trait with a fake, in
  the style of `CaptureBackend`.

## Not recording our own voice

### macOS (kept)

The system tap is a global tap that excludes the app's own Core Audio process
object (`CATapDescription` with the app PID translated through
`kAudioHardwarePropertyTranslatePIDToProcessObject`, see ADR 0004). Playback in
this process is therefore not part of the tap. Regression coverage:
`audio::macos_tap::tests::global_tap_description_excludes_the_supplied_process_object`
pins the exclusion list; playback must stay in the app process (never a helper
process), otherwise this exclusion no longer applies.

### Windows (candidate, UNVERIFIED)

Windows shared-mode loopback captures the whole endpoint mix, so TTS played on
the same render endpoint would be re-captured. Candidates, none confirmed:

1. Process loopback with `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`
   targeting the app's PID (`ActivateAudioInterfaceAsync` on
   `VAD\Process_Loopback`). Documented for Windows 10 build 20348 and later and
   Windows 11 (the API exists from build 19041, exclude mode was reported
   unreliable earlier). It is therefore **not** expected to work on Windows 10
   1809.
2. Splitting endpoints: play TTS on a different render endpoint than the one
   loopback captures. Needs a device policy and may not be possible with a
   single headset.

The decision to pick one needs the Windows Phase 0 spike (Win 10 1809 and
Win 11) showing the meeting audio is still fully captured and the transcript
contains no re-read TTS. Turning off all system capture while TTS plays is
explicitly rejected. Until then, no Windows-specific exclusion code was added
and playback on Windows uses the same cpal path, which is compiled only for
macOS and Windows and could not be built or run on the development machine.

## Interruption (S3)

`serverContent.interrupted` (accepted also as `audioInterrupted`) is parsed
and clears playback. The exact field name and timing on the real model are
candidates from documentation and remain to be confirmed by the S3 spike.
