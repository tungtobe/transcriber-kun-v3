---
title: 'Story 4.6: LiveSession — transcript realtime và trạng thái'
type: 'feature'
created: '2026-09-28'
status: 'done'
baseline_commit: '67188771da447228b2f6a1d7f75e28d4ff38d87b'
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

**Problem:** Capture, Recording và Gemini Live hiện là các phần rời; UI chưa có transcript realtime, trạng thái kết nối đáng tin hoặc Segment bền. **Approach:** Thêm một actor LiveSession sở hữu phiên/generation/seq, fan-out capture sang Recording và gateway, phát snapshot + sự kiện, flush Segment vào SQLite và nối IPC typed.

## Boundaries & Constraints

**Always:** Tối đa một LiveSession; `live_start(source,language,locale)` gate Consent hiện hành và key đã cấu hình nhưng không key-test/list-model online, mở capture rồi tạo Recording/row kể cả offline, WS nối sau. Hai nhánh capture độc lập không block callback. `live_subscribe(channel)` chụp snapshot `{seq,...}` rồi đăng ký nguyên tử; event `{ready,delta,turn,segment,gap,recording,connection,log,error,done,final}` có `seq` tăng đơn điệu xuyên generation; bỏ channel lỗi. `recording` và `connection` riêng; `connected` chỉ sau setupComplete, mất socket chuyển khỏi connected ngay. Text được tách câu khi gặp dấu kết; phần dở flush khi turn/generation/stop. Timestamp chỉ từ sample clock gốc của capture, không từ wall clock lúc nhận/replay; gap `disconnected` phản ánh đúng sample interval bị đẩy khỏi ring. Ghi Segment vào DB mỗi ≤5 s, DB là nguồn phục hồi; generation cũ bị bỏ và drain ≤1 s. Ghi Recording liên tục trong mạng lỗi; lỗi writer ngừng báo recording, dừng capture và phát category storage. Đồng bộ enum `connected` vào AD-10/addendum §H trước khi sinh TS binding.

**Never:** Suy diễn ACK/dedup hay timestamp chính xác của lời nói từ WebSocket send; gán timestamp replay theo lúc nhận lại; log key/URL/transcript; suy ra trạng thái connected từ Recording/ready; để sự kiện generation cũ ghi DB. Spike S3 thật chưa chạy, nên mọi khẳng định end-to-end không mất/lặp phải để chưa xác minh.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Start/busy | 2 lệnh start, consent/key hợp lệ | một phiên live/Recording, lệnh thứ hai lỗi rõ | thiếu consent/key từ chối trước capture |
| Offline/WS lỗi | capture còn chạy | WAV liên tục; connection connecting/reconnecting/stopped tách recording | lỗi model không dừng WAV |
| Remount | subscribe sát event khi connected/reconnecting | snapshot+cursor nguyên tử, seq không hụt/nhân đôi | channel gửi lỗi bị bỏ |
| Transcript | text delta, dấu câu, turn, generation mới | Segment không mất/lặp, timestamp sample gốc | event generation cũ bỏ |
| Persistence | nhiều câu/gap trong >5 s | DB flush ≤5 s, gap disconnected đúng interval | DB lỗi thành storage, không báo lưu thành công |
| Outage | 36,000 chunk/60 phút giả, ≥6 disconnect, outage 1,800 chunks | ring 600, gap 1,200 chunks (~120 s); WAV giữ 36,000 chunks | không claim server ACK/dedup thật |

</frozen-after-approval>

## Code Map

