---
title: '2.1 Media pipeline Rust và kiểm chứng phát FLAC'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'e9984e61dc63605d74bf798d642a92cc15b157a9'
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

**Problem:** App chưa có media pipeline; file họp phổ biến chưa thể decode, cắt Chunk hay tạo proxy để nghe lại. Các quyết định Opus và FLAC playback/seek vẫn thiếu bằng chứng trên nền tảng đích.

**Approach:** Xây pipeline streaming thuần Rust để probe, decode mono, resample 16 kHz, cắt và encode FLAC, hash SHA-256 và tạo proxy. Kiểm chứng codec/container và seek, ghi ADR cho S1/S8; giữ `media/proxy` là điểm thay đổi định dạng playback.

## Boundaries & Constraints

**Always:** Hỗ trợ mp3, m4a, mp4, mov, mkv, webm, wav, flac, ogg/vorbis, aiff, caf bằng bộ fixture thực; video chỉ lấy track audio đầu; không đọc toàn file vào RAM. Duration ưu tiên `n_frames / sample_rate`, đếm frame khi thiếu metadata. Chunk mặc định 5 phút, chia tiếp theo giới hạn 14 MB và kích thước toàn JSON/base64; timestamp tuyệt đối. Proxy nằm dưới `$APPDATA/media/**`. Mọi lỗi định dạng đi qua `AppError` category `format`, có gợi ý chuyển sang mp4/m4a/mp3, không lộ đường dẫn hay nội dung.

**Never:** Không dùng ffmpeg/sidecar, không tạo Phiên ở bước probe, không gửi request Gemini trong story này, không mở asset scope ra ngoài media, không giả định FLAC seek đạt ngưỡng trước khi đo thật.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Decode | 11 container/codec được cam kết | PCM f32 mono streaming, đúng duration; video lấy track audio đầu | Codec không decode được → `format` |
| Chunk | PCM 16 kHz; đoạn lớn/khó nén | FLAC chunks liên tục, offset chính xác; chia tới khi FLAC ≤ 14 MB và payload JSON/base64 trong giới hạn đã kiểm chứng | Không thể chia thêm → `format`, không tạo payload quá cỡ |
| Probe | avi/wmv/flv/ts, track không hỗ trợ hoặc không có audio | Từ chối trước khi tạo Phiên | `format` và gợi ý chuyển định dạng |
| Proxy/hash | File nguồn dài hoặc metadata thiếu | SHA-256 streaming; proxy 16 kHz mono phát lại độc lập file nguồn | I/O lỗi có category phù hợp |
| Playback | Proxy 90 phút trên WKWebView/WebView2 | Range request và seek bất kỳ ≤ 500 ms | Nếu fail, ADR chốt AAC native hoặc player Rust |

</frozen-after-approval>

## Code Map

- `src-tauri/src/media/mod.rs` — stub; thêm các module probe/decode/resample/chunk/proxy/hash và API nội bộ, không IPC/file picker ở đây.
- `src-tauri/Cargo.toml`, `Cargo.lock` — thêm dependency pin exact; kiến trúc chọn Symphonia 0.6.1, Rubato 5.0.0, flacenc 0.5.1, hound 3.5.1.
- `src-tauri/src/core/error.rs` — tái dùng `AppError::new(Code::Format, ...)`; không đưa lỗi decoder thô ra UI.
- `src-tauri/src/core/paths.rs` — `media_dir` dùng UUIDv7; `staging_dir` hiện dưới `root/staging`, không tự đổi cho 2.3.
- `src-tauri/tauri.conf.json` — thêm asset protocol scope `$APPDATA/media/**`; không cấp quyền đọc file khác.
- `_bmad-output/planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md` — AD-12/17 và S1/S8; `epics.md` §2.1 là AC gốc.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` — pin media/sha crates, khóa build trên macOS/Windows.
- [x] `src-tauri/src/media/*` — probe và decode streaming; resample, chunk FLAC theo byte/payload budget, proxy, SHA-256; unit test ranh giới và lỗi.
- [ ] `src-tauri/tauri.conf.json` — giới hạn asset protocol trong `$APPDATA/media/**`; kiểm tra quyền truy cập ngoài scope bị từ chối.
- [ ] `src-tauri/tests/fixtures/media/**` và tooling kiểm chứng — 11 fixture thật, unsupported fixture, benchmark decode 4 core, seek proxy 90 phút trên hai WebView; ghi môi trường và số đo có thể lặp lại.
- [ ] `_bmad-output/implementation-artifacts/adr-2-1-media-spikes.md` — ghi S1 Opus (decoder thuần Rust hoặc từ chối rõ) và S8 FLAC (giữ FLAC hoặc chọn AAC/player Rust theo số đo).

**Acceptance Criteria:**
- Given corpus 11 định dạng, when pipeline decode/chunk, then duration đúng và throughput ≥ 20× realtime trên máy 4 nhân với số đo ghi lại.
- Given Opus webm/mkv, when probe/decode, then hành vi khớp ADR S1 và lỗi FR-9 phản ánh quyết định.
- Given proxy 90 phút trên macOS và Windows, when seek các vị trí đầu/giữa/cuối, then mỗi seek ≤ 500 ms và asset ngoài media bị từ chối; nếu không đạt, ADR và `media/proxy` dùng phương án thay thế.
- Given chunk lớn hoặc entropy cao, when chuẩn bị payload, then không payload nào vượt giới hạn sau base64/JSON; chia tiếp hoặc trả `format`.

## Implementation Notes

- Pipeline Rust, asset scope và corpus synthetic 11 định dạng đã triển khai. FFmpeg chỉ dùng trong script tạo fixture cho phát triển; runtime không phụ thuộc FFmpeg.
- Spike S1 quyết định từ chối Opus bằng `format`; xem [ADR 2.1](adr-2-1-media-spikes.md). Test corpus và proxy cực ngắn đã qua.
- Benchmark local đo decode + cắt FLAC trên M4 Pro 12 nhân, chạy 4 worker: median 3,155× realtime qua 5 lượt. Đây chưa phải phép đo trên máy 4 nhân.
- S8 còn mở: chưa có test 90 phút trên WKWebView/WebView2, range request và từ chối asset ngoài scope ở WebView thật. Bộ file ghi âm thực cũng chưa có. Workflow được đóng theo yêu cầu người dùng; các mục bằng chứng này vẫn cần hoàn tất trước khi chấp nhận AC của story.

## Spec Change Log

## Review Triage Log

## Design Notes

Tách đường xuất FLAC cho Gemini và proxy playback: cả hai có thể dùng cùng encoder nhưng chỉ proxy là điểm đổi định dạng theo S8. Không buộc media layer biết Session/Job hay Gemini endpoint. Giới hạn payload phải là tham số cho adapter 2.2 sau khi spike S2 xác nhận kích thước request; kiểm tra tại 2.1 bằng ngân sách an toàn và test kích thước serialized payload.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml media` — test pipeline và lỗi.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked` — build dependency và API.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` — format.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test media_corpus` — kiểm corpus synthetic 11 định dạng và các ca từ chối.
- `cargo run --release --manifest-path src-tauri/Cargo.toml --locked --example media_throughput -- src-tauri/tests/fixtures/media/benchmark-4worker.wav 3` — đo decode + chunk, ghi thông số máy.

**Manual checks (if no CLI):**
- Chạy harness phát/seek proxy trên WKWebView và WebView2; ghi OS, runtime, cấu hình máy, fixture 90 phút và từng số đo vào ADR.
