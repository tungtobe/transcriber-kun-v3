---
title: Addendum — trans-kun v3 PRD
created: 2026-09-17
updated: 2026-09-18
---

# Addendum: chi tiết kỹ thuật & vận hành chuyển cho architecture / epics

Nội dung ở đây **không phải yêu cầu sản phẩm**; là các quyết định cách làm, dữ liệu vận hành và bài học đã có, để architecture và story không phải tìm lại. Nguồn chính: `docs/rebuild-v3/02..05`. Khi repo mới được tạo, copy thư mục `docs/rebuild-v3/` sang cùng PRD này.

## A. Quyết định đã chốt (chủ sản phẩm, 2026-09-13/14) — đầu vào cố định

| # | Quyết định |
|---|---|
| Q1 | Bỏ hoàn toàn Copilot ở v3 |
| Q2 | Proxy phát lại FLAC (16 kHz mono) |
| Q3 | macOS tối thiểu 14.4, chỉ Core Audio process tap, không ScreenCaptureKit |
| Q4 | Quảng cáo: banner nhỏ, house ads tự vận hành, không mạng bên thứ ba |
| Q5 | Không kênh phân phối trực tiếp (chỉ store) |
| Q6 | Bỏ Linux |
| Q7 | Bỏ cache dir tuỳ chỉnh |
| Q8 | Không import dữ liệu v2 |
| Q9 | Tên app trans-kun, bundle ID `com.transkun.app` |
| Q10 | Model mặc định như v2: `gemini-flash-lite-latest` (transcribe/memo), `gemini-3.5-live-translate-preview` (live) |
| Q11 | Premium + đăng nhập Google để phiên bản sau; v3 chỉ chừa trait `KeyProvider` và cờ `is_premium` |

Bổ sung trong phiên PRD 2026-09-17: Windows 10 1809+/11, x64 + Arm64, WebView2 Evergreen; Ad slot không hiển thị ở màn Live; không có chỉ số thành công định lượng; cache-hit khi trùng file → mở Phiên có sẵn (như v2), chạy lại là thao tác chủ động; 401/403 → loại key và thử key kế (được vì không còn Files API gắn file với key upload).

Bổ sung 2026-09-18 (chủ sản phẩm duyệt ở bước UX/architecture): không gửi thống kê ẩn danh ở v3, chỉ đếm cục bộ; Live bắt đầu được khi offline (Phiên + Recording tạo khi mở capture thành công); không hiển thị speaker trên UI; Dừng Live → điều hướng sang Transcript detail; tải Recording chỉ WAV/FLAC; theme sáng/tối trong bản submit đầu (FR-48).

**Nguồn cách làm:** từ 2026-09-18, `architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md` là nguồn quyết định kỹ thuật (AD-1…AD-19, bảng Stack có version). Addendum giữ dữ liệu vận hành và bối cảnh; khi mâu thuẫn, spine thắng.

## B. Stack mục tiêu (doc 04 §2)

Tauri 2 · Rust 2021 + tokio · Vite + TypeScript 6 + Svelte 5 SPA một cửa sổ · IPC typed bằng `tauri-specta` · SQLite (`rusqlite` bundled + `rusqlite_migration`) · settings trong bảng `settings` của SQLite do Rust sở hữu (**không** `tauri-plugin-store`), key qua `keyring` · HTTP `reqwest` feature `rustls` (platform verifier, xác minh qua CA store OS), WS `tokio-tungstenite` (`rustls-tls-native-roots`) · media `symphonia` + `rubato` + `flacenc` + `hound` · audio I/O `cpal` + Core Audio tap (`objc2`/`coreaudio-sys`) + WASAPI loopback · ID `uuid` v7, hash `sha2`, chữ ký `ed25519-dalek` · logging `tracing` + `tracing-appender` · test `cargo test`, Vitest, WebDriver smoke. Version cụ thể: bảng Stack của spine.

Không có: Python, ffmpeg, sidecar, `tauri-plugin-{shell,fs,updater,process,http,store}` (spine AD-17).

