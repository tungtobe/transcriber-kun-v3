---
title: 'Story 1.7 — Cổng Gemini duy nhất và liệt kê model'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: 'e2559701b3e01ca6b5862588c92c2d32bb244000'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-6-kho-khoa-os-va-key-pool.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** App chưa có REST gateway thật để kiểm tra key và lấy danh sách model; nếu mỗi feature tự gọi Google thì Consent, xoay key, timeout và phân loại lỗi sẽ không nhất quán.

**Approach:** Xây một cổng Gemini async trên `reqwest` + KeyPool, có transport giả để khoá request, pagination, capability filter, cancellation và lỗi; expose IPC typed cho list model/test key.

## Boundaries & Constraints

**Always:** Pin exact `reqwest 0.13.5`, feature `rustls` với platform verifier/CA store OS; check Consent bền vững trước acquire key hoặc mở transport. `GET /v1beta/models` chỉ dùng header `x-goog-api-key`; đọc hết trang trong một deadline/cancel budget tập trung. Chỉ lọc từ metadata `supportedGenerationMethods` hoặc capability table đã kiểm chứng; không suy version/capability từ tên alias. Mọi lỗi dùng `AppError` hiện có; TLS/CA → `Tls`, timeout → `Timeout`, lỗi mạng → `Network`, HTTP/API → `Quota|Auth|Model|Request|Blocked|Shape`; không lộ key, URL hay response body. Key test cô lập theo đúng key ID và chỉ cập nhật trạng thái của key đó. Không có timer/poll/request nền.

**Never:** Không dùng Gemini SDK, `tauri-plugin-http`, query-string key, bundled WebPKI roots, migration/settings, UI/i18n, WebSocket hay tự đổi model. Không sửa binding sinh tay, log dữ liệu nhạy cảm hoặc reset budget theo page/retry.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Consent gate | Pending, declined hoặc stale | Trả trước khi acquire/transport; fake transport ghi 0 call | `Blocked`, không mạng |
| Liệt kê | `kind` transcribe/live/memo, nhiều page | Một thao tác GET tuần tự, dedupe ổn định, live chỉ có capability Live | Repeated token/shape sai → `Shape`; giữ kết quả cấu hình ngoài scope nguyên vẹn |
| Key rotation | Pool gặp 429 hoặc 401/403 | Report lease rồi retry theo actor/budget hiện tại | 429 → `Quota`; 401/403 → `Auth`; timeout/request không xoay |
| Test key | Một opaque key ID giữa nhiều key | Gọi đúng secret của ID đó; kết quả không thể được cứu bởi key khác | Chỉ key đang test bị disable/restore theo chính kết quả của nó |
| Transport | Proxy/CA, DNS/connect/read timeout | Dùng CA hệ thống và trả lỗi đã redact | CA/TLS → `Tls`; timeout → `Timeout`; còn lại → `Network` |

</frozen-after-approval>

## Code Map

- `src-tauri/src/gemini/mod.rs` -- thay seam sync bằng gateway/transport async nhưng giữ zero-transport Consent invariant và unit tests hiện có.
- `src-tauri/src/gemini/keys.rs` -- tái dùng `KeyPoolHandle`, lease/report/cancel; bổ sung acquire/test theo `KeyId` để bảo đảm per-key isolation, không đổi actor ownership.
- `src-tauri/src/gemini/params.rs` -- endpoint, default models Q10 và timeout list/key-test tập trung.
- `src-tauri/src/core/error.rs`, `core/log.rs`, `core/sensitive.rs` -- dùng nguyên taxonomy/redaction; không thêm category hay đường lỗi thô.
- `src-tauri/src/ipc/{boot,mod}.rs` -- boot một HTTP client/gateway, đọc consent từ DB và expose `models_list`/`keys_test` qua builder.
- `src-tauri/Cargo.toml`, `Cargo.lock`, `src/lib/bindings.ts` -- direct-pin reqwest rustls, cập nhật lock và regenerate binding.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/Cargo.toml`, `Cargo.lock` -- thêm exact reqwest với `default-features = false`, `json,rustls`; không thêm HTTP plugin/SDK.
- [x] `src-tauri/src/gemini/{mod,params}.rs` -- implement request DTO/response DTO, client transport, gateway, pagination, filtering, cancel/deadline và error mapping.
- [x] `src-tauri/src/gemini/keys.rs` -- thêm đường cấp/test đúng một key, giữ rotation/deadline/generation semantics và late-report safety.
- [x] `src-tauri/src/ipc/{boot,mod}.rs`, `src/lib/bindings.ts` -- wire lifecycle/typed commands và regenerate binding.
- [x] Rust tests -- fake transport khoá method/path/header với key `AIza…` và `AQ.…`; phủ consent zero-call, pagination, repeated token, filters, cancel/timeout, rotation, per-key isolation và redaction.

**Acceptance Criteria:**
- Given bất kỳ Gemini feature nào, when cần REST, then chỉ gateway tạo URL/header/body và feature không có client riêng.
- Given Consent không current, when gọi list/test, then không acquire key và transport nhận 0 request.
- Given response nhiều trang và model metadata hỗ trợ khác nhau, when `models_list(kind)` hoàn tất, then mọi trang được đọc trong một budget và kết quả đúng capability, không suy alias.
- Given quota/auth/model/request/timeout/network/TLS/blocked/shape, when gateway trả lỗi, then code/category đúng taxonomy và serialized/log output không chứa key hay URL.

## Implementation Notes

- Added one process-wide `ReqwestTransport` using reqwest's rustls platform verifier and a normalized transport port whose debug output redacts headers/bodies.
- `GeminiGateway` now owns consent gating, whole-operation deadlines, cancellation, pagination, capability-only filtering, error mapping and KeyPool report/rotation semantics.
- Added target-key actor commands so validation cannot fall back to another key; only a successful check restores an auth-quarantined key.
- IPC reads consent from the Rust-owned database on every call, exposes generated `modelsList`/`keysTest` bindings, and keeps boot non-fatal if the TLS client cannot initialize.
- Audit fixes added race-free cancellation, lease cleanup on shape errors, Q10 default constants, and matrix tests for TLS/network/timeout, live filtering, in-flight cancellation and auth isolation.

## Spec Change Log

## Review Triage Log

## Design Notes

Gateway sở hữu request contract; transport port chỉ nhận request đã chuẩn hoá và trả response/status tối thiểu để test không cần mạng. IPC đọc Consent server-side. List theo kind dùng ưu tiên nền phù hợp với luồng nhưng timeout riêng ngắn; key-test dùng actor target-key path để không bao giờ fallback sang key khác.

## Verification

**Commands:**
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- gateway, actor, IPC, redaction và baseline Rust pass.
- `npm run bindings && git diff --exit-code src/lib/bindings.ts` -- generated binding không drift sau lần sinh.
- `npm run check:deps && npm run check:i18n && npm run check:ui && npm run check && npm run build && npm run check:build-assets && npm test` -- toàn bộ quality gates pass.

**Manual checks (if no CLI):**
- Trên macOS/Windows sau corporate proxy/CA (ví dụ Zscaler), thao tác list model dùng system trust store; không có request lúc app rảnh.
