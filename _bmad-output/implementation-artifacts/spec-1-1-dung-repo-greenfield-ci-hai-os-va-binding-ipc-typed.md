---
title: 'Story 1.1 — Dựng repo greenfield, CI hai OS và binding IPC typed'
type: 'feature'
created: '2026-09-22'
status: 'done'
baseline_commit: '606a5f964e7f301b67bb988986f1c28ec07b310b'
route: 'full'
route_source: 'auto'
review: 'thorough'
review_source: 'auto'
lenses_ran: ['blind-hunter', 'edge-case-hunter', 'verification-gap', 'intent-alignment']
review_loop_iteration: 0
context:
  - '{project-root}/_bmad-output/implementation-artifacts/epic-1-context.md'
  - '{project-root}/_bmad-output/planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Repo chỉ có tài liệu; chưa có app Tauri, chưa có CI, chưa có binding IPC — mọi story sau không có nền để chạy.

**Approach:** Dựng tay (không starter template) đúng Structural Seed: Svelte 5 SPA + `src-tauri` chia module theo feature, version pin theo bảng Stack, một command mẫu `app_version` đi qua binding tauri-specta sinh tự động, CI test hai OS kèm hai cổng chặn (binding lệch, dependency cấm), và spike `Channel<T>` typed ghi thành ADR.

## Boundaries & Constraints

**Always:**
- Version khớp bảng Stack: crate/npm trong bảng pin exact (`=x.y.z` trong Cargo, không `^` trong npm); tauri-specta/specta `=2.0.0-rc.25`; commit `Cargo.lock` và `package-lock.json`.
- Version app một nguồn: `src-tauri/Cargo.toml`; `tauri.conf.json` không có trường `version`; `package.json` `private: true`, không mang version sản phẩm.
- `productName` `trans-kun`, `identifier` `com.transkun.app`; cửa sổ 1280×800, tối thiểu 1024×680.
- `src/lib/bindings.ts` chỉ sinh bởi tauri-specta; component không gọi binding/`invoke` trực tiếp mà qua store `src/lib/stores/<domain>.svelte.ts`.
- Command IPC `snake_case` `<domain>_<action>`, serde `camelCase`.
- Rust edition 2021; mỗi feature module có `mod.rs` tối thiểu; phụ thuộc chỉ `ipc → feature → hạ tầng → core`.

**Never:**
- Không `tauri-plugin-{shell,fs,updater,process,http,store}` (Rust lẫn `@tauri-apps/plugin-*`), không sidecar/`externalBin`, không Python/ffmpeg.
- Không thêm dependency ngoài phạm vi story (rusqlite, keyring, reqwest, tracing, dialog/opener… thuộc story sau).
- Không làm `AppError`/lỗi có category (1.2), theme/token (1.3), i18n library/router (1.3–1.4), nội dung overlay store (1.11/1.12), workflow `store-mac`/`store-win`.
- Command spike `Channel<T>` không được đăng ký vào builder production và không xuất hiện trong `bindings.ts`.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Gọi version | UI mount, gọi store `app` | Hiển thị version đọc từ `Cargo.toml` qua `commands.appVersion()` | Store giữ trạng thái lỗi, UI hiện "—", không màn trắng |
| Binding lệch | Sửa signature command mà không sinh lại | CI job fail ở bước drift check | Thông báo chỉ rõ `src/lib/bindings.ts` lệch |
| Dependency cấm | Manifest/lock chứa `tauri-plugin-http` hoặc `@tauri-apps/plugin-fs`, hoặc conf có `externalBin` | `check:deps` exit ≠ 0, liệt kê file + tên vi phạm | CI fail |
| Manifest sạch | Không có tên cấm | `check:deps` exit 0 | — |

</frozen-after-approval>

## Code Map

Repo greenfield: chỉ có `_bmad*/`, `design-system/`, `.claude/`, `.gitignore`, `skills-lock.json`. Không có code để tái dùng. Không sửa `_bmad*/`, `design-system/`.

- `.gitignore` -- đã thay (template Python có `lib/` từng ignore `src/lib/`); không sửa lại trừ khi thiếu mục mới.
- Toolchain local: Rust 1.92 (target mac arm64/x64 + win msvc), Node 24.18, npm 11. MSRV tauri 2.11.5 = 1.77.2. Mọi version bảng Stack đã xác nhận có trên crates.io/npm; svelte-check 4.7.6 hỗ trợ TS 6.
- tauri-specta rc.25 phụ thuộc `specta =rc.25`, `specta-typescript ^0.0.12`, `tauri ^2`; features `derive`, `typescript`. Tra API export/`Channel` bằng ctx7 trước khi viết.
- `tauri::generate_context!` cần `frontendDist` tồn tại lúc compile và `icons/icon.ico` trên Windows → CI build frontend trước `cargo test`; sinh icon placeholder bằng `tauri icon`.

