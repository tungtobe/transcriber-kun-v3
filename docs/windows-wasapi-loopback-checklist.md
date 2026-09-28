# Windows WASAPI loopback acceptance checklist

Status: **Not run on Windows.** Implementation and portable tests were performed
on macOS. These checks require a Windows 10 version 1809 or newer / Windows 11
host; MSIX checks also require a package built with the project's package
identity configuration.

## Automated coverage

- [x] Console and Communications resolving to different endpoint IDs produces
  two capture lanes mixed once into one logical system stream.
- [x] Console and Communications resolving to the same endpoint ID produces
  one capture lane.
- [x] A failed endpoint can be removed while the other lane continues to feed
  the logical system stream.
- [x] A changed default endpoint is represented as an endpoint removal/addition
  and is reconciled by the capture worker.
- [x] Mixed capture keeps microphone audio and advances the shared output clock
  when the system source reports an endpoint error.
- [x] The system-only source requests no microphone stream.
- [x] Windows microphone guidance names Privacy & security > Microphone and
  `ms-settings:privacy-microphone`.

## Manual checks on Windows

- [ ] On a clean Windows 10 1809+ / Windows 11 x64 install, start system-only
  capture and play audio through the Console default endpoint.
- [ ] Assign Communications to a different output endpoint from Console. Play
  audio through both roles and confirm each contribution is present once in the
  recording.
- [ ] Assign both roles to the same endpoint ID. Confirm there is no doubled
  amplitude or echo caused by opening the endpoint twice.
- [ ] Change the default render endpoint during an active recording. Confirm
  capture follows the new endpoint, or reports a clear device error while the
  microphone and recording continue.
- [ ] Capture meeting audio from Zoom desktop, Teams desktop, and Google Meet in
  Chrome.
- [ ] Deny microphone access in Windows Settings, then select mic and mixed
  capture. Confirm each returns category `permission` with the Windows
  Microphone Settings path. Confirm system-only capture does not start a mic
  stream.
- [ ] Install and test the MSIX package on a clean machine. Confirm microphone
  capture requests the `microphone` capability and system loopback works
  without administrator access or a separate system-audio capability.
- [ ] Record OS build, architecture, endpoint-role assignments, application
  versions, package identity/capabilities, result, and any device errors here.

## Results

No real-device, Zoom, Teams, Chrome Meet, permission-prompt, or MSIX evidence has
been recorded yet. The repository's MSIX overlay currently contains no package
identity configuration, so packaged behavior remains unverified.
