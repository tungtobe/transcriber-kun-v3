---
title: 'Story 5.4: Ducking volume hệ thống'
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

**Problem:** Khi app đọc bản dịch, tiếng Teams/họp át giọng TTS và người dùng phải chỉnh volume tay. **Approach:** Module `audio/output_volume` hạ volume hệ thống còn 30 % khi model nói và phục hồi khi im, với marker bền `state/ducking.json` để phục hồi sau crash, và không "đánh nhau" với chỉnh tay.

## Boundaries & Constraints

**Always:** Backend volume là trait nội bộ `audio/` (không thêm port kiến trúc thứ tư) với impl macOS (Core Audio, dependency objc2-core-audio sẵn có), Windows (cfg-gated, best-effort, ghi rõ chưa kiểm chứng) và fake cho test. Ducking chỉ khi TTS thực sự bật và model đang nói (dùng tín hiệu speaking của 5.3). Hạ còn 30 % mức gốc; phục hồi sau khi model im. Trước lần đổi volume đầu: ghi bền marker `state/ducking.json` (ghi atomic tạm-rồi-rename; gồm device ID, mức gốc, mức app áp, hiệu lực) — thêm helper `state_dir`/`ducking_marker_path` vào `core/paths.rs`. Lượt nói chồng nhau không ghi đè mức gốc bằng mức đã duck. Người dùng chỉnh volume khi đang Ducking (phát hiện mức đọc lại ≠ mức app áp) → bỏ mức gốc, giữ mức người dùng, vô hiệu marker ngay. Boot: đọc marker sau migrate DB, TRƯỚC `detach_staging`/recovery (chỗ comment "Epic 5 owns the Ducking marker" trong `ipc/boot.rs`); chỉ phục hồi đúng device khi marker còn hiệu lực và volume hiện tại vẫn = mức app áp; thiết bị vắng/volume đã đổi thì không ép; xử lý xong xoá marker. Phục hồi khi: TTS tắt, Dừng Live, đổi thiết bị output (baseline mới cho thiết bị mới, thiết bị cũ được phục hồi nếu marker còn hiệu lực), đóng app (`close`/`RunEvent::Exit`).

**Never:** Ép volume cũ lên thiết bị khác; ghi đè volume người dùng vừa chỉnh; duck khi TTS tắt/Không dịch; thêm dependency mới ngoài những crate đã có; log nội dung.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Duck/restore | speaking true→false | Volume 30 % gốc rồi về gốc, marker ghi trước và xoá sau | Backend lỗi → log, không chặn TTS |
| Lượt chồng | speaking bật lại khi đang duck | Mức gốc giữ nguyên | — |
| Chỉnh tay | User đổi volume lúc duck | Giữ mức user, marker vô hiệu, không restore | — |
| Crash | Marker hợp lệ + volume = mức áp | Boot phục hồi mức gốc đúng device | Marker hỏng → bỏ, xoá |
| Crash + user đã đổi | volume ≠ mức áp | Không ép | Xoá marker |
| Thiết bị vắng | Device ID không còn | Không phục hồi | Xoá marker |
| Đổi output | Device mới | Restore cũ nếu hợp lệ, baseline mới | — |
| Stop/tắt TTS/đóng app | Đang duck | Restore | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/audio/output_volume.rs` (mới) + `audio/mod.rs` -- trait `OutputVolume`, `Ducker` (state machine, marker), impl macOS/Windows cfg, `FakeVolume`.
- `src-tauri/src/audio/playback.rs`, `src-tauri/src/live/mod.rs` -- nối speaking/TTS-off/Stop/device-change tới Ducker.
- `src-tauri/src/core/paths.rs` -- `state_dir`, `ducking_marker_path` + tests.
- `src-tauri/src/ipc/boot.rs` -- restore sau migrate, trước detach_staging; `ipc/close.rs`/`lib.rs` Exit -- restore khi đóng.
- Không UI mới; nếu có copy thì i18n vi/en/ja.

## Tasks & Acceptance

**Execution:**
- [x] `output_volume.rs` -- trait, Ducker, marker atomic, impl nền tảng, tests với fake (crash, chỉnh tay, lượt chồng, đổi thiết bị, marker hỏng)
- [x] `paths.rs`, `boot.rs`, close/exit, `live/mod.rs` -- wiring + tests
- [x] Ghi chú vào ADR 0006 (Ducking, Windows chưa kiểm chứng)

**Acceptance Criteria:**
- Given TTS bật, when model nói/im, then volume 30 % rồi phục hồi.
- Given crash khi duck, when mở lại, then boot phục hồi đúng device nếu volume chưa bị đổi ngoài ý muốn, trước dọn staging.
- Given chỉnh tay khi duck, then mức user được giữ.

## Implementation Notes

- `Ducker` sống trong `LiveSessionActor` (móc vào `set_speaking`, `sync_playback` khi tắt, tick 250 ms để bắt đổi thiết bị/chỉnh tay); `restore_from_marker` dùng chung cho boot và `RunEvent::Exit` (marker-guarded nên no-op nếu Stop đã restore).
- Chỉnh tay: trạng thái `Overridden` giữ tay người dùng đến khi model im (không re-duck cùng lượt); marker ghi `active:false`.
- Dung sai đọc lại 0.02 do phần cứng lượng tử hoá bước volume. Thiết bị định danh bằng Core Audio UID.
- Windows: stub trơ, chưa kiểm chứng (crate `wasapi` không có endpoint volume, cấm thêm dependency); ghi trong ADR 0006.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `npm run bindings && cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
