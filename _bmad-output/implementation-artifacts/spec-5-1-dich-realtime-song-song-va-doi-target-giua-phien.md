---
title: 'Story 5.1: Dịch realtime song song và đổi Target giữa phiên'
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

**Problem:** Live chỉ hiển thị transcript gốc; Linh chưa thấy bản dịch song song và không đổi được ngôn ngữ đích giữa họp mà không ngắt phiên. **Approach:** Bật `outputAudioTranscription` + `translationConfig` khi có Target, đưa bản dịch (chỉ UI, không lưu) vào cột Dịch, thêm Target mặc định trong Settings (nhóm Live) và dropdown "Dịch sang"; đổi Target bằng generation mới (chờ `setupComplete`, swap, drain cũ ≤ 1 s).

## Boundaries & Constraints

**Always:** Setup có dịch: `outputAudioTranscription: {}` top-level, `translationConfig {targetLanguageCode, echoTargetLanguage: true}`; "Không dịch": không `outputAudioTranscription`, không `echoTargetLanguage`, target = ngôn ngữ nguồn (`ja` khi `auto`). Test khoá exact JSON cả hai chế độ. Bản dịch chỉ tới UI (event `delta_translated`/`segment_translated`), không vào `segments`/DB. Audio đầu ra vẫn bị bỏ (TTS thuộc 5.3). Swap Target: tạo generation mới, chờ `setupComplete` có deadline, swap, drain cũ ≤ 1 s (`OLD_GENERATION_DRAIN`); các lần đổi liên tiếp chỉ giữ lựa chọn mới nhất; thất bại giữ generation/Target cũ và dropdown trả về giá trị thực; Dừng huỷ mọi candidate; event generation cũ bị bỏ; transcript gốc không gián đoạn, timestamp theo sample clock. Cột Dịch vẫn hiển thị khi lời nói đã ở ngôn ngữ đích. Lỗi category `model` dùng banner tại chỗ của 4.8. Segmented "Gốc · Dịch · Cả hai"; Gốc/Dịch chỉ một cột. Copy vi/en/ja.