- `src-tauri/src/live/mod.rs`, `src-tauri/src/live/recording.rs` -- actor mới và `RecordingHandle::start_with_locale/terminal_errors/stop`; file writer độc lập, `stop` là blocking join nên chạy ngoài async executor.
- `src-tauri/src/gemini/live/mod.rs` -- `LiveGateway::run` hiện chỉ phát input text/gap; bổ sung connection/setup/turn và sample provenance từ chunk gốc, giữ redaction/reconnect.
- `src-tauri/src/audio/mod.rs` -- `CaptureController::set_source/subscribe/stop_capture`, sample clock process-wide; actor tính baseline session.
- `src-tauri/src/db/repo/transcripts.rs`, `segments.rs`, `sessions.rs` -- thiếu create empty primary + append/flush + duration/status; SQL chỉ trong repo.
- `src-tauri/src/ipc/mod.rs`, `ipc/boot.rs`, `src/lib/bindings.ts` -- nối command/channel, boot một actor, generate binding qua `export_bindings`.
- `src-tauri/src/transcribe/registry.rs` -- khuôn mpsc/oneshot và subscribe snapshot cùng lệnh actor.
- `_bmad-output/planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md`, `_bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/addendum.md` -- thêm connection `connected`.
- `docs/adr/0005-gemini-live-transport-spike-s3.md` -- giới hạn chứng cứ ACK/timing thật.

## Tasks & Acceptance

**Execution:**
- [x] `live/` -- actor + params flush, generation guard, sentence buffer, snapshots/events, capture/Recording/gateway lifecycle.
- [x] `gemini/live/` -- emit connection/turn/provenance transitions, gồm mất kết nối tức thì, fake tests.
- [x] `db/repo/` -- create live transcript, append ordered Segment batch transaction, session duration/status; tests recovery readback.
- [x] `ipc/`, `src/lib/bindings.ts`, architecture/addendum -- typed commands/channel và enum connected đồng bộ; binding sinh tự động.
- [x] Deterministic matrix tests, gồm 60-minute sample-clock simulation với ≥6 disconnect và 3-minute outage, cùng kiểm tra Recording liên tục.

**Acceptance Criteria:**
- Given capture giả và transport giả, when stream câu hoàn chỉnh, then Segment hiện nhanh và được flush bền ≤5 s theo sample clock.
- Given remount và reconnect, when subscribe, then snapshot phản ánh connection hiện hành; seq tiếp nối không phụ thuộc generation.
- Given server chưa có ACK/timing contract, when đánh giá replay, then ghi giới hạn chưa xác minh thay vì khẳng định no-loss/no-duplicate.

## Implementation Notes

- Actor đăng ký hai receiver trước khi mở capture, quản lý generation/seq và snapshot+cursor nguyên tử; Recording xả PCM đã xếp hàng khi dừng bình thường.
- Segment câu được phát ngay, ghi theo transaction cùng duration; ngưỡng sample 4,6 s chừa biên cho chunk 100 ms và poll 250 ms. Gap `disconnected` đọc lại được từ DB.
- Gateway phát `connected` sau `setupComplete`, giữ `sinceMs` qua reconnect, xử lý socket close khi ring còn audio. Ring chỉ báo gap cho chunk chưa từng gửi; send không phải ACK.
- Đã đối chiếu đủ 6 hàng I/O matrix với 49 test `live::` qua, gồm channel lỗi bị bỏ mà stream khỏe vẫn tiếp tục; test 60 phút là mô phỏng client, không phải kết quả server thật. Nhánh cleanup khi `spawn_blocking` panic chưa có fault-injection riêng.

## Spec Change Log

## Review Triage Log

## Design Notes

Gemini 4.4 chưa có ACK theo chunk hay timestamp transcription. Provenance từ sample chunk giúp tránh dùng thời gian nhận lại sau replay, nhưng không chứng minh được mốc từng từ/câu chính xác. Test giả chỉ chứng minh client state/gap/Recording; spike S3 thật vẫn là điều kiện cho claim server. UI màn Live thuộc 4.7, stop/finalize Proxy thuộc 4.9; 4.6 cần lifecycle nội bộ để flush phần dở và test.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml live:: -- --test-threads=1` -- 49/49 qua.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml ipc::tests::export_bindings` -- qua; binding được sinh.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` -- qua.
- `npm run check` -- 0 lỗi, 0 cảnh báo.
