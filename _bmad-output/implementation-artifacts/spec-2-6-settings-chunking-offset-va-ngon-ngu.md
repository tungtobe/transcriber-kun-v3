---
title: '2.6 Settings — Chunking, offset và ngôn ngữ transcribe'
type: 'feature'
created: '2026-09-23'
status: 'done'
baseline_commit: 'e975817449c77777550fd48f31a732a67943054a'
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

**Problem:** Độ dài Chunk (300 s), ngôn ngữ transcribe (luôn auto) đang cố định trong code và chưa có offset timestamp, nên người dùng không chỉnh được theo file và cách họp của mình (FR-13, FR-16, FR-39).

**Approach:** Thêm ba trường Settings bền — `chunkMinutes` (mặc định 5), `timestampOffsetSec` (mặc định 0), `transcribeLanguage` (`auto|ja|vi|en`, mặc định `auto`) — với nhóm Settings mới "Chunking" (chunk + offset) và trường ngôn ngữ trong nhóm Gemini. Job transcribe file và Chạy lại chụp `chunkMinutes` + ngôn ngữ (cùng model) lúc nhận Job; offset chỉ là tuỳ chọn hiển thị/export qua một helper định dạng thời gian dùng chung.

## Boundaries & Constraints

**Always:** `chunkMinutes` là số nguyên ≥ 1; `timestampOffsetSec` là số nguyên ≥ 0 (quyết định: không offset âm). UI chặn tại chỗ giá trị không hợp lệ (rỗng, không phải số nguyên, nhỏ hơn mức tối thiểu) kèm giải thích và không lưu; Rust `settings::save` cũng từ chối (`Code::Format`), `load` fallback mặc định khi thiếu/hỏng mà không làm mất khoá khác. Mọi trường mới dùng `SettingsRow` có icon (?) tooltip + helper text bền. Job chụp `model`, `language`, `chunkMinutes` lúc `transcribe_start`/`transcribe_rerun`; đổi Settings giữa chừng không ảnh hưởng Job đang chạy/chờ. `auto` giữ nguyên prompt và request JSON hiện có byte-for-byte; `ja|vi|en` chỉ thêm một câu chỉ định ngôn ngữ chính, không dịch. `transcripts.language` lưu mã ngôn ngữ đã chụp (`NULL` khi `auto`). Offset không bao giờ ghi vào Segment/DB; helper định dạng `HH:MM:SS` (bỏ giờ khi < 1 giờ) cộng offset và phản ứng tức thì khi Settings đổi.

**Never:** Không migration (settings là key/value sẵn có). Không sửa Segment đã lưu khi đổi offset, không chạy lại Gemini. Không đổi ngân sách inline/tự chia Chunk quá lớn của `media::chunk`. Không làm Live (Epic 4) — chỉ lưu ngôn ngữ để Live dùng sau; không thêm `language_codes`. Không Transcript detail/export (2.7/2.10) ngoài helper định dạng.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Mặc định | DB chưa có khoá mới | `chunkMinutes` 5, offset 0, ngôn ngữ `auto` | — |
| Chunk hợp lệ | Nhập `3` | Lưu, `SettingsChanged` phát; Job mới cắt Chunk 180 s | — |
| Chunk không hợp lệ | `0`, `-2`, `1.5`, `abc`, rỗng | Không lưu, giá trị đã lưu giữ nguyên | Thông điệp tại chỗ (i18n) |
| Offset không hợp lệ | `-5`, `2.5`, `abc`, rỗng | Không lưu | Thông điệp tại chỗ |
| Rust chặn | `save` với `chunkMinutes = 0` hoặc offset âm (nếu kiểu cho phép) | Không ghi dòng nào | `AppError` `Format` |
| Khoá hỏng | JSON hỏng / ngôn ngữ lạ trong DB | Khoá đó về mặc định, khoá khác giữ nguyên | Log cảnh báo không lộ giá trị |
| Ngôn ngữ `auto` | Transcribe file | Request JSON giống hệt trước story này; `transcripts.language` NULL | — |
| Ngôn ngữ `ja` | Transcribe file / Chạy lại | Prompt thêm câu chỉ định tiếng Nhật; `transcripts.language = 'ja'` | — |
| Đổi giữa Job | Job đang chạy với 5 phút/`auto`, đổi sang 2 phút/`vi` | Job hiện tại vẫn 300 s/`auto`; Job kế tiếp dùng 120 s/`vi` | — |
| Offset hiển thị | Segment 65 s, offset 3600 | `01:01:05`; offset 0 và 65 s → `01:05`; Segment DB vẫn 65 s | — |
| Offset đổi | Đang xem, đổi offset | Giá trị định dạng cập nhật ngay, không gọi IPC transcribe | — |

