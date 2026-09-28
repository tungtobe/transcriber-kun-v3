---
title: 'Story 4.4: Gemini Live WebSocket transport, resumption và reconnect'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '2971c3ff73219be0391eb0ab06b549a09f6086cf'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Live chưa có WebSocket transport; kết nối bị đóng hoặc mất mạng sẽ làm mất khả năng lấy transcript realtime. Story này tạo transport chạy liên tục, phục hồi kết nối và giữ audio chưa được xác nhận trong giới hạn.

**Approach:** Thêm port WebSocket có fake, state machine cho setup/reconnect/cancel, ring audio theo sample clock, tích hợp consent/key pool hiện có và ADR spike S3 về hành vi server thực.

## Boundaries & Constraints

**Always:** Setup không dịch có `inputAudioTranscription: {}` ở top level của `setup`, không có output transcription; `generationConfig` gồm `responseModalities:["AUDIO"]` và `translationConfig.targetLanguageCode` = ngôn ngữ nguồn (`auto`→`ja`) mà không có `echoTargetLanguage`; có `sessionResumption` và `contextWindowCompression.slidingWindow`. Gửi `realtimeInput.audio` bằng base64 PCM16 little-endian mono 16 kHz, 1600 sample/chunk, MIME `audio/pcm;rate=16000`. Nhận `setupComplete`, `goAway`, resumption update, input transcription; bỏ ngay output audio. Reconnect không giới hạn khi chạy với backoff `min(30s, 1s×2^n×(1±20%))`, reset sau setupComplete; hủy được handshake/sleep khi Dừng. Chỉ setup-reject thật đếm tới 5; transport lỗi không đếm. Giữ tối đa 600 chunks chưa được xác nhận và báo gap theo sample interval khi vượt giới hạn. Consent gate trước kết nối; lease `Priority::Live`, áp dụng FR-6 cho 401/403, 429, 400/404, không nhân retry lồng nhau; key/URL/transcript không vào error/log.

**Never:** Coi WebSocket send thành model ACK; tự tạo ACK từ event không được tài liệu/spike xác nhận; đánh dấu no-loss/no-duplicate bằng số lần send; phát/lưu audio output; tiếp tục reconnect sau Dừng. Không đưa transport logic vào WebView hay bypass gateway/key pool.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Setup | language `auto`/`ja`/`vi`/`en` | JSON shape chính xác, target đúng, không output transcription | setup-reject trước complete tăng bộ đếm |
| Audio | PCM chunk 1600 samples | một realtimeInput với base64 PCM LE và MIME 16 kHz | sai định dạng bị từ chối trước send |
| Rớt mạng/goAway | phiên còn chạy | dùng handle mới nhất, replay phần chưa xác nhận, backoff có jitter | lỗi transport không tính setup-reject |
| Buffer đầy | >600 chunk chưa xác nhận | giữ 600 mới nhất, gap `[start,end]` phần rơi | không chặn capture |
| Dừng | đang sleep/setup/send | kết thúc kịp thời, không mở lại socket | không rò key/URL/reason thô |
| Key lỗi | 401/403, 429, 400/404 | key pool loại/cooldown/đổi key theo FR-6 | hết key → Auth; setup reject đủ 5 → SetupRejected |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs` -- `ConsentSnapshot`, `CancellationToken`, gateway boundary; REST transport không dùng lại làm WebSocket, nhưng Live port phải nằm dưới `gemini/`.
- `src-tauri/src/gemini/keys.rs` -- `KeyPoolHandle`, `Priority::Live`, `RequestOutcome`, `ReportAction`; chính sách key duy nhất, tránh retry lồng nhau.
- `src-tauri/src/gemini/params.rs`, `src-tauri/src/core/model_defaults.rs` -- URL/model/giới hạn hiện có; không log URL có credential.
- `src-tauri/src/audio/mod.rs` -- `PcmChunk` và `CaptureController::subscribe()` cho 100 ms/16 kHz, sample offset dùng cho gap/replay.
- `src-tauri/src/settings/mod.rs` -- `TranscribeLanguage::as_code()` và `Settings.live_model`; `auto` cần target `ja` riêng cho setup.
- `src-tauri/src/core/error.rs`, `src-tauri/src/core/sensitive.rs`, `src-tauri/src/core/log.rs` -- AppError/redaction/key wrapper.
- `src-tauri/src/live/mod.rs` -- actor LiveSession thuộc story 4.6; 4.4 chỉ cung cấp port/events để 4.6 dùng.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/gemini/live/` -- typed setup/audio messages, WebSocket port + fake, production connector và parse server events theo tài liệu hiện hành.
- [x] `src-tauri/src/gemini/live/` -- reconnect state machine, cancel, resumption, buffer/gap và key classification dùng gateway key pool.
- [x] `src-tauri/src/gemini/mod.rs`, `src-tauri/Cargo.toml`, `Cargo.lock` -- expose Live gateway và pin WebSocket dependency tối thiểu.
- [x] `src-tauri/src/gemini/live/` -- deterministic tests cho từng hàng matrix, exact JSON, clock/replay, backoff, setup rejection, key rotation, stop mọi trạng thái và redaction.
- [x] `docs/adr/` -- ADR S3 phân biệt kết quả test giả với spike 60 phút thật, ≥6 disconnect, ACK/dedup, handle hết hạn, goAway, interruption, output audio/cost; ghi rõ chưa chứng minh nếu không có bằng chứng thực.

**Acceptance Criteria:**
- Given consent hiện hành và Live key, when mở phiên, then setupComplete dẫn tới stream audio và input transcription event; không giữ output audio.
- Given goAway hoặc lỗi transport, when phiên còn chạy, then reconnect không giới hạn, hủy được, và giữ clock/gap theo samples.
- Given 5 setup rejection liên tiếp, when chưa có setupComplete, then phát `SetupRejected`; thành công reset bộ đếm.
- Given spike S3 với 60 phút và ≥6 disconnect, when có bằng chứng ACK/replay/dedup thực, then ADR mới được kết luận không mất/lặp; thiếu bằng chứng thì để chưa nghiệm thu.

## Implementation Notes

- Gateway owns the WebSocket state machine and a 600-chunk sample-indexed ring. The recording path in story 4.5 remains independent.
- The S3 60-minute real-server spike was not run; the ADR records the missing evidence and avoids a no-loss/no-duplicate claim.
- Matrix audit: all six rows have passing deterministic tests (14 Live tests total). `cargo check --locked` and `npm run check` passed. Review selection was `none` per rendered workflow.

## Spec Change Log

## Review Triage Log

## Design Notes

API reference của Google đặt `inputAudioTranscription` ở `BidiGenerateContentSetup` top level và `translationConfig` dưới `generationConfig`; trang Live Translate có ví dụ khác về transcription, nên exact JSON theo story/API schema được khóa test và spike phải thử với server thật. Tài liệu chỉ mô tả resumption handle, không công bố ACK audio theo chunk. Ring chỉ loại chunk khi có tín hiệu xác nhận được chứng minh; nếu không, ADR phải nêu rõ giới hạn dedup, không suy diễn từ send thành công. Không thử key/model thật ngoài key pool hoặc ghi bí mật vào ADR.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml gemini::live` -- fake transport tests qua.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` -- Rust build qua.
- `npm run check` -- frontend/build bindings không lỗi.

**Manual checks:**
- Nếu có key/quota và môi trường kiểm soát disconnect, chạy spike 60 phút và điền bằng chứng vào ADR; nếu không, ghi trạng thái chưa nghiệm thu.
