# Review: Tính hiện hành công nghệ (Tech Currency) — trans-kun v3

**Lens:** Xác minh mọi quyết định có xác thực web/reality-check thay vì suy diễn từ training data — đặc biệt các tuyên bố FIT (thư viện có làm được điều được gán cho nó không), không chỉ số phiên bản.
**Đối tượng:** `ARCHITECTURE-SPINE.md` + `.memlog.md` (thư mục `architecture-transcriber_kun-2026-09-18/`)
**Ngày review:** 2026-09-18
**Phương pháp:** WebSearch/WebFetch + crates.io API (curl, có User-Agent) cho từng claim FIT được chỉ định. Không re-check số phiên bản thuần túy (đã có mục `(version)` trong memlog, query cùng ngày).

---

## Tóm tắt

10 claim FIT được kiểm; 1 **sai rõ ràng** (reqwest), 1 **rủi ro hiện hành nghiêm trọng nhưng đã bị đánh giá thấp** (TypeScript 7 + svelte-check), 1 rủi ro RC đã biết nhưng thiếu bằng chứng cụ thể trong tài liệu, còn lại xác nhận đúng (PASS) với evidence kèm theo.

---

## Findings

### F1 — [HIGH] `reqwest 0.13.5` ghi feature `rustls-tls-native-roots` — feature này không còn tồn tại trong reqwest 0.13

**Claim:** Bảng Stack: `reqwest (rustls-tls-native-roots) · tokio-tungstenite | 0.13.5 · 0.30.0`. Cách ghi ngụ ý feature Cargo tên `rustls-tls-native-roots` áp dụng cho reqwest.

