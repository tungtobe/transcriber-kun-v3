# macOS system-audio development

System capture uses Core Audio process taps on macOS 14.4 and later. The bundle contains the system-audio and microphone usage descriptions in `src-tauri/Info.plist` and requests no Screen Recording permission.

## Sign and launch a development build

Use a signing identity for team `B2U85XPU55`. The local verification used the installed `Developer ID Application` identity; TestFlight packaging will require the distribution setup from Story 1.11:

```sh
security find-identity -v -p codesigning
npm run tauri -- build --bundles app
codesign --force --deep --sign "Developer ID Application: RELIPA COMPANY LIMITED (B2U85XPU55)" \
  src-tauri/target/release/bundle/macos/trans-kun.app
codesign --verify --deep --strict --verbose=2 \
  src-tauri/target/release/bundle/macos/trans-kun.app
```

Use the installed team identity shown by `security find-identity` if it differs. Confirm that the built app reports minimum system version 14.4 and that its Info.plist has `NSAudioCaptureUsageDescription` and `NSMicrophoneUsageDescription` before testing.

## Retry after a denial

1. Select `system` or `mixed:<microphone>` in Live to make macOS present the System Audio Recording prompt.
2. If access was denied, open **System Settings > Privacy & Security > Screen & System Audio Recording**, enable trans-kun, return to the app, and select the source again.
3. Confirm that an input which is simply silent continues to emit silence chunks without a permission error.

Microphone consent is managed separately under **System Settings > Privacy & Security > Microphone**. A system-capture error includes the relevant System Audio Recording path in its `permission` detail.

## Acceptance checklist

Record the machine identifier, macOS version, app build/signing identity, permission result, and a short evidence note for each row. For each machine, test System and Mixed sources with app playback excluded, and use an actual remote participant or test tone in Zoom, Teams, and Chrome Meet.

| Machine | macOS (14.4+) | Signed build | Zoom | Teams | Chrome Meet | System + Mixed result / evidence |
|---|---|---|---|---|---|---|
| 1 | Pending | Pending | Pending | Pending | Pending | Pending |
| 2 | Pending | Pending | Pending | Pending | Pending | Pending |
| 3 | Pending | Pending | Pending | Pending | Pending | Pending |

TestFlight/sandbox acceptance is pending Story 1.11. Record its signed build, entitlements, TCC prompt, and the same app matrix here after that distribution path is available. Until those runs are recorded, the three-machine and TestFlight acceptance criteria are not complete.
