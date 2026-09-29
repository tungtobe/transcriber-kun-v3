---
title: 'Story 5.3: Đọc bản dịch (TTS) trong tiến trình app'
type: 'feature'
created: '2026-09-30'
status: 'done'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-5-context.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Người dùng đeo tai nghe không nghe được bản dịch; audio đầu ra của model Live Translate hiện bị bỏ. **Approach:** Giải mã audio đầu ra (PCM16 24 kHz mono LE, base64 trong `model_turn.parts[].inline_data`) và phát trực tiếp trong tiến trình Rust bằng `audio::playback` (cpal), không qua IPC; UI chỉ nhận `speaking: bool`; có toggle loa, supervisor thiết bị output, và ADR cho cơ chế loại trừ TTS khỏi loopback Windows.

## Boundaries & Constraints

**Always:** Nguồn giọng duy nhất là audio của model; không thêm model TTS local, TTS của OS hay TTS cloud (mở rộng `scripts/check-forbidden-deps.mjs` chặn crate TTS, có test). Audio không đi qua IPC. Toggle loa tắt = vẫn nhận nhưng bỏ audio (không phát, không đệm); tooltip: "chỉ tắt việc phát, không giảm token dịch". Toggle dùng `aria-pressed`, vô hiệu kèm tooltip khi "Không dịch" (thay placeholder của 5.1); pill "Đang đọc" ở đầu cột Dịch bật/tắt theo lúc thực sự phát. Supervisor poll thiết bị output mặc định mỗi 1 s, phát tiếp trên thiết bị mới ≤ 2 s; stall 2.5 s = thiết bị chết; Bluetooth tắt đột ngột chuyển ≤ 3 s. Backend playback trừu tượng qua trait + fake để test xác định (theo kiểu `CaptureBackend`). `audio_interrupted`/`interrupted` → dừng và xoá buffer ngay. Đổi generation (5.1/5.2) hoặc tắt dịch/TTS xoá audio chờ; audio của generation cũ không phát sau swap. macOS: tap đã loại trừ PID app (giữ, thêm test/tài liệu hồi quy). Windows: ADR ghi cơ chế loại trừ (ứng viên: process-loopback EXCLUDE_TARGET_PROCESS_TREE trên Win10 build ≥ 20348/Win11, hoặc tách endpoint) và trạng thái CHƯA kiểm chứng trên Win10 1809/Win11; không tắt toàn bộ system capture khi TTS phát.

**Never:** Phát audio khi "Không dịch" hay toggle tắt; gửi PCM qua IPC/log; thêm crate TTS; tuyên bố Windows đã xác nhận khi chưa spike.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Phát | Dịch bật, TTS bật, nhận inline_data | Giải mã, phát in-process, speaking=true rồi false khi hết | Base64/độ dài lẻ → bỏ chunk, không panic |
| Toggle tắt | TTS tắt | Audio nhận nhưng bỏ, speaking=false | — |
| Không dịch | target=none | Toggle vô hiệu, audio bỏ | — |
| Ngắt lời | interrupted | Dừng + xoá buffer ngay | — |
| Đổi thiết bị | Mặc định đổi | Phát tiếp ≤2 s (fake clock) | Mở stream fail → thử lại/báo nhẹ |
| Thiết bị chết | Stall 2.5 s / rút BT | Chuyển thiết bị ≤3 s | Không có thiết bị → speaking=false, phiên vẫn chạy |
| Swap generation | Sau swap | Audio cũ không phát, buffer xoá | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/live/mod.rs` -- `WireServerContent` thêm model_turn/inline_data + interrupted; `LiveEvent` thêm `OutputAudio` (nội bộ, không specta/IPC) và `Interrupted`; hiện đang bỏ output audio (cập nhật test `output_drop`).
- `src-tauri/src/audio/playback.rs` (mới), `audio/mod.rs` -- trait `PlaybackBackend`, cpal backend (cfg macos/windows, stub khác), buffer, supervisor, fake cho test.
- `src-tauri/src/live/mod.rs` -- giữ playback handle, `Command::SetTts`, phát `speaking` event/snapshot, xoá buffer khi swap/stop/tắt dịch.
- `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- `live_set_tts`.
- `src/routes/Live.svelte`, `src/lib/stores/live.svelte.ts` -- toggle loa, pill "Đang đọc".
- `src/i18n/{vi,en,ja}.json`, `scripts/check-forbidden-deps.mjs` (+ test).
- `src-tauri/src/audio/macos_tap.rs` -- test hồi quy loại trừ PID; `docs/adr/0006-tts-playback-loopback-exclusion.md` (mới) cho Windows + S3 (audio_interrupted).

## Tasks & Acceptance

**Execution:**
- [x] `gemini/live` -- parse audio đầu ra + interrupted, tests
- [x] `audio/playback.rs` -- backend, buffer, supervisor (fake clock), tests
- [x] `live/mod.rs`, ipc, bindings -- SetTts, speaking, dọn buffer, tests
- [x] UI + i18n + tests; forbidden-deps + test; ADR 0006

**Acceptance Criteria:**
- Given dịch + loa bật, when model nói, then phát in-process và pill "Đang đọc" bật/tắt đúng.
- Given toggle tắt hoặc Không dịch, then không có audio phát/đệm.
- Given đổi thiết bị/thiết bị chết, then phát tiếp trong ngưỡng thời gian (test fake clock).

## Implementation Notes

- Gateway emits `OutputAudio`/`Interrupted` only when translating; decoded and validated (even length) in `parse_server_message`, so IPC/log never see PCM (`Sensitive`).
- `audio::playback`: `PlaybackBuffer` (refuses audio while disabled), clock-injected `Supervisor` (poll 1 s, stall 2.5 s, stream-error flag), `PlaybackHandle` thread (cpal streams are !Send, so the backend is built on that thread), cpal backend cfg(macos/windows), `NullBackend` elsewhere. Linear resampling 24 kHz mono to device rate/channels inside the buffer.
- Actor: `tts_enabled` defaults to off (no surprise audio in meetings); playback enabled only when tts on and target != none; `LiveSnapshot` gained `tts`/`speaking`; events `Tts`/`Speaking`. Swap promote, interrupt, stop and toggle-off clear the buffer.
- Windows exclusion is documented in ADR 0006 and left UNVERIFIED; no Windows-only exclusion code added. macOS relies on the existing PID exclusion test.
- Verified here on macOS only (cargo tests, frontend checks). Real-device listening, cpal on Windows and the S3 model behavior are not verified.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `npm run bindings && cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