**Evidence:**
- docs.rs liệt kê feature của reqwest 0.13.5: chỉ có `rustls`, `rustls-no-provider`, `native-tls`, `default-tls` — không có `rustls-tls-native-roots` / `rustls-tls-webpki-roots`. (https://docs.rs/reqwest/0.13.5/reqwest/)
- Bài viết chính thức của tác giả reqwest xác nhận: từ 0.13, khi bật `rustls`, reqwest **mặc định dùng platform verifier** (native/OS root certs) — không cần feature `-native-roots` riêng nữa; muốn tùy biến dùng `tls_certs_merge()` / `tls_certs_only()`. (https://seanmonstar.com/blog/reqwest-v013-rustls-default/)
- Đối chiếu: `tokio-tungstenite 0.30` (dòng kế bên trong cùng ô bảng) **vẫn có** feature `rustls-tls-native-roots` hợp lệ — nên rất có thể tên feature của tokio-tungstenite đã bị gán nhầm sang reqwest khi soạn bảng.

**Mức độ ảnh hưởng:** Nếu triển khai copy thẳng `features = ["rustls-tls-native-roots"]` vào `Cargo.toml` cho reqwest, build sẽ fail ngay (unknown feature). Đây là build-substrate, sai sót này lan sang code thật nếu không sửa trước khi viết Cargo.toml.

**Suggested fix:**
- Sửa bảng Stack: `reqwest | 0.13.5 (feature "rustls"; native-roots là mặc định qua rustls-platform-verifier)`.
- Giữ `tokio-tungstenite (rustls-tls-native-roots) 0.30.0` — claim này đúng, không cần đổi.
- Thêm 1 dòng vào Consistency Conventions hoặc `gemini/` note: TLS backend là rustls cho cả `reqwest` và `tokio-tungstenite`, root cert nguồn = OS (không dùng webpki bundled), để tránh future-dev đoán nhầm 2 crate dùng chung tên feature.

---

### F2 — [MEDIUM-HIGH] TypeScript 7.0.2 pinned nhưng `svelte-check` hiện KHÔNG chạy được với TS7 native compiler (tình trạng tại 2026-09-18, không chỉ là "chưa xác nhận")

**Claim:** Bảng Stack: `TypeScript | 7.0.2`. Cuối spine, mục Deferred đã liệt "Tương thích ... TypeScript 7 với svelte-check ... xác nhận ở Phase 0" — nhưng được xếp ngang hàng với các câu hỏi mở thông thường, không nêu rõ đây là một **incompatibility đã biết, đang chờ fix ngược dòng**.

**Evidence:**
- GitHub issue trực tiếp: "TypeScript 7 blocked: svelte-check incompatible with the TS 7 native compiler" (Artist-Alley-Org/artist-alley#309).
- Bài phân tích kỹ thuật: `svelte-check` (qua svelte language-tools) gọi `require('typescript').default.sys`, mà compiler native/tsgo của TS7 không expose theo shape CJS đó → svelte-check crash khi khởi động. (https://www.digitalapplied.com/blog/typescript-7-native-compiler-early-adopter-migration-readiness)
- Báo cáo tổng hợp (TechTimes, 07/2026): "TypeScript 7 Now Stable: 10× Faster Builds, But Not for Vue or Svelte Yet" — Microsoft dự kiến TS 7.1 (khoảng 10/2026, **sau** ngày viết spine hôm nay 2026-09-18) mới đóng khoảng trống API để Volar/svelte-check adopt native compiler. (https://www.techtimes.com/articles/320049/20260710/typescript-7-now-stable-10-faster-builds-not-for-vue-svelte-yet.htm)
- PR đang mở để vá `svelte-check-rs`/`tsgo` (pheuter/svelte-check-rs#179, #165) — xác nhận cộng đồng đang xử lý nhưng chưa merge/release ổn định tại thời điểm review.

**Mức độ ảnh hưởng:** `svelte-check` được dùng ngầm định trong mọi pipeline Svelte 5 + TS chuẩn (type-check CI, editor tooling), và Consistency Conventions của spine đòi "CI chặn thiếu" cho i18n set — quy trình CI rất có thể phụ thuộc `svelte-check`. Nếu Phase 0 rơi vào lúc TS 7.1 (fix) chưa ra, toàn bộ frontend toolchain bị chặn ngay từ đầu, không phải rủi ro nhỏ.

**Suggested fix:**
- Nâng câu hỏi mở này từ "xác nhận ở Phase 0" thành một **gate rõ ràng có phương án dự phòng**: nếu tại thời điểm Phase 0 mà `svelte-check` vẫn chưa hỗ trợ TS7 (kiểm tra release note `svelte-check`/`typescript-tools` trước), pin tạm **TypeScript 6.x** (dòng classic tsc, ổn định với toolchain Svelte hiện tại) thay vì 7.0.2, và ghi rõ điều kiện để nâng lên 7.x sau.
- Thêm dòng "Deferred" tách riêng khỏi nhóm câu hỏi mở thường, đánh dấu mức độ chặn (blocking) cụ thể hơn.

---

### F3 — [MEDIUM] `tauri-specta 2.0.0-rc.25` + `specta 2.0.0-rc.25` ghép với `tauri 2.11.5`: có tiền sử breakage giữa RC pin và Tauri point release — cần spike xác nhận thay vì chỉ "pin exact" là đủ

**Claim:** Bảng Stack pin `tauri-specta / specta = 2.0.0-rc.25 (exact)` cạnh `Tauri 2.11.5`. Convention "Dependency: RC (tauri-specta, specta) pin exact" coi việc pin chính xác là đủ để đảm bảo an toàn.

**Evidence:**
- Có tiền sử thực tế: `tauri-specta` rc.21 từng **không tương thích** với Tauri 2.9.x do một PR trong chính repo Tauri tạo ra "implicit version lock" trên Specta =rc.20, chặn cả các bản vá sau này — chỉ được xử lý bằng `tauri-apps/tauri#12371`. (https://github.com/tauri-apps/tauri/pull/12371, thảo luận liên quan tại github.com/specta-rs/specta/issues/305)
- `tauri::ipc::Channel<T>` **có** được specta hỗ trợ (remote type qua feature `specta` trong crate `tauri`, dùng trong command argument/result) — claim "hỗ trợ Channel<T> typed" về cơ bản đúng, nhưng chưa "cứng": issue #198 (specta-rs/tauri-specta, mở 2026-01-23) cho thấy việc dùng `Channel` sai vị trí (ngoài argument/result, ví dụ lồng trong struct) hiện **chưa bị bắt lỗi build-time** một cách đầy đủ — đây là loại lỗi runtime/silent mà spine's AD-3 (snapshot+delta qua Channel) sẽ đụng trực tiếp nếu code lỡ đặt `Channel` sai chỗ.
- Changelog rc.24/rc.25 công khai (github.com/specta-rs/tauri-specta/releases) không đề cập tương thích Tauri 2.11 hay Channel<T> cụ thể — không có xác nhận positive rõ ràng trong release note, chỉ có "upgrade to latest specta", "semantic types".

**Mức độ ảnh hưởng:** Vẫn ở mức RC sau nhiều năm (rc.16 từ 2024 đến rc.25 giữa 2026) — rủi ro breaking giữa các rc là có thật và đã từng xảy ra với đúng cặp Tauri-2.x/tauri-specta-rc. Spine đã đúng khi bắt buộc pin exact + đưa vào Phase 0 spike, nhưng chưa nêu rõ **case cụ thể cần test** (Channel lồng trong struct, hoặc đổi version Tauri patch).

**Suggested fix:**
- Trong spike Phase 0, thêm test case cụ thể: (a) build thử với đúng `tauri = "=2.11.5"` + `tauri-specta = "=2.0.0-rc.25"` để phát hiện sớm version-lock ẩn kiểu rc.21/2.9.x đã từng xảy ra; (b) thử dùng `Channel<T>` cả ở vị trí hợp lệ (argument/result) lẫn thử (cố ý) đặt sai để xác nhận hành vi hiện tại (build lỗi rõ ràng hay silent).
- Ghi lại kết quả spike vào memlog trước khi khóa AD-3 là "adopted" thay vì để ngầm định RC luôn ổn miễn pin exact.

---

### F4 — [PASS] `rusqlite_migration 2.6.0` ↔ `rusqlite 0.40.2`: khớp chính xác, đã xác nhận qua metadata dependency thật (không phải suy diễn)

**Evidence:** Gọi trực tiếp crates.io API cho `rusqlite_migration@2.6.0/dependencies` → dependency `rusqlite` có `req = "^0.40.0"`. Vì rusqlite là crate 0.x nên `^0.40.0` (Cargo pre-1.0 semver) khớp chính xác dải `>=0.40.0, <0.41.0` — bao trùm `0.40.2` đã pin. Không có xung đột.

Không cần sửa gì — nêu ở đây để xác nhận claim này *đã* được reality-check bằng dữ liệu thật, không phải suy diễn.

---

### F5 — [PASS] `cpal 0.18.2` hỗ trợ WASAPI loopback native (không cần virtual cable)

**Evidence:** cpal 0.18 cho phép mở input stream trực tiếp trên render/output device và tự động set `AUDCLNT_STREAMFLAGS_LOOPBACK`; nhiều dự án (couchlink PR#70, minutes issue#253) đã xác nhận dùng thực tế "no VB-CABLE required". Khớp đúng kỳ vọng AD-10 (capture fan-out) cho nhánh Windows.

---

### F6 — [INFO/PASS-với-lưu-ý] `symphonia 0.6.1` không có decoder Opus thuần Rust bundled — xác nhận đúng hướng "Deferred" đã ghi, nhưng mọi lựa chọn thay thế đều kéo theo phụ thuộc C

**Evidence:** Symphonia core (0.6) không ship Opus decoder; hỗ trợ Opus chỉ qua crate ngoài `symphonia-adapter-libopus`, vốn **bọc libopus (C)**, không phải pure-Rust. (docs.rs/symphonia-adapter-libopus, GitHub releases pdeljanov/Symphonia)

**Nhận xét:** Spine đã tự nhận thức đúng — mục Deferred ghi "Opus trong webm/mkv — chờ spike S1". Finding này chỉ củng cố: khi resolve, team cần chọn tường minh giữa (a) chấp nhận phụ thuộc C (libopus) cho model output cần đọc lại, hoặc (b) loại webm/mkv có Opus khỏi phạm vi hỗ trợ nếu muốn giữ toolchain build "không sidecar/ffmpeg" thuần Rust như constraint đã ghi trong memlog dòng 9 ("không Python/ffmpeg/sidecar"). Đây là **mâu thuẫn tiềm ẩn với constraint đã adopt**, nên nêu rõ trong spike S1 thay vì chỉ "chờ spike".

---

### F7 — [PASS] Asset protocol: config key, FLAC playback, và seek (range request) đều hợp lệ với target platform đã chọn

**Evidence:**
- Config key đúng: `app.security.assetProtocol.enable` + `app.security.assetProtocol.scope` (FsScope, hỗ trợ biến `$APPDATA`). (https://v2.tauri.app/security/asset-protocol/, config schema tauri-apps/tauri)
- FLAC: macOS target của spine là `≥ 14.4` (WebKit tương đương Safari 17+) — vượt xa ngưỡng Safari 13 (macOS Catalina) là nơi Safari bắt đầu hỗ trợ FLAC đầy đủ trong `<audio>` (trước đó Safari 11-12.1 chỉ tải file, không phát trong thẻ audio). WebView2/Edge hỗ trợ FLAC từ Edge 16. Cả hai nền tảng target đều an toàn. (caniuse.com/flac)
- Seek/range: Tauri asset `asset://` protocol hỗ trợ single-range request — đúng loại request mà `<audio>`/`<video>` gửi khi seek; bug multi-range (tauri-apps/tauri#15837/#15838) không ảnh hưởng vì đó là trường hợp multipart/byteranges, không phải luồng phát audio đơn giản.

Không cần sửa — claim FIT trong AD-12 đã đúng thực tế.

---

### F8 — [PASS] Gemini Live API: `BidiGenerateContent`, `sessionResumption`, `contextWindowCompression` vẫn hiện hành

**Evidence:** Tài liệu chính thức Google AI (ai.google.dev/api/live, ai.google.dev/gemini-api/docs/live-api/session-management) tại thời điểm review (9/2026) xác nhận endpoint WebSocket `.../BidiGenerateContent`, field `sessionResumption` trong `BidiGenerateContentSetup`, và `contextWindowCompression` (sliding-window truncation phía server) đều còn đúng như addendum §H mô tả. Không phát hiện lệch.

---

### F9 — [INFO] `keyring 4.2.0` trong MAS sandbox: đã là câu hỏi mở đúng hướng, nhưng nên ghi rõ điều kiện kỹ thuật thay vì chỉ "xác nhận ở Phase 0"

**Evidence:**
- keyring-rs backend Keychain trên macOS dựa vào crate `security-framework` (bindings Security.framework của Apple).
- Trong app đã code-sign + sandbox, gọi Keychain API không đúng entitlement sẽ trả `PlatformError` mã **-34018** ("A required entitlement isn't present") — xác nhận qua ví dụ thực tế trong chính ecosystem keyring-rs.
- Không tìm thấy tài liệu chính thức nào nói keyring 4.x **không dùng được** trong MAS sandbox — vấn đề là **cấu hình entitlement** (`keychain-access-groups`, `com.apple.security.app-sandbox`), không phải giới hạn của crate.

**Suggested fix:** Đổi câu hỏi mở "Tương thích keyring 4 trong sandbox MAS ... xác nhận ở Phase 0" thành cụ thể hơn: "Xác nhận entitlement `keychain-access-groups` + `com.apple.security.app-sandbox` khớp Team ID `B2U85XPU55` khi build MAS, tránh lỗi -34018". Việc này chắc chắn cần làm dù kết quả gần như chắc chắn "hoạt động được nếu cấu hình đúng" — không phải rủi ro về crate.

---

### F10 — [PASS] `tauri 2.11.5` / `tauri-plugin-dialog 2.7.3` / `tauri-plugin-opener 2.5.5` là bản 2.x mới nhất; Tauri 3.0.0-alpha mới ra (13-15/09/2026) không ảnh hưởng quyết định ở lại 2.x

**Evidence:** Gọi trực tiếp crates.io versions API: `tauri` mới nhất ổn định 2.x = `2.11.5` (01/07/2026), `3.0.0-alpha.0/alpha.1` chỉ vừa xuất hiện 13-15/09/2026 — vài ngày trước khi viết spine. `tauri-plugin-dialog` mới nhất 2.x = `2.7.3` (31/08/2026), cũng có `3.0.0-alpha.0` song song. Quyết định "3.0 alpha có — ở lại 2.x" trong memlog dòng `(version)` là chính xác và đã được xác nhận bằng dữ liệu thật, không phải suy diễn.

---

## Không đủ bằng chứng để kết luận (không phải PASS/FAIL, cần tự spike thay vì tin claim)

- **objc2 0.6.4 / coreaudio-sys 0.2.18** cho Core Audio process tap (macOS 14.4+): không tìm được xác nhận trực tiếp rằng đúng combo version này expose API process-tap (`AudioHardwareCreateProcessTap`) qua binding an toàn — đây là API riêng tư/ mới của Apple (macOS 14.4 Sonoma), rủi ro binding chưa cập nhật kịp. Khuyến nghị: spike riêng dựng thử process tap với đúng version pin trước khi coi AD-10/audio là ổn định — không đủ thời gian trong review này để xác nhận qua web search (tài liệu cộng đồng thưa).

---

## Kết luận

Phát hiện quan trọng nhất là **F1 (reqwest feature sai tên — sẽ fail build nếu copy thẳng)** và **F2 (TypeScript 7 + svelte-check đang thực sự gãy, không chỉ "chưa xác nhận")** — cả hai nên được sửa/nâng cấp mức độ ngay trong spine trước khi bước vào implementation, vì cách viết hiện tại ("xác nhận ở Phase 0") đánh giá thấp mức độ chắc chắn của rủi ro đã có bằng chứng công khai. F3 (tauri-specta RC) và F9 (keyring MAS) là các câu hỏi mở đã đúng hướng, chỉ cần cụ thể hóa hành động. Các claim còn lại (F4-F8, F10) đều PASS với evidence trực tiếp.