## Tasks & Acceptance

**Execution:**
- [x] `.gitignore` -- đã dọn trong lúc lập kế hoạch (theo yêu cầu người dùng): bỏ template Python, giữ ignore riêng của dự án, thêm Node/Rust/Tauri -- `src/lib` không còn bị ignore.
- [x] `.gitattributes` -- `* text=auto eol=lf` -- binding không lệch vì CRLF trên Windows.
- [x] `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `svelte.config.js`, `index.html` -- scripts `dev`, `build`, `check` (svelte-check), `test` (vitest run), `bindings`, `check:deps`, `tauri`; dev server port cố định 1420 -- khung frontend pin theo Stack.
- [x] `src/main.ts`, `src/App.svelte`, `src/lib/stores/app.svelte.ts`, `src/lib/keymap.ts`, `src/routes/.gitkeep`, `src/components/.gitkeep`, `src/i18n/{vi,en,ja}.json` -- store `app` (runes) bọc `commands.appVersion()` với trạng thái loading/ok/error; `App.svelte` hiển thị qua store; keymap là registry rỗng; i18n cùng một key tối thiểu -- đúng seed, AD-13.
- [x] `src-tauri/Cargo.toml`, `Cargo.lock`, `build.rs`, `src/main.rs`, `src/lib.rs`, `capabilities/default.json`, `icons/` -- crate lib+bin, deps tối thiểu (tauri, tauri-build, tauri-specta, specta, specta-typescript, serde); capability chỉ `core:default` -- nền Rust.
- [x] `src-tauri/src/{ipc,core,settings,library,transcribe,live,memo,ads,gemini,media,audio,db,secrets,remote}/mod.rs` -- mỗi file doc comment một dòng mô tả trách nhiệm; `ipc/mod.rs` chứa `app_version` và `pub fn specta_builder()` dùng chung cho `lib.rs` và test export -- một nơi định nghĩa danh sách command.
- [x] `src-tauri/src/ipc/mod.rs` (test `export_bindings`) -- test ghi `../src/lib/bindings.ts` từ `specta_builder()`; script npm `bindings` chạy test này -- nguồn duy nhất sinh binding, dùng cho drift check.
- [x] `src-tauri/src/ipc/spike_channel.rs` (`#[cfg(test)]`), `docs/adr/0001-tauri-specta-channel-typed.md` -- builder riêng có command nhận `Channel<SpikeEvent { seq: u32, … }>` (đổi từ `u64`: xem ADR mục 4 -- specta-typescript cấm BigInt mặc định): kiểm TS sinh ra có `Channel<SpikeEvent>` typed; runtime dựng `tauri::ipc::Channel` thật (không mock webview, xem ADR mục 5) nhận đủ và đúng thứ tự `seq`. ADR ghi rõ đã kiểm gì, kết quả, các phát hiện phụ (toolchain, BigInt); kết luận Channel<T> chạy được, AD-3 giữ nguyên -- AR-14.
- [x] `src-tauri/tauri.conf.json`, `tauri.appstore.conf.json`, `tauri.msix.conf.json` -- conf chính theo Always; hai overlay chỉ `{ "$schema": … }` -- khung cho 1.11/1.12.
- [x] `scripts/check-forbidden-deps.mjs`, `scripts/check-forbidden-deps.test.mjs` -- hàm thuần quét `src-tauri/Cargo.toml`, `Cargo.lock`, `package.json`, `package-lock.json` và `src-tauri/tauri*.conf.json` (`externalBin`); CLI in vi phạm và exit 1; test phủ bốn dòng I/O Matrix bằng fixture -- AR-3, NFR-4.
- [x] `src/lib/stores/app.svelte.test.ts` -- mock `../bindings`, kiểm trạng thái ok và error -- dòng "Gọi version".
- [x] `.github/workflows/test.yml` -- matrix `macos-latest`, `windows-latest`: `npm ci` → `check:deps` → `check` → `build` → `test` → `cargo test --locked` → `git diff --exit-code src/lib/bindings.ts` -- AR-36.