Frontend phụ trợ: i18n bằng `i18next` hoặc module tự viết (chốt ở story nền tảng) với lint key set trong CI, migrate ≈ 400 key từ v2 (bỏ nhóm setup/whisper/copilot); Markdown bằng `marked` + `dompurify` qua npm (thay `marked.min.js` vendored). Mọi dependency pin trong `Cargo.lock` / `package-lock.json`; RC (tauri-specta, specta) pin exact; không có runtime tải về.

## C. Module map (theo spine — Design Paradigm, Structural Seed)

Bốn tầng, phụ thuộc chỉ đi xuống: `ipc/` (adapter UI, boot, đóng cửa sổ, điều phối chéo feature) → feature `settings/`, `library/{sessions,tags,notes,export}`, `transcribe/` (FileJob, JobRegistry), `live/` (LiveSession, generation guard, restart, finalize, orphan recovery), `memo/`, `ads/` → hạ tầng `gemini/{keys,params,rest,live}`, `media/{decode,resample,probe,chunk,proxy,wav}`, `audio/{capture,coreaudio_tap,wasapi,playback,output_volume}` (port từ v2), `db/{migrations,repo}`, `secrets/`, `remote/` → `core/` (AppError, ID, `Sensitive<T>`, đồng hồ). Actor chỉ cho `LiveSession`, `JobRegistry`, `KeyPool`.

## D. Gemini — shape đã verify (doc 01 §3–4, doc 04 §3.1–3.2)

- REST: `POST /v1beta/models/{model}:generateContent`, header `x-goog-api-key`; audio Chunk gửi `inline_data {mime_type: audio/flac}`; `responseMimeType: application/json` + `responseSchema` mảng `{start:"MM:SS", end:"MM:SS", text}`; giới hạn request inline 20 MB (đã tính base64) → Chunk 5 phút FLAC 16 kHz mono ≈ 3–6 MB, tự hạ 3 phút nếu > 14 MB.
- Transcribe file dùng model generateContent thông thường, prompt tự nhận diện lời Việt–Nhật xen kẽ mà không dịch. Spike S2 kiểm chứng inline FLAC/JSON schema trên model đã chọn; không dùng Interactions/Files API cho file.
- Thinking config chỉ gắn cho model ID đã kiểm chứng. Alias và tên tự nhập dùng thinking mặc định của model; không đoán major version từ chuỗi.
- Live WS: `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=…`; `inputAudioTranscription`/`outputAudioTranscription` ở **top-level setup** (không trong generationConfig); `generationConfig.responseModalities: ["AUDIO"]`, `translationConfig {targetLanguageCode, echoTargetLanguage: true}`; `sessionResumption` + `contextWindowCompression.slidingWindow`; audio `realtimeInput.audio {data: base64 PCM16, mimeType: "audio/pcm;rate=16000"}` mỗi 100 ms; `goAway` → reconnect trong suốt; resend chunk chưa ack.
- Chính sách reconnect (FR-22): mọi lỗi transport (timeout, EOF, DNS, TLS, close code ≠ setup-reject) → reconnect với exponential backoff `min(30 s, 1 s × 2^n)` + jitter ±20 %, không giới hạn số lần khi Phiên còn chạy; reset `n` sau khi `setupComplete`. `MAX_FAILURES=5` chỉ đếm lần server từ chối setup (1007/1008 trước `setupComplete`, 401/403/404 ở handshake). Buffer audio pending tối đa 60 s (600 chunk 100 ms) dạng ring buffer; phần bị đẩy ra ghi thành Khoảng thiếu vào Transcript live. Capture → WAV writer không đi qua task WS: mất WS không ảnh hưởng ghi file. Phát hiện mạng có lại: thử reconnect theo backoff, không poll OS network API.
- Lỗi phân loại (code nội bộ): `Quota | Auth | Model | Request | Timeout | Network | Blocked | Shape`, thêm `Tls` cho CA/proxy doanh nghiệp. Ánh xạ sang 8 category hiển thị (`quota | auth | model | network | format | permission | storage | blocked`) chỉ nằm ở `core/error` (spine AD-7).
- Restart Live (Nhận diện lại / đổi Target / "Không dịch"): `LiveGeneration { id, ws_task, clock_offset }`; tạo generation mới → chờ `setupComplete` → swap sender dưới gate → drain generation cũ ≤ 1 s; sự kiện mang id cũ bị bỏ. Tắt dịch = setup không có `outputAudioTranscription`, target = source (hoặc `ja` khi auto), không `echoTargetLanguage`.
- Vận hành: smoke test tự động hằng tuần với key test cho model Live preview và model transcribe mặc định (D8); kết quả lỗi → cập nhật `recommended-settings.json` để người dùng đồng bộ model mới (FR-42).
- Tham số vận hành: chunk max attempts 4; max wait 180 s/chunk; quota cooldown 60 s; timeout transcribe chunk 120 s, memo 90 s; timeout không fan-out.