**Never:** Lưu bản dịch vào segments; tuyên bố "không tốn token dịch" trong UI khi chưa có bằng chứng S3 (ghi chú trong ADR/Implementation Notes là chưa kiểm chứng); thêm TTS/phát audio; đổi model; log nội dung dịch.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Setup có dịch | Target=vi | JSON có outputAudioTranscription, echoTargetLanguage:true | — |
| Không dịch | Target=none, lang=auto | target `ja`, không outputAudioTranscription/echo | — |
| Đổi Target | Đang chạy, chọn en | Generation mới sau setupComplete, cũ drain ≤1s, transcript gốc liền | Fail → giữ cũ, dropdown về giá trị thực |
| Đổi liên tiếp | vi→en→ja nhanh | Chỉ ja được swap | Candidate cũ bị huỷ |
| Dừng giữa swap | Stop khi đang setup | Huỷ candidate, phiên kết thúc bình thường | — |
| Lưu phiên | Có bản dịch | segments chỉ có transcript gốc | — |
| Không dịch | UI | Chỉ cột Gốc, nút TTS vô hiệu (placeholder), output bị bỏ | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/live/mod.rs` -- `LiveRunConfig` (l.126), `build_setup_message` (~l.2338) và structs setup (~l.2289), `WireServerContent` (~l.2242), `LiveEvent` (~l.85), `run_socket` map event; test setup JSON ~l.256 và các test parser/output_drop cần cập nhật.
- `src-tauri/src/live/mod.rs` -- `start` (~l.480-620) spawn gateway; `RunningSession.generation`; `OLD_GENERATION_DRAIN`; `Command` enum + handler `set_source` làm mẫu cho `SetTarget`; `LiveEvent`/`LiveSnapshot`; `apply_internal` bỏ event generation cũ. Refactor spawn thành helper dùng lại cho swap.
- `src-tauri/src/settings/mod.rs`, `src/lib/stores/settings.svelte.ts` -- thêm `liveTarget` (none + ngôn ngữ), KEY, load/save/default.
- `src-tauri/src/ipc/mod.rs`, `src/lib/bindings.ts` -- `live_set_target`, `live_start` nhận target; đăng ký command; `npm run bindings`.
- `src/routes/Settings.svelte`, `src/routes/settings/SettingsLive.svelte` (mới) -- nhóm Live với Target mặc định.
- `src/routes/Live.svelte`, `src/lib/stores/live.svelte.ts` -- select "Dịch sang", segmented view, hai cột, `setTarget` theo mẫu `changeSource`.
- `src/i18n/{vi,en,ja}.json` -- copy mới; `npm run check:i18n`.

## Tasks & Acceptance

**Execution:**
- [x] `gemini/live/mod.rs` -- thêm target vào `LiveRunConfig`, setup dịch/không dịch, parse output transcription → event dịch, tests exact JSON + parser
- [x] `live/mod.rs` -- LiveStartParams.target, event/snapshot dịch (không vào segments), `SetTarget` với swap tuần tự + tests (fake transport: swap ok, fail giữ cũ, liên tiếp, Stop, không lưu bản dịch)
- [x] `settings` (Rust + FE), `ipc`, bindings -- liveTarget, live_set_target
- [x] UI Settings Live group, Live.svelte/store, i18n ba ngôn ngữ, tests
- [x] Ghi chú S3 (chi phí "Không dịch" chưa kiểm chứng) vào `docs/adr/0005-gemini-live-transport-spike-s3.md`

**Acceptance Criteria:**
- Given chế độ có dịch, when mở Live, then setup đúng JSON và cột Dịch nhận delta/segment dịch.
- Given đổi Target khi đang chạy, when swap, then không lặp segment, timestamp không nhảy, generation cũ bị bỏ sau swap.
- Given phiên kết thúc, when lưu, then bản dịch không nằm trong segments.

## Implementation Notes

- Target là enum riêng `LiveTarget {none, ja, vi, en}` (settings, IPC, snapshot); `LiveSnapshot.target` và event `target` cho UI biết Target thực sau swap/remount.
- Swap: `set_target` không chặn actor. Candidate gateway (generation mới) chạy cạnh generation cũ; sự kiện của candidate bị bỏ cho tới `setupComplete`, lúc đó promote: đổi generation/cancel/task, cancel generation cũ và drain ≤ `OLD_GENERATION_DRAIN` trong task tách rời. Lần đổi liên tiếp huỷ candidate cũ và trả `Ok` cho yêu cầu bị thay (yêu cầu mới nhất sở hữu kết quả); thất bại (Reconnecting/ended/quá `TARGET_SWAP_DEADLINE` 15 s) giữ nguyên và trả lỗi; Stop và `continue_recording_only` huỷ candidate. Deadline 15 s là giá trị tạm, chốt ở S3.
- Chống lặp segment/timestamp nhảy: audio tới candidate đi qua forwarder có gate, chỉ mở khi promote; sample đầu tiên được chuyển làm sàn của transcript cursor (`LiveRunConfig.start_sample` là `Arc<AtomicU64>`), nên transcript của generation mới bắt đầu đúng chỗ generation cũ dừng. Sự kiện generation cũ sau swap bị bỏ (một câu đang dở của generation cũ có thể mất; câu dở trong buffer nguồn giữ và nối với generation mới).
- Bản dịch: gateway phát `OutputTranscription` chỉ khi có Target (target `none` bỏ output dù server gửi); actor tách câu bằng `is_sentence_end`, phát `deltaTranslated`/`segmentTranslated`, không đưa vào `pending`/DB (test khẳng định segments chỉ có transcript gốc). Buffer dịch được xả ở turn complete, swap và Stop. Không log nội dung dịch.
- Failure trả `Code::Network` (Reconnecting), `Code::Timeout` (deadline) hoặc lỗi gateway (vd. `Model`); FE chỉ bật banner tại chỗ 4.8 cho category model/quota/auth, mọi lỗi đều hiện dòng inline và dropdown về `snapshot.target`.
- UI: nút TTS chỉ là placeholder vô hiệu (TTS thuộc 5.3); không có câu "không tốn token dịch". Chi phí chế độ "Không dịch" ghi là chưa kiểm chứng trong ADR 0005.
- Verification: `npm run bindings`, `cargo test --locked`, `npm run check`, `npm test`, `check:i18n`, `check:ui`, `check:deps` đều xanh.

## Spec Change Log

## Review Triage Log

## Verification

**Commands:**
- `npm run bindings && cargo test --locked --manifest-path src-tauri/Cargo.toml` -- pass
- `npm run check && npm test && npm run check:i18n && npm run check:ui && npm run check:deps` -- pass