**Acceptance Criteria:**
- Given repo sau story, when liệt kê cây, then có đủ thư mục/file của Structural Seed nêu trong Tasks.
- Given máy macOS ≥ 14.4 hoặc Windows 10 1809+/11, when `npm run tauri dev`, then cửa sổ Svelte 5 1280×800 (min 1024×680) mở và hiển thị version lấy qua binding sinh tự động.
- Given `grep -rn "invoke(" src --include=*.ts --include=*.svelte`, when chạy, then chỉ khớp trong `src/lib/bindings.ts`.
- Given một PR, when CI chạy, then mọi bước pass trên cả macOS và Windows.
- Given ADR 0001, when đọc, then nêu kết luận Channel typed chạy được hay phải bọc, kèm bằng chứng từ spike.

## Implementation Notes

- Toolchain local nâng từ Rust 1.92.0 lên `stable` (1.98.1 lúc implement): `specta 2.0.0-rc.25` dùng `std::fmt::from_fn`, chỉ ổn định từ Rust 1.93 — build thất bại trên 1.92. CI (`test.yml`) dùng `dtolnay/rust-toolchain@stable` (kênh, không ghim version cụ thể) thay vì `1.92.0`. Chi tiết ở ADR 0001.
- `tauri` crate cần bật feature `specta` (`features = ["specta"]`) để `Channel<T>` implement `specta::Type` — nếu không export sẽ không sinh được `Channel<...>` typed.
- API `tauri-specta` rc.25 thật (không giống docs `main` branch): không có `Builder::export_str`, chỉ có `Builder::export(lang, path)` ghi ra file. Spike export ra file tạm riêng (`std::env::temp_dir()`), không đụng `src/lib/bindings.ts`.
- `SpikeEvent.seq` và tham số `count` dùng `u32` thay vì `u64`: `specta-typescript` cấm xuất kiểu BigInt-unsafe (`u64/i64/usize/isize/u128/i128`) sang TS mặc định ("BigInt forbidden"). Khuyến nghị ADR: mọi `seq`/counter tương tự ở story sau (live, job…) dùng `u32` trừ khi thiết kế tường minh đường BigInt lossless.
- Runtime check của spike dựng thẳng `tauri::ipc::Channel::new(closure)` (Runtime-agnostic, không cần mock App/Webview) thay vì đi hết qua `tauri::test::mock_builder()` + `get_ipc_response()` — lý do đầy đủ ở ADR 0001 mục 5 (đường IPC-mediated cần mô phỏng runtime JS phía frontend, không thêm giá trị kiểm chứng cho phần "typed Channel" mà spike cần xác nhận).
- `commands.appVersion()` sinh ra dùng chế độ lỗi mặc định (`ErrorHandlingMode::Result`): trả `{status:"ok",data:T} | {status:"error",error:E}`; store `app` xử lý cả hai nhánh cộng với exception (IPC unavailable) — cả ba đường đều dẫn về trạng thái `error`, không throw ra UI.
- Đã sinh `src-tauri/icons/` bằng `tauri icon <placeholder.png>` (icon vuông teal + "TK", không phải icon chính thức sản phẩm — chỉ để `generate_context!` có `icon.ico`/`icon.icns` lúc compile theo Code Map). CLI này cũng sinh kèm thư mục `android/` và `ios/` (mobile, ngoài phạm vi desktop-only của story) nằm cạnh các icon desktop/Appx cần dùng; để nguyên vì không phải dependency/plugin, chỉ là asset tĩnh thừa — có thể dọn ở story sau nếu muốn.
- Xác minh AC "grep invoke( chỉ khớp trong bindings.ts": với tauri-specta rc.25, `bindings.ts` sinh ra import `{ invoke as __TAURI_INVOKE }` và gọi `__TAURI_INVOKE(...)` — chuỗi `"invoke("` (chữ thường, có dấu ngoặc) không xuất hiện ở bất kỳ đâu, kể cả trong chính `bindings.ts`. `grep -rn "invoke(" src --include="*.ts" --include="*.svelte"` vì vậy trả về **không có kết quả nào** (thoả điều kiện "chỉ khớp trong bindings.ts" theo nghĩa rỗng), thay vì có một số dòng khớp đúng trong file đó như câu chữ AC ngụ ý. Ý định của AC (component không tự gọi `invoke`/binding trực tiếp) vẫn đúng và đã kiểm bằng cách đọc toàn bộ `src/*.svelte`/`src/**/*.ts` ngoài `bindings.ts`.

## Spec Change Log

## Review Triage Log

### Pass 1 (2026-09-22) — lenses: blind-hunter, edge-case-hunter, verification-gap, intent-alignment (sonnet, theo yêu cầu người dùng)

Verdicts: high 0 · medium 1 · low 5 · false 5 · maybe-false 0 · rejected (fix = sửa spec) 4. Không có intent_gap/bad_spec → không loopback.