## E. Media pipeline (doc 04 §3.3)

| Việc | Cách |
|---|---|
| Duration | symphonia `n_frames / sample_rate`, fallback decode đếm |
| Tách audio khỏi video | demux track audio đầu tiên → decode → f32 mono (streaming) |
| Cắt Chunk | buffer 16 kHz mono theo `chunk_seconds` → FLAC bytes |
| Proxy | FLAC 16 kHz mono (≈ 55 MB/giờ); tuỳ chọn `aac-native` (AudioToolbox / Media Foundation) nếu S8 thất bại |
| Recording | WAV 16 kHz mono PCM16, vá header mỗi ~160 000 byte (~5 s) |
| Hash Phiên | SHA-256 của file nguồn |

Định dạng: mọi container symphonia demux được (mp3, m4a/mp4/mov AAC/ALAC, mkv/webm AAC/Vorbis, wav, flac, ogg, aiff, caf). Opus cần crate riêng (Open Question 1). `avi/wmv/flv/ts` không hỗ trợ.

## F. Audio capture & playback (doc 02 §E)

- macOS: Core Audio process tap **global**, loại trừ PID chính app (giải quyết vòng lặp TTS); `NSAudioCaptureUsageDescription` + `NSMicrophoneUsageDescription`; không có API hỏi quyền trước, hệ thống hỏi khi tạo tap lần đầu; binary phải ký; TCC khoá theo Team ID → dev build cũng ký bằng cert team.
- Windows: WASAPI loopback cả endpoint console + communications rồi cộng; cùng device → capture một lần.
- Playback TTS trong tiến trình Rust (cpal), supervisor poll thiết bị output mặc định mỗi 1 s, stall 2.5 s coi như chết; ducking 30 % với marker file phục hồi sau crash; không "đánh nhau" với người dùng.
- Đổi Nguồn audio giữa phiên: gate mềm (thiết bị vẫn mở, nguồn tắt góp silence).

## G. Lưu trữ (doc 04 §3.4)

SQLite `app.db` (WAL, một connection do `db/` giữ) + `media/`, `cache/`, `state/` trong Container. Phác thảo theo spine AD-4, AD-5, AD-8, AD-16 (cột chính xác chốt ở story `db/`):

```
sessions(id UUIDv7, kind file|live, title, source_hash UNIQUE NULL, source_name,
         status recording|finalizing|complete, recovered, created_at, duration_sec)
transcripts(id UUIDv7, session_id, variant primary|retranscribe, status complete|partial,
            model, language, created_at)
segments(transcript_id, idx, start, end, kind text|gap, gap_reason chunk_failed|disconnected NULL,
         text, speaker NULL)
tags(id, name UNIQUE COLLATE NOCASE)  session_tags(session_id, tag_id)
notes(session_id PK, body, updated_at)  memos(session_id, template_id, body, created_at)
memo_templates(id, name, prompt, is_default)
settings(key, value)
```