</frozen-after-approval>

## Code Map

- `src-tauri/src/settings/mod.rs` -- `Settings` (serde camelCase, specta), `Default` thủ công, `KEY_*`, `load` (match từng khoá, fallback + `tracing::warn!` không lộ giá trị), `save` (`require_non_blank_model`, `upsert_many` một transaction), nhiều test mẫu `corrupt_*`/`legacy_rows_*`. Thêm 3 trường + enum `TranscribeLanguage` + validation tương tự.
- `src-tauri/src/transcribe/adapter.rs:14` `PROMPT`, `:82` `build_general_request(model, chunk)` -- thêm tham số ngôn ngữ; `auto` phải giữ exact-shape test 2.2 xanh.
- `src-tauri/src/transcribe/registry.rs` -- `DEFAULT_CHUNK_SECONDS` (`:40`), `estimate_chunk_count`, `StartParams`/`RerunParams` (có `model`), `Chunker::new(ChunkOptions::default())` ở `decode_and_transcribe`, `ChunkOptions::default()` truyền vào `decode_ranges_and_chunk`, `TranscriptDraft { language: None }` ở commit/swap; trait `ChunkTranscriber::transcribe(model, …)` + `GatewayTranscriber` + `FakeTranscriber` tests. Chuyền `language` + `chunk_seconds` từ params.
- `src-tauri/src/media/chunk.rs:32-44` `ChunkOptions { max_duration_seconds, budget }` -- dựng từ `chunkMinutes * 60`, giữ `budget` mặc định.
- `src-tauri/src/transcribe/rerun.rs` `decode_ranges_and_chunk(…, options, …)`.
- `src-tauri/src/ipc/mod.rs` -- `transcribe_start_inner` (`:542`, đã `settings::load` để lấy model/consent) và `transcribe_rerun_inner` -- chụp thêm chunk/ngôn ngữ vào params. Tái sinh `src/lib/bindings.ts` bằng test `export_bindings`.
- `src/lib/stores/settings.svelte.ts` -- hàng đợi save lạc quan (`setTheme`, `setUiLanguage`, `setModel` `:392`, `normalizeSettings`); thêm setter cho 3 trường.
- `src/routes/Settings.svelte:14` mảng `groups` + nhánh `{#if}`; `src/routes/settings/SettingsGemini.svelte` (mẫu `SettingsRow` label/help/helperText/fieldId, validation inline model); tạo `src/routes/settings/SettingsChunking.svelte` (+ test) theo mẫu `SettingsGeneral.svelte`.
- `src/components/SettingsRow.svelte` -- row có tooltip (?) + helper text (UX-DR11).
- `src/lib/` -- chưa có helper thời gian: tạo `src/lib/time.ts` (+ test).
- i18n `src/i18n/{vi,en,ja}.json`, kiểm bằng `npm run check:i18n`.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/settings/mod.rs` -- `chunk_minutes: u32` (mặc định 5), `timestamp_offset_sec: u32` (mặc định 0), `transcribe_language: TranscribeLanguage` (`auto|ja|vi|en` lowercase, mặc định `Auto`); load/save + validation `chunk_minutes >= 1`; tests mặc định, round-trip, hỏng từng khoá, khoá cũ thiếu, save từ chối 0 không ghi dòng nào.
- [x] `src-tauri/src/transcribe/adapter.rs` -- `build_general_request(model, chunk, language)`: `Auto` → body không đổi; khác → prompt thêm một câu chỉ định ngôn ngữ; test exact shape cho `auto` và một ngôn ngữ cụ thể.
- [x] `src-tauri/src/transcribe/{registry,rerun}.rs` -- `StartParams`/`RerunParams` thêm `language` + `chunk_minutes`; Chunker/`estimate_chunk_count` dùng giá trị chụp; `ChunkTranscriber::transcribe` nhận ngôn ngữ; `TranscriptDraft.language` = mã hoặc `None`; test registry: Chunk 60 s cắt đúng số Chunk, ngôn ngữ chụp được truyền tới transcriber và lưu vào `transcripts.language`, đổi Settings sau khi start không ảnh hưởng Job.
- [x] `src-tauri/src/ipc/mod.rs` -- chụp `chunk_minutes` + `transcribe_language` cùng model ở `transcribe_start_inner`/`transcribe_rerun_inner`; tái sinh `bindings.ts`.
- [x] `src/lib/stores/settings.svelte.ts` (+test) -- `setChunkMinutes`, `setTimestampOffsetSec`, `setTranscribeLanguage` dùng cùng hàng đợi save.
- [x] `src/routes/settings/SettingsChunking.svelte` (+test), `src/routes/Settings.svelte` -- nhóm `chunking` giữa Gemini và Chẩn đoán: input số phút + offset giây, validation tại chỗ (chỉ lưu khi hợp lệ, blur/Enter), tooltip + helper.
- [x] `src/routes/settings/SettingsGemini.svelte` (+test) -- select ngôn ngữ transcribe 4 lựa chọn, helper nêu áp dụng cho file và Live, Job đang chạy giữ cấu hình cũ.
- [x] `src/lib/time.ts` (+test) -- `formatTimestamp(sec, offsetSec)` → `MM:SS` hoặc `HH:MM:SS`; `displayTimestamp(sec)` đọc offset từ `settingsStore` (reactive).
- [x] i18n vi/en/ja cho nhóm, nhãn, tooltip, helper, lỗi validation, tên ngôn ngữ.

**Acceptance Criteria:**
- Given Settings có `chunkMinutes = 1` và file 150 s, when transcribe, then transcriber nhận 3 Chunk với `start_ms` 0/60000/120000.
- Given Job đang chạy với cấu hình cũ, when `settings_save` đổi chunk và ngôn ngữ, then Job đó hoàn tất với cấu hình cũ và Job start sau đó dùng cấu hình mới.
- Given `cargo test`, `cargo fmt --check`, `npm test`, `npm run check`, `npm run check:i18n`, `npm run check:ui`, when chạy, then pass và `bindings.ts` khớp bản sinh.

## Implementation Notes

Triển khai đúng theo Code Map, không lệch thiết kế. Điểm đáng chú ý:
- `TranscribeLanguage::as_code()` là điểm chuyển đổi duy nhất `auto -> None` dùng ở cả `transcripts.language` (transcribe file + Chạy lại).
- `registry.rs` thêm `chunk_options_for(chunk_minutes)` dùng chung cho cả pipeline transcribe file (`decode_and_transcribe`) và Chạy lại (`decode_and_transcribe_ranges`), chỉ đổi `max_duration_seconds`, giữ `ChunkBudget::default()` (spec Never: không đổi ngân sách).
- `estimate_chunk_count` nhận thêm `chunk_minutes` thay vì hằng số `DEFAULT_CHUNK_SECONDS` (đã xoá hằng đó khỏi `registry.rs`; `media::chunk::DEFAULT_CHUNK_SECONDS` giữ nguyên làm mặc định của `ChunkOptions::default()`, không liên quan Settings).
- `build_general_request` thêm tham số `language`; `auto` trả `PROMPT` y hệt (test exact-shape xác nhận byte-for-byte), ngôn ngữ khác nối thêm một câu cố định sau `PROMPT` qua `prompt_text()`.
- `StartParams`/`RerunParams` chụp `language` + `chunk_minutes` cùng lúc với `model`/`consent` ngay trong `transcribe_start_inner`/`transcribe_rerun_inner` (một lần `settings::load` mỗi lệnh) -- Job đã tạo không bao giờ đọc lại Settings.
- Frontend: `settingsStore` thêm 3 setter cùng hàng đợi save lạc quan/rollback đã có; validate tại store (defense in depth) và tại `SettingsChunking.svelte`/`SettingsGemini.svelte` (chặn chính). `src/lib/time.ts` thuần hàm + một wrapper đọc `settingsStore.timestampOffsetSec` reactive, không gọi IPC.
- Nhóm `chunking` chèn giữa `gemini` và `diagnostics` trong `src/routes/Settings.svelte`.

## Spec Change Log

Không có thay đổi so với spec đã duyệt -- triển khai bám sát Code Map/Tasks không cần renegotiate Intent/Boundaries.

## Review Triage Log

## Design Notes

Offset là thuần hiển thị (AD-9): mọi nơi vẽ/export timestamp ở 2.7/2.10 phải đi qua `src/lib/time.ts` thay vì tự cộng. Offset âm bị loại để tránh timestamp hiển thị âm/kẹp 0 gây trùng mốc; nếu cần sau này sẽ nới kiểu mà không đổi dữ liệu. Câu chỉ định ngôn ngữ đặt sau `PROMPT` cố định để `auto` không đổi byte nào, ví dụ: `"The primary spoken language is Japanese; transcribe in the spoken language without translating."`

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` -- expected: pass -- **actual: pass (289 passed, 0 failed, gồm `export_bindings` tái sinh `bindings.ts`)**
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` -- expected: sạch -- **actual: sạch**
- `npm test && npm run check && npm run check:i18n && npm run check:ui` -- expected: pass -- **actual: pass (vitest 221 passed / 34 files; svelte-check 263 files, 0 lỗi/cảnh báo; check:i18n ok; check:ui ok)**
- `git diff --stat src/lib/bindings.ts` sau lần `cargo test` cuối cùng không đổi thêm -- **actual: ổn định, không lệch giữa các lần sinh**
