---
title: 'Story 1.6 — Kho khoá OS và Key pool'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: '39a982336241eb85cc7a2453db049f46491ca186'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/implementation-artifacts/spec-1-5-onboarding-consent.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** API key chưa có nơi lưu an toàn và các luồng Gemini chưa có chính sách cấp/xoay key thống nhất.

**Approach:** Thêm adapter Keychain/Credential Manager, port `KeyProvider`, và actor `KeyPool` sở hữu cấp key, cooldown, ưu tiên và deadline; kiểm thử bằng store/clock/consumer giả, chưa gọi Gemini thật.

## Boundaries & Constraints

**Always:** Pin exact `keyring 4.2.0`/`tokio 1.53.1`; key chỉ ở kho OS hoặc `Sensitive<String>` trong RAM, không vào Settings/SQLite/file/log/error/event/response. Parse danh sách comma: trim, bỏ rỗng/trùng, chỉ prefix `AIza`/`AQ.`; trả ID opaque + nhãn che. Store lỗi → `Code::Storage`, app vẫn boot và settings nguyên vẹn. `KeyPool` là một tokio task, chỉ `mpsc` + `oneshot`, không `Arc<Mutex>`; ưu tiên `Live > Job > Memo`, cancel/clock tiêm được. 429 cooldown 60 s rồi thử key kế; 401/403 loại tới khi sửa rồi thử key kế; 400/404/timeout không xoay hay fan-out. Job có tổng 180 s/Chunk, tối đa 4 lần gửi; Memo 90 s. Live 429 chặn cấp mới cho Job tới hết cooldown. Sửa/xóa cho lease hiện tại hoàn tất, nhưng generation cũ không thể hồi sinh/cấp lại key.

**Never:** Không lưu secret ngoài OS store; không thêm migration, Tauri plugin, entitlement, sidecar hay Linux. Không làm UI/i18n, REST/WS, model listing hoặc `keys_test` production; Story 1.7 dùng lease API để test từng key, Story 1.8/1.9 sở hữu UI.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|---------------------------|----------------|
| Lưu/xóa | Key hợp lệ, trùng/space; xóa theo ID | Normalize, round-trip/xóa thật; chỉ trả metadata che | Sai format không lộ key; store hỏng → `storage` |
| Quota | Key A 429, B sẵn | A cooldown 60 s, cùng request lấy B; Live 429 chặn cấp mới cho Job | Hết key sẵn thì chờ trong budget/cancel được |
| Auth | A 401/403, B sẵn hoặc không | Loại A; thử B; chỉ khôi phục A khi chính A được sửa/kiểm lại | Không còn key → `auth` ngay |
| Không xoay | 400/404 hoặc timeout | Trả lỗi trên đúng lease, không cấp key khác | Timeout không gửi lại payload qua key khác |
| Cập nhật đồng thời | Xóa/sửa key đang có lease | Lease kết thúc; generation cũ bị bỏ | Không hồi sinh key từ report trễ |

</frozen-after-approval>

## Code Map

- `src-tauri/src/secrets/mod.rs` -- parser + native/fakeable store; tái dùng `Sensitive` và `Code::Storage`.
- `src-tauri/src/gemini/{mod,keys,params}.rs` -- giữ nguyên Consent gate; thêm `KeyProvider`, actor/handle/lease/report, priority/deadline/cooldown constants và clock port.
- `src-tauri/src/ipc/{boot,mod}.rs` -- boot pool không fail app; command typed quản lý metadata, I/O blocking ngoài main thread.
- `src-tauri/src/settings/mod.rs`, `src-tauri/src/db/**` -- không thêm field/table key; dùng để test quét persistence.
- `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src/lib/bindings.ts` -- pin dependency và sinh binding, không sửa binding tay.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/Cargo.toml`, `Cargo.lock` -- thêm exact keyring/tokio với feature tối thiểu.
- [x] `src-tauri/src/secrets/mod.rs` -- implement port + native/fake store, normalize/validate, opaque metadata, set/list/delete nguyên tử ở mức service và redaction.
- [x] `src-tauri/src/gemini/{keys,params}.rs` -- implement actor state machine, lease generation, priority, rotation, budgets, cancellation và fake clock.
- [x] `src-tauri/src/ipc/{boot,mod}.rs`, `src/lib/bindings.ts` -- wire lifecycle/commands, boot chịu store lỗi, regenerate bindings.
- [x] Rust tests -- phủ toàn bộ matrix, per-key isolation, persistence/log scan và late-report race; giữ Consent zero-transport test.

**Acceptance Criteria:**
- Given một hoặc nhiều key hợp lệ, when lưu/list/xóa, then chỉ native OS store giữ secret, response chỉ có ID/nhãn che, và quét settings/DB/log không thấy key.
- Given fake provider/clock và kết quả quota/auth/request/timeout, when actor cấp/nhận report, then policy đúng matrix và không có `Arc<Mutex>` state.
- Given native store không dùng được, when app boot hoặc command key chạy, then app vẫn mở, settings không đổi và command trả category `storage` đã redaction.

## Implementation Notes

- Native service stores one versioned `{id, secret}` payload under `com.transkun.app/gemini-api-keys`; only masked `KeyMetadata` crosses IPC through `keys_list`, `keys_set`, and `keys_delete`.
- `KeyPoolActor` owns state and uses `mpsc`/`oneshot`; refresh failures clear the in-memory pool, and every store mutation refreshes generations before returning.
- Added deterministic fake provider/clock coverage for priority, rotation, 60 s cooldown, 180/90 s total deadlines, four-attempt ceiling, cancellation, Live quota barrier and stale-lease races. Packaged Keychain/MSIX smoke checks remain intentionally deferred to Stories 1.11/1.12.

## Spec Change Log

## Review Triage Log

## Design Notes

Native adapter dùng `Entry::new`/`set_password`/`get_password`/`delete_credential`; fake store không chạm OS UI. Một credential payload versioned giữ `{id, secret}` để không cần manifest plaintext. Lease chứa ID + `Sensitive<String>` + generation; report trễ chỉ tác động khi generation còn hiện hành.

## Verification

**Commands:**
- `npm run bindings && npm run check:deps && npm run check:i18n && npm run check:ui && npm run check && npm run build && npm run check:build-assets && npm test` -- frontend/dependency gates pass.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- adapter, actor, races, security scans và baseline Rust pass.
- `git diff --exit-code src/lib/bindings.ts` -- generated binding không drift.

**Manual checks (if no CLI):**
- Trên macOS/Windows dev build, lưu/list/xóa và xác nhận native store; packaged entitlement smoke test thuộc Story 1.11/1.12.