- Không có bảng `jobs`: Job không sống qua restart, `JobRegistry` trong bộ nhớ là nguồn sự thật.
- Chỉ Phiên live đi qua `recording → finalizing`; lúc boot, Phiên còn ở hai trạng thái này là mồ côi → `complete` + `recovered`.
- Transcript `partial` ⇔ có gap `chunk_failed`. Chạy lại dựng transcript mới rồi swap nguyên tử thay `primary`; Transcribe lại tạo `retranscribe`, không bao giờ thay bản live.
- Job file làm việc trong `media/.staging/<job-id>/`, commit session + transcript + proxy trong một transaction lúc kết thúc; huỷ → xoá staging.
- Mốc thời gian lưu trữ = epoch ms UTC; `start/end` = giây `f64` tuyệt đối.
- File: `media/<session-id>/{proxy,recording}.<ext>`, `cache/remote/<sha256-của-url>`, `state/ducking.json`. Tên file chỉ từ ID tự sinh (NFR-12).

Một chủ sở hữu schema (Rust), migration versioned chỉ tiến. Không import v2.

## H. IPC gợi ý (doc 04 §3.6)

Tên command theo spine: `snake_case` `<domain>_<action>`, binding TS sinh bằng tauri-specta. Nhóm: `settings_get/save`, `settings_recommended_preview/apply`, `keys_set/test`, `models_list(kind)`, `library_list/search/get/rename/delete`, `tags_set/list/delete`, `notes_save`, `memo_templates_*`, `memo_generate`, `transcribe_start` (trùng hash → trả `Existing { session_id }`)/`transcribe_cancel`, `jobs_subscribe`, `live_sources`, `live_start/stop/set_source/set_target/redetect/set_tts`, `live_subscribe`, `export_*`, `diagnostics_export/clear`, `ads_next/report`.
Đồng bộ UI: `*_subscribe(channel)` trả `Snapshot { seq, … }` rồi stream qua `Channel` (spine AD-3); mọi event có `seq`. `LiveEvent`: ready, delta, turn, segment, delta_translated, segment_translated, gap, recording `{state}`, connection `{connecting | connected | reconnecting{sinceMs} | stopped}`, speaking, log, error, done, final. `connected` chỉ sau `setupComplete`; khi socket bị đóng trạng thái đổi khỏi `connected` ngay. `JobEvent`: progress, log, waitingQuota, result, error, cancelled. Lỗi qua IPC là `AppError { category, code, detail_redacted }`. TTS audio không đi qua IPC.

## I. Ad slot (doc 03 §3)

Bối cảnh thanh toán cho phiên bản sau: Apple bắt buộc StoreKit IAP (3.1.1); Microsoft cho phép app non-game dùng cổng thanh toán riêng hoặc song song Microsoft commerce (Windows.Services.Store cần package identity → MSIX), hoa hồng khác theo đường, xác nhận khi submit.

`ads.json` và `recommended-settings.json` ký ed25519 (public key nhúng binary, private key giữ ngoài repo app), host trên GitHub Pages LP repo hoặc Cloudflare Workers + KV (Câu hỏi mở 6), chỉ tải qua `remote/`; cache 24 h; fallback Creative nhúng; Creative `{id, image ≤ 100 KB, title, sponsor, url, locale[], start, end, weight}`; 300×100 hoặc 320×50; cap 1 Creative/10 phút; cờ `ads_enabled` server; `is_premium` cục bộ. Redirect URL có tham số chiến dịch, không định danh người dùng.

## J. Build & phân phối (doc 03, doc 04 §5)