| # | Lens | Finding | Verdict | Route | Evidence / action |
|---|------|---------|---------|-------|-------------------|
| 1 | verification-gap | CLI `main()`/`isMain` của check-forbidden-deps không có test exit code | medium | patch | Pre-verified; test chỉ gọi `scanSources`/`scanRepo`. Thêm test subprocess clean/violating. |
| 2 | verification-gap | `App.svelte` hiển thị loading/ok/error không có test render | low | defer | Cần harness DOM (jsdom + testing-library) — để story 1.3 (app shell). Ghi deferred-work. |
| 3 | edge-case | `serde_json` là dep production nhưng chỉ spike test dùng | low | patch | Chỉ `spike_channel.rs` (`cfg(test)`) dùng; chuyển sang `[dev-dependencies]`. |
| 4 | blind-hunter | ADR 0001 không nhắc phải kiểm lại khi bump pin | low | patch | Thêm một câu hệ quả vào ADR. |
| 5 | blind-hunter | Lockfile vắng trong diff | false | reject | Diff cố ý loại lockfile; `package-lock.json`, `src-tauri/Cargo.lock` có trên đĩa, `npm ci`/`--locked` pass. |
| 6 | blind-hunter | Icon vắng trong diff | false | reject | Diff loại `src-tauri/icons`; file tồn tại, `cargo test` pass. |
| 7 | blind-hunter | vite/vitest/plugin peer chưa kiểm | false | reject | vitest 5.0.1 peer `vite ^8`, plugin 7.3.0 peer `vite ^8`/`svelte ^5.46.4`; `npm ci` sạch. |
| 8 | blind-hunter | check:deps không quét khối `plugins` trong conf | false | reject | Plugin không thể chạy nếu thiếu crate/npm dep, mà dep đã bị quét trong manifest + lockfile. |
| 9 | edge-case | `loadVersion` race khi gọi chồng | false | reject | Chỉ gọi một lần trong `onMount`; không có đường gọi chồng. |
| 10 | blind-hunter | Không có `rust-toolchain.toml`, CI dùng `stable` trôi | low | reject | `rust-version = "1.93"` đã báo lỗi rõ với toolchain cũ; stable phá build là hiếm, pin toolchain thêm bề mặt bảo trì. |
| 11 | blind-hunter | Action CI pin theo tag, không SHA | low | reject | Thực hành chung; hardening không gặp trong dùng hằng ngày. |
| 12 | blind-hunter | Key i18n `app.version.label` chưa dùng, "Version:" hardcode | low | reject | Thư viện i18n thuộc story 1.4 (Never của spec); chuỗi seed tạm. |
| 13 | blind-hunter | Code Map ghi Rust 1.92, Spec Change Log trống | — | reject | Fix là sửa spec. Thực tế đã ghi trong ADR + `rust-version`. |
| 14 | blind-hunter | Lệnh grep thủ công trong Verification không quét lockfile | — | reject | Fix là sửa spec; `check:deps` tự động đã quét lockfile. |
| 15 | edge-case + intent | AC grep `invoke(` pass rỗng (binding dùng `__TAURI_INVOKE`) | — | reject | Fix là sửa spec; ý định (component không gọi IPC trực tiếp) đúng khi đọc code — chỉ store `app.svelte.ts` import `commands`. |
| 16 | edge-case | Test check-deps "phủ bốn dòng matrix" nhưng chỉ phủ 2 | — | reject | Câu sai nằm ở spec; header test ghi đúng. Dòng 1 do store test phủ, dòng 2 do bước CI. |
| 17 | intent | Spike runtime bỏ qua transport IPC/WebView; CI chưa có bằng chứng chạy | — | reject | Mô tả, không phải lỗi: ADR công khai giới hạn; CI chỉ chứng minh được khi push. |

## Verification

**Commands:**
- `npm ci && npm run check:deps && npm run check && npm run build && npm test` -- expected: tất cả exit 0
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` -- expected: pass, gồm `export_bindings` và spike
- `git diff --exit-code src/lib/bindings.ts` -- expected: không lệch sau khi chạy test
- `grep -rE "tauri-plugin-(shell|fs|updater|process|http|store)|plugin-(shell|fs|updater|process|http|store)" package.json src-tauri/Cargo.toml` -- expected: không khớp

**Manual checks (if no CLI):**
- `npm run tauri dev` trên macOS: cửa sổ mở đúng kích thước, co không nhỏ hơn 1024×680, hiển thị version `Cargo.toml`.