- `store-mac`: overlay `tauri.appstore.conf.json` (entitlements sandbox: app-sandbox, network.client, device.audio-input, files.user-selected.read-write, keychain-access-groups nếu `keyring` cần trong sandbox (spike S5); application-identifier `B2U85XPU55.com.transkun.app`; provisionprofile; category; minimumSystemVersion 14.4); universal; `productbuild` ký "3rd Party Mac Developer Installer"; upload `xcrun altool`; TestFlight for Mac.
- `store-win`: `winapp` CLI hoặc `tauri-windows-bundle` → MSIX x64 + Arm64; Store ký; capability `microphone`; WACK; package flight.
- Tài khoản & chứng chỉ: Apple Developer Program (team RELIPA `B2U85XPU55`), App ID `com.transkun.app` đăng ký mới, cert **Apple Distribution** (ký app) + **Mac Installer Distribution** (ký .pkg; `productbuild --sign` gọi tên cũ "3rd Party Mac Developer Installer" — cùng loại cert), provisioning profile Mac App Store Connect, App Store Connect API key. Microsoft: Partner Center developer account (phí một lần theo khu vực), reserve tên, `Package.appxmanifest` (identity, publisher từ Partner Center, capability `microphone`), bộ icon assets.
- Hồ sơ submit: screenshot theo kích thước từng store, mô tả vi/en/ja, category Productivity, age rating 4+, Privacy Policy URL, support URL, review notes + key demo.
- CI GitHub Actions 2 job; secrets cert / App Store Connect API key / Partner Center.
- Version một nguồn (bài học I1).

## K. Phương án dự phòng nếu Apple từ chối BYOK

Kích hoạt khi: App Review từ chối với lý do app không dùng được nếu không có key/dịch vụ bên ngoài, và key demo trong review notes không được chấp nhận. Phương án: backend nhỏ của Relipa (Cloudflare Workers) cấp **ephemeral token** Gemini theo phiên (Google hỗ trợ ephemeral token cho Live API) cho người dùng đã đăng nhập Google; app gọi qua trait `KeyProvider` (Q11) nên không đổi luồng Gemini. Chi phí token do Relipa chịu → đi kèm Premium/IAP. Ước tính 2–3 tuần nếu phải kéo lên trước submit. Không triển khai trong v3 trừ khi kích hoạt.

## L. Lộ trình & spike (doc 05)

12–15 tuần, 1–2 dev + AI agent. Phase 0 (tuần 1–2) spike bắt buộc: S1 media thuần Rust, S2 Gemini inline/Interactions, S3 Live WS 60 phút, S4 Core Audio tap trong sandbox, S5 MAS build + TestFlight, S6 MSIX, S7 IAP (chỉ nếu chọn freemium — không ở v3), S8 proxy FLAC phát trong WebView. Phase 1 nền tảng → 2 transcribe file → 3 quản lý phiên/tag/memo/notes → 4 live → 6 store readiness + ads → 7 ra mắt (v2 ngừng phát hành sau 3 tháng). Định nghĩa xong từng phase và bảng rủi ro ở doc 05. Tiêu chí QA Phase 2 "khớp bản cũ ± timestamp": cùng file mẫu, so với transcript v2 bằng cùng model → độ lệch timestamp Segment ≤ 2 s, không mất đoạn; text khác biệt chấp nhận (model không tất định).

## M. Bài học phải tôn trọng khi thiết kế (doc 02, chọn lọc)

*Mã A1…K trong mục này là mã bài học của doc 02, không phải mục A–N của addendum.*

- Gọi REST/WS thuần, không SDK (A6, D1, D2); test khoá exact JSON shape.
- Timestamp `MM:SS` tương đối Chunk, Chunk 5 phút (C1); parser chịu lỗi (C2); Khoảng thiếu không bao giờ bị cache như hoàn chỉnh (C3).
- Generation guard cho WS restart (A8, D6); sanitizer redaction cho close reason (D7).
- Recording tạo sau khi *mở thiết bị capture* thành công (không chờ WS, nên Live chạy được khi offline) hoặc dọn ở mọi nhánh lỗi (F3, diễn giải 2026-09-18).
- Schema một chủ sở hữu (F4); i18n key set lint trong CI (H3); tooltip mọi setting (H5).
- Log content-free là bất biến, có test grep (G11).
- Deferred bug v2 không tái tạo: request rơi âm thầm khi worker vắng, delta mất trong lúc respawn, ID chưa sanitize, UI báo recording khi không còn worker.

## N. Tư liệu Copilot (không phải yêu cầu v3)

Doc 01 §2.6 và doc 02 §G; prompt YAML `app/python/skills/<profile>/*.yaml` trong repo v2. Chỉ dùng nếu phiên bản sau làm lại.
