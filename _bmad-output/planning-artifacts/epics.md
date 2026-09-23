---
stepsCompleted:
  - step-01-validate-prerequisites
  - step-02-design-epics
  - step-03-create-stories
  - step-04-final-validation
validatedAt: 2026-09-22
reviewedAt: 2026-09-20
reviewStatus: revised-with-open-decisions
storyCount: 55
inputDocuments:
  - _bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/prd.md
  - _bmad-output/planning-artifacts/prds/prd-transcriber_kun-2026-09-17/addendum.md
  - _bmad-output/planning-artifacts/architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md
  - _bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md
  - _bmad-output/planning-artifacts/ux-designs/ux-transcriber_kun-2026-09-18/EXPERIENCE.md
  - docs/rebuild-v3/01-hien-trang-va-pham-vi.md
  - docs/rebuild-v3/02-bai-hoc-va-luu-y.md
  - docs/rebuild-v3/03-dieu-kien-len-store.md
  - docs/rebuild-v3/04-kien-truc-moi-rust-only.md
  - docs/rebuild-v3/05-ke-hoach-rebuild.md
  - docs/rebuild-v3/06-thiet-ke-giao-dien-v3.md
---

# trans-kun v3 - Epic Breakdown

## Overview

Tài liệu phân rã trans-kun v3 thành 7 epic và 55 story để đội phát triển triển khai và kiểm thử. Bản này đã được review và chỉnh sửa ngày 2026-09-20; các quyết định trong bảng “Điều kiện trước triển khai” còn mở, không đồng nghĩa toàn bộ story đã sẵn sàng bắt đầu. `stepsCompleted` giữ nguyên lịch sử workflow, không dùng để suy ra story đã được nghiệm thu.

**Thứ tự ưu tiên nguồn (chủ sản phẩm chốt 2026-09-20):** PRD (+ addendum) là nguồn yêu cầu → Architecture Spine là nguồn quyết định kỹ thuật (khi mâu thuẫn với addendum, spine thắng) → cặp UX DESIGN.md/EXPERIENCE.md là nguồn thị giác và hành vi. Bộ `docs/rebuild-v3/01–06` chỉ là **tài liệu tham khảo bối cảnh**; khi mâu thuẫn với các nguồn trên thì **không** ưu tiên.

**Mâu thuẫn giữa các nguồn đã phát hiện và cách xử lý:**

| # | Chủ đề | Nguồn lệch | Quyết định áp dụng |
|---|---|---|---|
| C1 | Thống kê ẩn danh opt-in | EXPERIENCE.md (nhóm Chẩn đoán có checkbox "Gửi thống kê ẩn danh"; Ad slot "chỉ gửi tổng hợp ẩn danh nếu opt-in"), doc 03, doc 05, doc 06 | **Không có** — PRD FR-41, NFR-1, §10 và addendum §A: v3 không gửi thống kê đi đâu, không có tuỳ chọn opt-in. Chỉ đếm cục bộ. |
| C2 | Định dạng tải Recording | EXPERIENCE.md (WAV/FLAC "+M4A nếu có encoder") | **Chỉ WAV/FLAC** — PRD FR-31: không có lựa chọn M4A/AAC. |
| C3 | Bundle ID | doc 03 (`com.transcriberkun.app`) | `com.transkun.app` — Q9. |
| C4 | Cột Model/Segment/Trạng thái ở dòng phiên | PRD FR-27 (bản gốc), doc 01, doc 06 (bản cũ) | **Theo quyết định đã ghi trong bản epics ngày 2026-09-20 và EXPERIENCE.md** — bỏ Model, số Segment (và cột Trạng thái) khỏi dòng phiên; FR-27 trong PRD hiện vẫn giữ danh sách cột cũ, **chưa được sửa tương ứng**. Giữ quyết định giao diện đã ghi ở epics/EXPERIENCE; cần đồng bộ PRD trước khi baseline triển khai. Không có Copilot (Q1). |
| C5 | Inline FLAC cho model tổng quát | NFR-8 yêu cầu inline; quyết định người dùng ngày 2026-09-23 dùng generateContent cho file | S2 kiểm chứng inline FLAC/JSON schema với model generateContent được chọn; không thêm Files API. |
| C6 | Trạng thái kết nối sống | AD-10/addendum §H chỉ có connecting/reconnecting/stopped; UX cần “Đang transcribe” | Story 4.6 bổ sung `connected` sau setupComplete vào snapshot/event. Đồng bộ contract nguồn trước khi sinh binding; không suy ra từ Recording. |
| C7 | Không mất ghi chú vs debounce | FR-36 hứa force-quit không mất; UX chỉ nêu debounce 800 ms/flush khi finalize | Story 3.5 nêu ACK bền và test kill trước/sau debounce; OQ10 chốt durability, chưa coi lời hứa tuyệt đối là đã chứng minh. |
| C8 | Recording khi lỗi lưu trữ | FR-23 chỉ nêu Dừng/lỗi thiết bị, chưa có disk-full hoặc writer failure | Story 4.5 dừng an toàn và báo storage khi không còn ghi được; bảo toàn bytes đã lưu, cần đồng bộ ngoại lệ vật lý vào PRD. |

Nguồn chính: [PRD](prds/prd-transcriber_kun-2026-09-17/prd.md), [addendum](prds/prd-transcriber_kun-2026-09-17/addendum.md), [Architecture Spine](architecture/architecture-transcriber_kun-2026-09-18/ARCHITECTURE-SPINE.md), [DESIGN](ux-designs/ux-transcriber_kun-2026-09-18/DESIGN.md), [EXPERIENCE](ux-designs/ux-transcriber_kun-2026-09-18/EXPERIENCE.md). Bảng mâu thuẫn ghi cách triển khai/điểm cần đồng bộ; không khẳng định tài liệu nguồn đã được sửa.

Đi tới: [Thứ tự triển khai](#implementation-order) · [Điều kiện trước triển khai](#implementation-gates) · [Epic 1](#epic-1) · [Epic 2](#epic-2) · [Epic 3](#epic-3) · [Epic 4](#epic-4) · [Epic 5](#epic-5) · [Epic 6](#epic-6) · [Epic 7](#epic-7) · [Ánh xạ yêu cầu](#coverage) · [Danh mục yêu cầu](#requirements).

## Epic List

### Epic 1: Cài đặt & mở app lần đầu
Người dùng cài bản beta từ kênh store nội bộ (TestFlight for Mac / package flight), chọn ngôn ngữ, đồng ý Consent, nhập key hợp lệ rồi vào Home mà không bao giờ gặp màn trắng. App có theme sáng/tối, i18n vi/en/ja đồng bộ, key lưu trong kho khoá OS, key pool xoay theo quota, log content-free và khung Settings.
**FRs covered:** FR-1, FR-2, FR-3, FR-4, FR-5, FR-6, FR-7, FR-39, FR-41, FR-43, FR-47, FR-48
**Ghi chú triển khai:** Story 1.1 = dựng repo greenfield theo Structural Seed (AR-1, không có starter template), CI test hai OS, binding tauri-specta (kèm spike AR-14). Có KeyPool + cổng Gemini với Consent gate (AD-6), `AppError`/`Sensitive<T>`/redaction (AD-7, AD-15), settings do Rust sở hữu (AD-8), DB + migration (AD-4). Story duyệt token dark `[ASSUMPTION]` trước khi code theme (UX-DR2). Chốt thư viện i18n và router (AR-5). Phase 0 chạy sớm phần spike S5 (kiểm keyring trong sandbox macOS) và S6 (MSIX), phần tích hợp hoàn tất trong Epic 1 — bản beta nền tảng cài được qua TestFlight for Mac / Store package flight để gỡ sớm rủi ro tài khoản, cert, sandbox.

### Epic 2: Transcribe file & xem kết quả
Người dùng kéo file ghi âm/video họp vào và có transcript có timestamp; nghe lại và click-to-seek từ proxy trong Container (không phụ thuộc file nguồn); tìm trong transcript, export, copy; lỗi từng Chunk không bao giờ bị giấu và chạy lại được phần thiếu; mở lại file trùng không tốn token. Home xuất hiện ở epic này (drop-zone, danh sách phiên, card job).
**FRs covered:** FR-8, FR-9, FR-10, FR-11, FR-12, FR-13, FR-14, FR-15, FR-16, FR-27, FR-32, FR-33, FR-34, FR-35
**Ghi chú triển khai:** Spike S1 (media thuần Rust, Opus), S2 (inline FLAC với model `generateContent`), S8 (FLAC trong WebView) ở các story đầu; ADR của chúng quyết định FR-9, FR-14, FR-32. `JobRegistry` + hàng đợi tuần tự (AD-11), staging + transaction DB + publish file có phục hồi (AD-16, story 2.3), asset protocol scope `$APPDATA/media/**` (AD-12). Tiêu chí QA: lệch timestamp ≤ 2 s so với transcript v2 cùng model (AR-53). Bổ sung trường Chunking vào Settings.

### Epic 3: Thư viện phiên, Ghi chú & Memo
Người dùng tìm lại phiên cũ theo tên và tag (UJ-4), đổi tên inline, xoá sạch dữ liệu, xem dung lượng lưu trữ và xoá toàn bộ; gõ ghi chú tự lưu; sinh, sinh lại và copy memo/議事録 theo template của mình, quản lý template trong Settings (UJ-3 cao trào).
**FRs covered:** FR-28, FR-29, FR-30, FR-36, FR-37, FR-38, FR-40
**Ghi chú triển khai:** Tag picker là một component dùng chung ba nơi (Home, Transcript detail, LiveSetup — UX-DR19) và panel Ghi chú dùng lại ở Live (Epic 4), nên xây thành component độc lập. Xoá phiên bị chặn khi phiên có Job (AD-1: điều phối ở `ipc/`). Memo lỗi không ảnh hưởng transcript (AD-19); badge "Tốn token Gemini". Template mặc định 3 ngôn ngữ (Open Question 8). Bổ sung nhóm Memo và Lưu trữ vào Settings.

### Epic 4: Live transcribe bền vững
Người dùng họp online với transcript realtime từ nguồn system/mic/mixed; kết nối tự nối lại trong suốt và Recording không bao giờ đứt vì mạng; force-quit thì phiên vẫn được phục hồi; Dừng xong vào thẳng Transcript detail, Transcribe lại từ Recording (xem cạnh bản live) và tải Recording WAV/FLAC.
**FRs covered:** FR-17, FR-18, FR-22, FR-23, FR-24, FR-25, FR-26, FR-31
**Ghi chú triển khai:** Epic rủi ro cao nhất. Spike S3 (Live WS 60 phút) và S4 (Core Audio tap trong sandbox, ≥ 3 máy) chạy từ Phase 0; các story đầu epic tích hợp kết quả đã có. `LiveSession` actor, capture fan-out hai nhánh, ring buffer 60 s, `LiveGeneration` (AD-2, AD-10), snapshot + `seq` (AD-3), boot và đóng cửa sổ một chủ (AD-18). Hoàn thiện chế độ cạnh nhau của FR-34. Port `audio/` từ v2 (AR-4). Pill "Đang ghi" và pill kết nối luôn riêng biệt (UX-DR13).

### Epic 5: Dịch realtime, TTS & Nhận diện lại
Người dùng đọc bản dịch song song ngay trong họp, đổi Target hoặc "Không dịch" giữa phiên mà không ngắt phiên, bấm Nhận diện lại khi khách đổi ngôn ngữ, và nghe bản dịch qua loa với volume Teams tự hạ (Ducking).
**FRs covered:** FR-19, FR-20, FR-21
**Ghi chú triển khai:** Dựa trên `LiveSession`/`LiveGeneration` của Epic 4 (restart Target/Nhận diện lại đều là generation mới). TTS phát trong tiến trình Rust, không qua IPC (AD-10); supervisor thiết bị output, marker phục hồi Ducking (AR-42); tắt dịch = setup không có `outputAudioTranscription` (AR-38). Bản dịch không lưu vào Phiên.

### Epic 6: Cấu hình từ xa & Ad slot
Người dùng tuỳ chọn tải và áp dụng bộ cấu hình đề xuất (xem diff, không bao giờ đụng key); Ad slot house ads đúng chuẩn store (Sponsored, Báo cáo quảng cáo, Vì sao tôi thấy quảng cáo này), không tracking, tự fallback khi offline và **không bao giờ hiện ở màn Live**.
**FRs covered:** FR-42, FR-44, FR-45, FR-46
**Ghi chú triển khai:** Hạ tầng `remote/` chung (tải, xác minh ed25519, cache, fallback nhúng sẵn — AD-14) là story đầu, dùng cho cả `ads.json` và `recommended-settings.json`. Ad slot ở đáy sidebar 260 px, ẩn theo route Live bằng layout (AD-13). Lỗi ads/remote cô lập khỏi lõi (AD-19). Cần quyết định Open Question 6 (nơi host) và cơ chế giữ private key.

### Epic 7: Sẵn sàng lên store & ra mắt
Đội build có bản Mac App Store (universal, sandbox) và Microsoft Store (MSIX x64; Arm64 nếu có máy test thật) đạt review: entitlement tối thiểu, hồ sơ submit đầy đủ (Privacy Policy 3 ngôn ngữ, screenshot, mô tả vi/en/ja, review notes + key demo), kiểm sandbox thật, WACK, đo NFR-5/10/11 và diễn tập luồng reviewer (UJ-5).
**FRs covered:** không có FR mới — phủ NFR-4, NFR-7, NFR-10, NFR-11 và AR-44..AR-48 (đóng gói, hồ sơ submit, kiểm chứng store); thực hiện mục tiêu SM-1.
**Ghi chú triển khai:** Nâng bản beta rỗng của Epic 1 thành build store đầy đủ. Kiểm tra WebView2 lúc chạy (Windows), `ITSAppUsesNonExemptEncryption=false`, privacy label, Arm64 chỉ vào bản submit đầu nếu có máy test thật (Open Question 7). Cần Open Question 4 (key demo), 5 (reserve tên), 6 (Privacy Policy host) đã chốt. Checklist smoke thủ công trên máy sạch cả hai OS trước mỗi submit (AR-36).

<a id="implementation-order"></a>

## Thứ tự triển khai và hợp đồng chung

ID story là tham chiếu ổn định, **không phải thứ tự bắt buộc**. Dòng “Phụ thuộc” liệt kê đầu vào kỹ thuật của bản tích hợp; phần spike được chạy sớm bằng harness tối thiểu, không cần xây xong UI của epic chứa nó. Các story nền tảng rộng được chia task trong chính story, nghiệm thu cả phần lõi và UI trước khi đánh dấu done.

| Giai đoạn | Công việc | Điều kiện qua giai đoạn |
|---|---|---|
| Phase 0 | Khung 1.1; phần spike S5/S6 ở 1.11/1.12; S1/S8 ở 2.1; S2 ở 2.2; S3 ở 4.4; S4 ở 4.2; chống thu TTS Windows ở 5.3 | ADR + fixture + bằng chứng máy thật; kiểm chứng API/model/codec/runtime, đo NFR-5/10 sớm. S4 dùng sandbox harness S5; không chờ xong Epic 3 mới kiểm Live. S7 IAP ngoài v3. |
| Nền tảng | Tích hợp Epic 1, giữ lại phần spike đã đạt | Onboarding/Consent/key/theme trên beta nền tảng; không chỉ hello-world |
| File | Epic 2 | File → Job → Transcript/Proxy → replay sau relaunch; retry/cancel và cleanup đã có |
| Thư viện | Epic 3; `remote/` 6.1 có thể làm độc lập khi đủ phụ thuộc | Tag/tìm/Ghi chú/Memo và thao tác xoá nhất quán |
| Live | Epic 4 rồi Epic 5, tích hợp kết quả spike | Capture/Recording độc lập WS; retry/generation/TTS vượt test máy thật |
| Remote & release | Hoàn tất Epic 6; Epic 7 tích hợp mọi story 1–6 | Đạt checklist, đóng gate liên quan, rồi submit/publish |

- **Nguồn API/version:** AR-2, AR-37, AR-38 là baseline từ tài liệu nguồn, chưa phải kết quả kiểm chứng ở lần review này. Spike phải ghi model ID thật, capability, request/response đã redact và version chạy được; chỉ pin exact shape sau khi kiểm chứng. Không tự nâng version, thêm Files API, đổi model hoặc bỏ OS để làm test pass.
- **Bất biến dữ liệu:** `sessions.status` là vòng đời phiên; `transcripts.status` là complete/partial. Gap `disconnected` vẫn hiển thị dù transcript không có cờ partial. Một Phiên có một `primary` hiện hành và tối đa một `retranscribe` hiện hành; candidate chưa commit không được coi là bản chính.
- **Tác vụ dài:** hash/decode/Proxy/Memo/export/remote đều có busy/progress phù hợp và cancel/deadline; cancel không hứa thu hồi token request đã gửi. Khi thoát, ưu tiên Recording/metadata đã lưu, không chờ Proxy vô hạn.
- **UI/binding:** Rust kiểm tra lại Consent, key, validation và busy; không chỉ vô hiệu nút ở UI. Snapshot + đăng ký stream nguyên tử theo cursor; kết quả tới muộn không được hồi sinh dữ liệu bị huỷ/xoá. Các thành phần i18n/theme/accessibility áp dụng ngay khi thêm màn, không để toàn bộ đến Epic 7.
- **Đo kiểm:** fixture thật, OS/CPU/RAM/build và điều kiện mạng phải được ghi cùng kết quả. Test mock chứng minh logic; S2/S3 chứng minh API thật; S4/S5/S6/S8 và checklist chứng minh môi trường store thật. Không thay bằng chứng loại này bằng loại kia.

<a id="implementation-gates"></a>

## Điều kiện trước triển khai

`Q1–Q11` = quyết định đã chốt ở addendum §A; `OQ1–OQ8` = câu hỏi mở PRD §13. OQ9/OQ10 là khoảng trống được phát hiện trong lần review này, không giả định đã được chủ sản phẩm duyệt. Các lựa chọn kỹ thuật không đổi phạm vi do đội build chốt trong ADR; quyết định đổi yêu cầu phải cập nhật nguồn. Gate chỉ chặn story liên quan, không chặn công việc độc lập.

| Mã | Cần chốt/bằng chứng | Chủ trì | Trước story |
|---|---|---|---|
| OQ1 | Opus hỗ trợ hay từ chối rõ, theo S1 | Đội build | 2.1, 2.8 |
| OQ2 / C5 | Inline FLAC/JSON schema của model generateContent đã chọn; nếu không được phải giải quyết xung đột NFR-8 | Build + chủ sản phẩm/kiến trúc | 2.2 |
| OQ3 | FLAC seek trên hai WebView; phương án thay thế khi S8 fail | Đội build | 2.1, 2.7 |
| OQ4 | Chủ key demo, quota và rotation | Chủ sản phẩm | 7.3, 7.5 |
| OQ5 | Tên store, tài khoản, cert/profile/identity | Chủ sản phẩm + release | 1.11, 1.12 |
| OQ6 | URL Privacy Policy/support và host remote; chủ giữ khoá ký | Chủ sản phẩm + release | 1.5 bản beta công khai, 6.1, 6.5, 7.3 |
| OQ7 | Máy Windows Arm64 thật; không có thì submit x64 trước như PRD | Chủ sản phẩm + QA | 7.2 |
| OQ8 | Nội dung Template mặc định vi/en/ja | Chủ sản phẩm | 3.6 |
| OQ9 | Phạm vi “Xoá toàn bộ dữ liệu”: dữ liệu họp hay cả key/settings/Consent/Template | Chủ sản phẩm | 3.4 |
| OQ10 / C7 | Durability Ghi chú: cơ chế lưu và giới hạn mất ký tự chưa ACK khi kill | Chủ sản phẩm + build | 3.5 |
| UX-DR2 | Token dark còn ASSUMPTION phải được duyệt theo DESIGN | Chủ sản phẩm/UX | 1.3 phần token dark |
| S3 | ACK/resumption thực tế, alias model và “Không dịch” không yêu cầu audio dịch | Đội build | 4.4, 5.1 |
| S6 / Windows TTS | Runtime thiếu, capability thật và chống thu lại app trên Windows 10 1809/11 | Đội build | 1.12, 4.3, 5.3, 7.2 |
| C4, C6, C8 | Đồng bộ nguồn về cột Home, connected và lỗi lưu trữ Recording | Chủ sản phẩm/kiến trúc | 2.9, 4.6, 4.5 |

Giả định gốc về ngưỡng hiệu năng, cách export partial, nhãn Memo cũ và preview diff vẫn giữ thẻ `[ASSUMPTION]` ở danh mục nguồn. Chúng là mục cần kiểm chứng/duyệt theo story, không tự trở thành kết quả pass sau lần review tài liệu này.

<a id="epic-1"></a>

## Epic 1: Cài đặt & mở app lần đầu

Người dùng cài bản beta từ kênh store nội bộ, chọn ngôn ngữ, đồng ý Consent, nhập key hợp lệ rồi vào Home mà không bao giờ gặp màn trắng. App có theme sáng/tối, i18n vi/en/ja đồng bộ, key lưu trong kho khoá OS, key pool xoay theo quota, log content-free và khung Settings.

### Story 1.1: Dựng repo greenfield, CI hai OS và binding IPC typed

**Phụ thuộc:** Không có; đầu vào là tài liệu nguồn và môi trường dev.

As a đội build (dev + AI agent),
I want một repo mới dựng đúng Structural Seed, build và test được trên macOS lẫn Windows,
So that mọi epic sau có nền chạy được, version được pin và binding IPC được sinh tự động.

**Acceptance Criteria:**

**Given** repo trống
**When** khởi tạo dự án
**Then** có cấu trúc đúng Structural Seed: `src/` (Svelte 5 + Vite + TypeScript 6, `lib/stores`, `lib/keymap.ts`, `lib/bindings.ts`, `routes`, `components`, `i18n/{vi,en,ja}.json`) và `src-tauri/src/{ipc,core,settings,library,transcribe,live,memo,ads,gemini,media,audio,db,secrets,remote}` với `mod.rs` tối thiểu
**And** có `tauri.conf.json`, hai overlay `tauri.appstore.conf.json` / `tauri.msix.conf.json` (khung rỗng) và thư mục `.github/workflows/`

**Given** bảng Stack của Architecture
**When** kiểm tra manifest
**Then** version khớp bảng, `Cargo.lock` và `package-lock.json` được commit, tauri-specta/specta pin exact `2.0.0-rc.25` (AR-2)
**And** version app có một nguồn duy nhất (`Cargo.toml` → `tauri.conf.json`)

**Given** máy macOS ≥ 14.4 hoặc Windows 10 1809+/11
**When** chạy `tauri dev`
**Then** một cửa sổ Svelte 5 mở với kích thước mặc định 1280×800 và tối thiểu 1024×680
**And** một command mẫu (`app_version`) được gọi qua binding sinh bởi tauri-specta, không viết tay `invoke`

**Given** `Channel<T>` typed của tauri-specta rc.25 trên Tauri 2.11
**When** chạy spike với một stream mẫu có `seq`
**Then** kết quả (chạy được hoặc phải bọc bằng type viết tay) được ghi thành ADR và AD-3 giữ nguyên (AR-14)

**Given** một pull request
**When** CI chạy
**Then** `cargo test` và Vitest chạy trên cả macOS và Windows
**And** CI fail nếu `bindings.ts` lệch so với bản sinh ra (AR-36)

**Given** manifest Rust và npm
**When** chạy kiểm tra tự động
**Then** không có `tauri-plugin-{shell,fs,updater,process,http,store}`, không sidecar hay binary ngoài; thêm vào sẽ làm CI fail (AR-3, NFR-4)

### Story 1.2: Nền lưu trữ, lỗi có category và log content-free

**Phụ thuộc:** 1.1

As a người dùng,
I want dữ liệu và cấu hình lưu bền trong Container, lỗi có nhãn dễ hiểu, và log không bao giờ chứa nội dung họp hay key,
So that tôi tin app khi để nó ghi họp và khi gửi log cho hỗ trợ.

**Acceptance Criteria:**

**Given** lần chạy đầu tiên
**When** app khởi động
**Then** `db/` tạo `app.db` (WAL, một connection duy nhất) trong Container của OS/sandbox (không tuỳ chỉnh được) và chạy migration versioned chỉ tiến
**And** migration đầu tiên chỉ tạo bảng `settings(key, value)`; bảng khác do story cần chúng tạo (AR-18, AR-43, Q7)

**Given** dịch vụ settings
**When** ghi hoặc đọc giá trị
**Then** chỉ đi qua `settings/` bằng `settings_get` / `settings_save` typed, có giá trị mặc định cho từng khoá, không dùng `tauri-plugin-store`
**And** mỗi lần settings đổi phát một event toàn cục tần suất thấp (AR-22, AD-3)

**Given** một lỗi bất kỳ đi qua IPC
**When** UI nhận kết quả
**Then** lỗi là `AppError { category, code, detail_redacted }` với `category` thuộc 8 giá trị `quota | auth | model | network | format | permission | storage | blocked`
**And** ánh xạ `code → category` (Quota, Auth, Model, Request, Timeout, Network, Blocked, Shape, Tls) chỉ nằm ở `core/error` và có test phủ mọi `code` (AD-7, NFR-9)

**Given** giá trị bọc `core::Sensitive<T>`
**When** in bằng `{:?}` hoặc `{}`
**Then** kết quả là `[redacted]`
**And** layer redaction của `tracing` thay các chuỗi `AIza…`, `AQ.…`, URL và `authorization` (unit test)

**Given** log xoay vòng bằng `tracing-appender`
**When** chạy một phiên mẫu giả chứa transcript và key giả
**Then** test tự động grep toàn bộ log không thấy nội dung hay key (FR-41, NFR-1, AR-29)

**Given** ID Phiên, Transcript, Job
**When** sinh mới
**Then** dùng UUIDv7 và helper đường dẫn của `core` chỉ nhận ID tự sinh, từ chối chuỗi từ người dùng hoặc server làm thành phần đường dẫn (NFR-12, AR-19)

### Story 1.3: Token thị giác, theme sáng/tối và app shell

**Phụ thuộc:** 1.2

As a người dùng,
I want giao diện yên tĩnh, nhất quán, có theme sáng/tối theo hệ thống,
So that mắt tôi chịu được cả buổi họp và mọi màn dùng chung một ngôn ngữ thiết kế.

**Acceptance Criteria:**

**Given** bảng token dark trong DESIGN.md còn gắn `[ASSUMPTION]`
**When** bắt đầu story
**Then** chủ sản phẩm hoặc UX duyệt bảng đó và quyết định được ghi lại trước khi code
**And** token light và dark được triển khai thành CSS variable đúng giá trị đã duyệt, không thêm màu thứ năm (UX-DR1, UX-DR2)

**Given** bộ kiểm tra tương phản tự động
**When** CI chạy
**Then** mọi cặp màu mang nội dung đạt WCAG AA (text ≥ 4.5:1), đo **riêng từng theme**, và fail build nếu không đạt (FR-48, NFR-11)

**Given** typography
**When** build và chạy app
**Then** IBM Plex Sans và Mono được bundle trong app, không có request mạng tới Google Fonts; thang 8 bậc 11–22 px; số trong mono dùng `tabular-nums`; có fallback chữ Nhật (UX-DR4)

**Given** lint frontend
**When** code có `box-shadow` ngoài segmented control đang chọn và menu/dialog/toast/popover
**Then** lint fail (UX-DR6)
**And** có sẵn token spacing/radius và focus ring `2px solid accent` offset 2px cho light và dark; icon chỉ dùng Lucide; `prefers-reduced-motion` tắt pulse và caret (UX-DR5, 7, 8, 9)

**Given** app shell
**When** mở app
**Then** có sidebar 260 px (logo, nav Trang chủ / Cài đặt, chỗ cho card job) và header cao 64 px
**And** ở cửa sổ 1024–1279 px panel phụ đóng trước còn sidebar giữ 260 px; cửa sổ không nhỏ hơn 1024×680 (UX-DR23)

**Given** router client
**When** điều hướng
**Then** các route `/onboarding`, `/home`, `/settings/:group` hoạt động, deep link nội bộ được, và route của epic sau chỉ được đăng ký khi epic đó có màn thật (UX-DR24)
**And** lựa chọn thư viện router frontend được chốt trong ADR (AR-5)

**Given** giá trị theme trong settings (Theo hệ thống / Sáng / Tối)
**When** giá trị đổi hoặc OS đổi sáng/tối
**Then** giao diện đổi tức thì, không cần khởi động lại (FR-48)
**And** có store `stores/settings.svelte.ts` bọc binding và `lib/keymap.ts` (phím tắt chỉ trong app, không global shortcut); component không gọi `invoke` trực tiếp (AR-27)

### Story 1.4: i18n vi/en/ja và Onboarding chọn ngôn ngữ

**Phụ thuộc:** 1.3

As Linh mở app lần đầu,
I want app nói ngôn ngữ của tôi ngay màn đầu tiên và đổi được tức thì,
So that tôi hiểu mọi bước mà không phải khởi động lại.

**Acceptance Criteria:**

**Given** yêu cầu i18n của FR-47
**When** chọn thư viện
**Then** quyết định `i18next` hay module tự viết được ghi lại trong ADR
**And** key theo dạng `<màn>.<khối>.<nhãn>` với ba file `vi`, `en`, `ja` cùng một tập key (AR-5, AR-35)

**Given** CI
**When** một key thiếu ở bất kỳ ngôn ngữ nào
**Then** CI fail (FR-47)
**And** bộ key migrate từ v2 chỉ mang key còn dùng ở v3, bỏ nhóm setup, whisper, copilot, updates

**Given** lần mở đầu tiên với ngôn ngữ hệ thống là vi, en hoặc ja
**When** vào Onboarding
**Then** bước "Ngôn ngữ" mặc định theo hệ thống; ngôn ngữ hệ thống không thuộc vi/en/ja thì fallback `en` (FR-1)
**And** có 3 radio card vi/en/ja, card trùng ngôn ngữ hệ thống có badge "Theo hệ thống" (UX-DR25)

**Given** người dùng đổi radio ngôn ngữ
**When** chọn một ngôn ngữ khác
**Then** toàn bộ Onboarding (stepper, nút, mô tả) đổi tức thì không reload
**And** giá trị `uiLanguage` được lưu bền qua settings (FR-1, FR-47)

**Given** onboarding card
**When** hiển thị
**Then** card rộng 600 px với stepper "Ngôn ngữ · Dữ liệu · API key", không có bước kiểm tra môi trường (UX-DR25)
**And** nút và badge dùng `min-width`, chuỗi ja/vi dài nhất không bị cắt (UX-DR46)

**Given** app chưa hoàn tất Onboarding
**When** mở app
**Then** route chuyển về `/onboarding`; sau khi hoàn tất thì mở thẳng `/home`

**Given** người dùng đã từ chối Consent hoặc Consent trong binary vừa tăng phiên bản
**When** khởi động lại hoặc mở deep link
**Then** guard route ưu tiên trạng thái Consent: đã từ chối chỉ vào Settings/About và đường quay lại Consent; phiên bản cũ phải đồng ý phiên bản mới trước mọi Gemini request; không mắc vòng lặp “chưa hoàn tất Onboarding” (FR-2).

### Story 1.5: Onboarding Consent

**Phụ thuộc:** 1.4

As Linh,
I want biết rõ audio và transcript sẽ đi tới đâu trước khi đồng ý,
So that tôi kiểm soát dữ liệu họp của mình.

**Acceptance Criteria:**

**Given** bước "Dữ liệu" của Onboarding
**When** hiển thị
**Then** có sơ đồ "Máy của bạn → (bằng key của bạn) → Google Gemini", ba gạch đầu dòng, nêu đích danh Google là bên nhận, link Privacy Policy mở bằng trình duyệt ngoài, dòng "Văn bản đồng ý phiên bản N" (FR-2, UX-DR26)
**And** nội dung có đủ ba ngôn ngữ

**Given** URL Privacy Policy
**When** cấu hình
**Then** URL lấy từ một hằng số cấu hình, giá trị thật chốt cùng Open Question 6 (hồ sơ Epic 7)

**Given** phiên bản Consent là hằng số compile-time đi cùng văn bản trong binary
**When** người dùng bấm "Đồng ý và tiếp tục"
**Then** settings lưu bền số phiên bản đã đồng ý; mở lại app không hỏi lại
**And** khi số phiên bản trong binary tăng thì Consent hiện lại (AD-8)

**Given** người dùng chưa đồng ý
**When** app chạy ở bất kỳ màn nào
**Then** test với transport giả xác nhận không có request nào tới Google (FR-2)

**Given** người dùng bấm "Không đồng ý"
**When** vào app
**Then** app ở chế độ chỉ xem Cài đặt và Giới thiệu, có banner "quay lại đồng ý" đưa về màn Consent (FR-2, UX-DR40)
**And** không màn trắng, không dialog lặp

### Story 1.6: Kho khoá OS và Key pool

**Phụ thuộc:** 1.2

As Linh có hai Gemini key,
I want key lưu an toàn trong Keychain/Credential Manager và app tự xoay key khi hết quota,
So that key không lộ trong file cấu hình và buổi họp không gián đoạn vì quota.

**Acceptance Criteria:**

**Given** một hoặc nhiều key (cách nhau dấu phẩy)
**When** lưu qua `secrets/` (`keyring`)
**Then** key vào kho khoá OS, file `settings` và `app.db` không chứa key (test quét) (FR-5)
**And** chấp nhận định dạng `AIza…` và `AQ.…`

**Given** người dùng xoá key
**When** thao tác hoàn tất
**Then** key bị xoá khỏi kho khoá thật, không chỉ khỏi UI (FR-5)

**Given** kho khoá không truy cập được
**When** đọc hoặc ghi key
**Then** trả `AppError` category `storage` kèm hành động gợi ý, các setting khác không mất (FR-5, UX-DR41)

**Given** trait `KeyProvider` và actor `KeyPool` (mpsc + oneshot, không `Arc<Mutex>` bọc state)
**When** request gặp 429
**Then** key đó nghỉ 60 s và request thử lại bằng key kế; 401/403 loại key khỏi vòng tới khi người dùng sửa và thử key kế nếu còn; 400/404 và timeout không xoay (FR-6, AD-2)
**And** timeout không bao giờ gửi cùng request sang key khác

**Given** mọi key đang nghỉ
**When** có request cần key
**Then** Job chờ tối đa tổng cộng 180 s cho mỗi Chunk (không reset ngân sách khi xoay key), vẫn huỷ được; Memo bị giới hạn bởi deadline 90 s; list-model/key-test có timeout và cancel xác định ở story 1.7; Live dùng reconnect của story 4.4. Hết key vì 401/403 trả category `auth` ngay, không đưa key bị loại trở lại vòng (FR-6)

**Given** lớp ưu tiên `Live > Job > Memo`
**When** Live nhận 429 trong khi Job đang chạy
**Then** Job ngừng nhận key mới tới khi cooldown hết (AD-6)
**And** tham số (cooldown, attempts, timeout) chỉ định nghĩa ở `gemini::params`; toàn bộ được test với port giả và đồng hồ giả (AR-36, AR-40)

**Given** người dùng kiểm tra nhiều key, trong đó có key sai
**When** `keys_test` kiểm tra từng key
**Then** trả kết quả riêng theo ID/nhãn đã che của từng key, không dùng key tốt trong pool để báo key sai là hợp lệ; key chỉ được loại/khôi phục đúng theo kết quả của chính nó.

**Given** key bị xoá hoặc sửa trong lúc có request
**When** cấp key cho request tiếp theo
**Then** không cấp lại key đã xoá; request đang chạy được huỷ hoặc hoàn tất theo chính sách hiện hành, không làm key cũ xuất hiện lại trong kho khoá hay pool.

### Story 1.7: Cổng Gemini duy nhất và liệt kê model

**Phụ thuộc:** 1.5, 1.6

As Linh,
I want app kiểm tra key và liệt kê model qua một cổng duy nhất tôn trọng Consent,
So that không request nào tới Google trước khi tôi đồng ý và mọi lỗi mạng đều được nói rõ.

**Acceptance Criteria:**

**Given** module `gemini/`
**When** bất kỳ feature nào cần gọi Google
**Then** mọi request đi qua đây (reqwest `rustls` platform verifier), feature không tự dựng URL hay body (AR-20, AR-37)

**Given** Consent phiên bản hiện hành chưa được ghi nhận
**When** có request bất kỳ, kể cả liệt kê model
**Then** `gemini/` từ chối trước khi mở kết nối và test với transport giả xác nhận không có request nào (FR-2, AD-6)

**Given** command `models_list(kind)` với `kind ∈ {transcribe, live, memo}`
**When** gọi
**Then** app gọi `GET /v1beta/models` với header `x-goog-api-key`, lọc theo năng lực, danh sách live chỉ gồm model hỗ trợ Live (FR-7)
**And** test khoá exact shape của request với cả key `AIza…` và `AQ.…` (FR-3, AR-36)

**Given** phản hồi lỗi từ Google hoặc từ mạng
**When** phân loại
**Then** trả code tương ứng `Quota | Auth | Model | Request | Timeout | Network | Blocked | Shape`; lỗi CA/proxy doanh nghiệp dùng code `Tls`, thuộc category `network` và có thông điệp riêng (NFR-6, AR-21)
**And** thông điệp không lộ key hay URL (NFR-9)

**Given** máy có proxy hoặc CA doanh nghiệp (ví dụ Zscaler)
**When** gọi Gemini
**Then** dùng CA store hệ thống; việc xác minh này được ghi trong checklist kiểm thử thủ công (NFR-6)

**Given** không có timer hay gọi nền nào
**When** app rảnh
**Then** không có request tới Google phát sinh ngoài thao tác người dùng (NFR-8, AD-6)
**And** mặc định model theo Q10 (`gemini-flash-lite-latest`, `gemini-3.5-live-translate-preview`) nằm trong `gemini::params` (FR-7)

**Given** API trả danh sách model phân trang hoặc tên alias không biểu lộ năng lực/version
**When** liệt kê và chọn adapter
**Then** đọc hết các trang trong một thao tác có thể huỷ; chỉ phân loại từ metadata hoặc bảng capability đã kiểm chứng trong S2/S3; alias không parse được không được tự suy ra major version. Timeout list-model/key-test được đặt tập trung và kiểm thử; lỗi không xoá cấu hình hiện có (FR-7).

### Story 1.8: Onboarding nhập key và Home khởi đầu khi chưa có key

**Phụ thuộc:** 1.7, 1.9

As Linh,
I want dán key, thấy kết quả kiểm tra tại chỗ và vào Home dùng được kể cả khi bỏ qua bước key,
So that tôi không bao giờ gặp màn trắng.

**Acceptance Criteria:**

**Given** bước "API key" của Onboarding
**When** người dùng dán key và bấm "Kiểm tra key"
**Then** app tải danh sách model và báo kết quả trong vùng `role=status`: "Key hợp lệ, N model khả dụng" (FR-3, UX-DR27)
**And** ô key là `type=password` có nút hiện/ẩn, chấp nhận một hoặc nhiều key

**Given** key không hợp lệ, lỗi mạng, lỗi CA hoặc quota
**When** kiểm tra
**Then** lỗi hiện đúng category kèm một câu hướng dẫn và sửa được tại chỗ, không rời màn (FR-3)

**Given** nút "Bỏ qua, nhập sau"
**When** bấm
**Then** vào `/home` ở trạng thái chưa có key; app không bao giờ vào Home ở trạng thái trắng (FR-3, FR-4)

**Given** Home ở lần đầu, chưa có phiên
**When** hiển thị
**Then** có header và dòng hướng dẫn "Chưa có phiên nào…" (card kéo file và nút Live được thêm ở Epic 2 và Epic 4)
**And** nếu chưa có key hợp lệ thì có banner warning "Chưa có API key hợp lệ — các tính năng cần Gemini đang tắt" với link "Nhập key" tới Settings → Gemini (FR-4, UX-DR40)

**Given** component banner và mẫu nút vô hiệu
**When** dùng ở nhiều nơi
**Then** banner có ba phần (tiêu đề = category, một câu nguyên nhân + cách sửa, một nút hành động), ba biến thể, tối đa hai banner cùng lúc theo mức danger > warning > info (UX-DR17)
**And** nút vô hiệu dùng `opacity .45` với tooltip nêu lý do và lối tắt, tới được bằng bàn phím, không bao giờ bị ẩn (UX-DR10, UX-DR45)

**Given** ba tình huống: bỏ qua key, từ chối Consent, key sai
**When** kiểm thử luồng Onboarding
**Then** bỏ qua key sau khi đã Consent → Home; từ chối Consent → chỉ Settings/About; key sai → ở bước key để sửa hoặc chủ động bỏ qua; cả ba không có màn trắng, crash hay dialog lặp (FR-2–4)

### Story 1.9: Settings — khung, Chung và Gemini

**Phụ thuộc:** 1.3, 1.4, 1.7

As Linh,
I want chỉnh ngôn ngữ, theme, key và model ở một nơi rõ ràng,
So that tôi tự sửa được khi key sai hoặc model đổi tên.

**Acceptance Criteria:**

**Given** màn Cài đặt
**When** mở `/settings/:group`
**Then** có nav trái 220 px và nội dung dạng bảng hai cột `220px | control`, mỗi trường có icon (?) tooltip tới được bằng bàn phím và helper text bền (FR-39, UX-DR11, UX-DR39)
**And** các nhóm chưa có tính năng thật (Chunking, Live, Memo, Lưu trữ, Cấu hình đề xuất) chỉ hiện khi epic của chúng hoàn tất

**Given** nhóm Chung
**When** người dùng đổi ngôn ngữ UI hoặc theme (Theo hệ thống / Sáng / Tối)
**Then** thay đổi áp dụng tức thì và được lưu bền (FR-39, FR-47, FR-48)

**Given** nhóm Gemini
**When** dùng ô key
**Then** ô key mặc định ẩn ký tự, có nút hiện/ẩn, "Kiểm tra key" với kết quả trong `role=status` kèm thời điểm kiểm tra, và xoá key xoá khỏi kho khoá thật (FR-5, FR-39)

**Given** ba select model (transcribe file, live, memo)
**When** bấm "Tải danh sách"
**Then** nút chuyển trạng thái đang chạy nhưng select vẫn dùng được với giá trị hiện tại; danh sách live chỉ hiện model Live (FR-7)
**And** lỗi tải hiện banner category tại chỗ và **không** xoá giá trị đang cấu hình (UX-DR40)

**Given** người dùng nhập tên model không có trong danh sách
**When** lưu
**Then** tên được chấp nhận kèm cảnh báo nhẹ (FR-7)
**And** giá trị không hợp lệ (ví dụ rỗng) bị chặn tại chỗ, không đợi tới lúc Lưu (FR-39)

**Given** một lỗi category `model` từ Google ở luồng bất kỳ về sau
**When** hiển thị
**Then** có sẵn component cảnh báo tại chỗ với lối tắt tới Settings → Gemini và app không tự đổi model (FR-7)
**And** ngôn ngữ transcribe (FR-16) được thêm vào nhóm Gemini ở Epic 2

### Story 1.10: Settings — Chẩn đoán và Giới thiệu & Quyền riêng tư

**Phụ thuộc:** 1.5, 1.9

As Linh hoặc người hỗ trợ,
I want xuất gói nhật ký an toàn và xem lại văn bản đồng ý,
So that gửi log mà không lộ nội dung họp và biết mình đã đồng ý điều gì.

**Acceptance Criteria:**

**Given** nhóm Chẩn đoán
**When** bấm "Xuất gói nhật ký"
**Then** gói chỉ gồm file trong allow-list và được lưu qua dialog lưu của hệ thống (FR-41, AR-29)
**And** test xác nhận file ngoài allow-list không bao giờ vào gói

**Given** "Xoá nhật ký"
**When** bấm
**Then** log xoay vòng bị xoá (FR-41)

**Given** một phiên mẫu giả đã chạy
**When** xuất gói
**Then** test grep gói không thấy transcript, bản dịch, ghi chú, memo hay key (FR-41, NFR-1)

**Given** mục Chẩn đoán
**When** hiển thị
**Then** có bộ đếm cục bộ số Phiên, số lỗi theo category và số crash (đếm bằng marker "tắt sạch" khi khởi động), không nội dung (FR-41)
**And** không có checkbox thống kê hay opt-in nào và không có request nào gửi số liệu đi (FR-41, mâu thuẫn C1)

**Given** nhóm Giới thiệu & Quyền riêng tư
**When** mở
**Then** hiện version (một nguồn), tác giả, liên hệ hỗ trợ, link Privacy Policy mở trình duyệt ngoài và "Xem lại văn bản đồng ý" (FR-43)
**And** xem lại Consent không đặt lại số phiên bản đã đồng ý

### Story 1.11: Bản beta macOS lên TestFlight for Mac (spike S5)

**Phụ thuộc:** 1.1, 1.2, 1.6

As đội build,
I want một bản build macOS rỗng tính năng chạy trong sandbox và lên TestFlight,
So that rủi ro cert, profile, sandbox và keyring lộ sớm và tester nội bộ cài được từ kênh store.

**Acceptance Criteria:**

**Given** overlay `tauri.appstore.conf.json`
**When** build `store-mac`
**Then** entitlements chỉ gồm `app-sandbox`, `network.client`, `device.audio-input`, `files.user-selected.read-write` (và `keychain-access-groups` nếu `keyring` cần), `application-identifier B2U85XPU55.com.transkun.app`, provisioning profile nhúng, category Productivity, `minimumSystemVersion 14.4`, universal (AR-44, AD-17)
**And** Info.plist có `NSMicrophoneUsageDescription`, `NSAudioCaptureUsageDescription`, `ITSAppUsesNonExemptEncryption=false` và không có updater

**Given** cert Apple Distribution và Mac Installer Distribution
**When** chạy `productbuild` rồi `xcrun altool` với App Store Connect API key
**Then** gói qua TestFlight processing và cài được từ TestFlight for Mac (AR-46)
**And** App ID `com.transkun.app` đã đăng ký và tên "trans-kun" đã reserve (Open Question 5)

**Given** bản build chạy từ `/Applications`
**When** kiểm tra sandbox thật
**Then** `codesign -d --entitlements` khớp danh sách, dialog chọn file mở được file trong sandbox, relaunch vẫn giữ settings (AR-44)

**Given** `keyring` trong sandbox
**When** lưu, đọc, xoá key
**Then** hoạt động đúng với entitlement đã chọn; nếu không thì ADR ghi phương án thay thế và đề xuất sửa Architecture Spine (AR-11)

**Given** workflow CI `store-mac`
**When** chạy với secrets cert/API key
**Then** build thành công và secrets không xuất hiện trong log (AR-48)
**And** kết quả spike được ghi thành ADR S5

### Story 1.12: Bản beta Windows MSIX qua Store package flight (spike S6)

**Phụ thuộc:** 1.1, 1.2, 1.6

As đội build,
I want một gói MSIX rỗng tính năng cài được qua Store package flight,
So that rủi ro đóng gói Windows, WACK và kho khoá lộ sớm.

**Acceptance Criteria:**

**Given** overlay `tauri.msix.conf.json` và `Package.appxmanifest`
**When** build `store-win` bằng `winapp` CLI hoặc `tauri-windows-bundle`
**Then** ra MSIX x64 với identity/publisher từ Partner Center, capability `microphone` và bộ icon assets (AR-45, AR-46)
**And** Arm64 được build thử; chỉ kiểm bằng máy thật nếu có (Open Question 7)

**Given** máy Windows sạch (10 1809+ hoặc 11)
**When** cài rồi gỡ gói
**Then** cài và gỡ sạch, app mở được, và kết quả có/không có sẵn WebView2 Evergreen được ghi lại (AR-12)

**Given** gói MSIX
**When** chạy WACK
**Then** WACK pass (AR-45)

**Given** app chạy trong package identity
**When** lưu, đọc, xoá key qua `keyring`
**Then** Credential Manager hoạt động đúng (FR-5)

**Given** Partner Center
**When** upload lên Store package flight
**Then** tester nội bộ cài được từ flight; tên app đã reserve (Open Question 5)
**And** workflow CI `store-win` build được artifact và kết quả spike được ghi thành ADR S6 (AR-48)

<a id="epic-2"></a>

## Epic 2: Transcribe file & xem kết quả

Người dùng kéo file ghi âm/video họp vào và có transcript có timestamp; nghe lại và click-to-seek từ proxy trong Container (không phụ thuộc file nguồn); tìm trong transcript, export, copy; lỗi từng Chunk không bao giờ bị giấu và chạy lại được phần thiếu; mở lại file trùng không tốn token. Home xuất hiện ở epic này (drop-zone, danh sách phiên, card job).

### Story 2.1: Media pipeline thuần Rust và kiểm chứng phát FLAC (spike S1, S8)

**Phụ thuộc:** 1.1, 1.2

As Minh nhận file recording Zoom 90 phút,
I want app đọc được các file ghi âm/video họp phổ biến và cắt thành Chunk mà không cần cài ffmpeg,
So that transcribe file chạy được ngay trên máy công ty sau khi cài từ store.

**Acceptance Criteria:**

**Given** bộ ít nhất 11 file mẫu thật phủ từng định dạng cam kết (mp3, m4a, mp4, mov, mkv, webm, wav, flac, ogg/vorbis, aiff và caf), cùng fixture container/codec không hỗ trợ
**When** decode bằng `media/decode` (symphonia, streaming, f32 mono)
**Then** mọi file decode đúng thời lượng (`n_frames / sample_rate`, fallback đếm khi thiếu) với tốc độ ≥ 20× realtime trên máy 4 nhân; số đo được ghi lại (NFR-5, AR-41)
**And** với file video chỉ demux track audio đầu tiên và không nạp cả file vào RAM

**Given** audio đã decode
**When** `media/resample` (rubato) về 16 kHz mono và `media/chunk` cắt theo `chunk_seconds` rồi encode FLAC (flacenc)
**Then** Chunk 5 phút của bộ mẫu nhỏ hơn 14 MB
**And** Chunk vượt ngưỡng an toàn 14 MB được chia nhỏ tiếp (3 phút là lần thử đầu, không phải fallback duy nhất); trước mỗi request đo toàn bộ payload gồm base64 và JSON để không vượt giới hạn đã kiểm chứng ở S2. Chia tới khi vừa giới hạn; không còn chia được thì lỗi `format`, không gửi request quá cỡ (FR-13, AR-37)

**Given** track Opus trong webm/mkv
**When** thử decode
**Then** kết quả spike S1 (thêm decoder hay từ chối rõ) được ghi thành ADR, cân nhắc với AD-17, và thông báo lỗi của FR-9 phản ánh đúng quyết định đó (Open Question 1)

**Given** file `avi`, `wmv`, `flv`, `ts` hoặc track không giải mã được
**When** `media/probe` kiểm tra
**Then** trả `AppError` category `format` nêu định dạng nên chuyển sang (mp4, m4a, mp3) trước khi bất kỳ Phiên nào được tạo (FR-9)

**Given** file nguồn
**When** tính hash
**Then** dùng SHA-256 nội dung file theo kiểu streaming (AR-41)

**Given** `media/proxy` tạo Proxy FLAC 16 kHz mono
**When** WebView (WKWebView và WebView2) phát Proxy bằng `<audio>` qua asset protocol có range request
**Then** seek tới điểm bất kỳ của file 90 phút phản hồi ≤ 500 ms trên cả macOS và Windows (FR-32, spike S8)
**And** nếu không đạt, ADR chọn AAC native hoặc player trong Rust và `media/proxy` là điểm đổi duy nhất (AR-13)

**Given** asset protocol
**When** WebView cố đọc file ngoài `$APPDATA/media/**`
**Then** bị từ chối; scope chỉ gồm `$APPDATA/media/**` (AD-12)

### Story 2.2: Gemini transcribe file — model tổng quát và parser chịu lỗi (spike S2)

**Phụ thuộc:** 1.7, 2.1

As Minh,
I want app gửi từng Chunk tới model Gemini generateContent đã chọn và nhận về Segment đúng định dạng,
So that transcript có timestamp đáng tin và không phụ thuộc việc tôi chọn model gì.

**Acceptance Criteria:**

**Given** model Gemini thông thường hỗ trợ generateContent
**When** gửi một Chunk
**Then** gọi `POST /v1beta/models/{model}:generateContent` với `inline_data {mime_type: audio/flac}`, `responseMimeType: application/json` và `responseSchema` mảng `{start: "MM:SS", end: "MM:SS", text}`, prompt yêu cầu Segment 5–15 s
**And** test khoá exact JSON shape của request (FR-14, AR-37, AR-36)

**Given** tên model
**When** dựng `generationConfig`
**Then** tên version đã kiểm chứng dùng thinking config phù hợp; alias `gemini-flash-lite-latest` và tên tự nhập không đoán version, để model dùng mặc định và giữ nguyên lựa chọn của người dùng (FR-7)

**Given** file có tiếng Việt và tiếng Nhật xen kẽ
**When** transcribe bằng model generateContent đã chọn
**Then** prompt yêu cầu tự nhận diện ngôn ngữ, giữ nguyên lời nói từng ngôn ngữ và không dịch; `*-transcribe` và Live Translate không nằm trong luồng file (FR-14)

**Given** spike S2
**When** kiểm chứng model generateContent được chọn
**Then** ADR ghi bằng chứng inline FLAC, JSON schema và giới hạn payload. Nếu model từ chối, trả lỗi model rõ ràng; không dùng Files API hoặc tự đổi model (NFR-8)

**Given** response JSON bị cắt cụt, vỡ hoặc bọc trong fence markdown
**When** parse
**Then** parser cứu mọi Segment hợp lệ và báo lại phần không cứu được để thành Khoảng thiếu (FR-15)

**Given** Segment có `MM:SS` tương đối Chunk
**When** merge
**Then** cộng offset tuyệt đối của Chunk và không Segment nào lùi thời gian so với Segment trước (FR-13)

**Given** mọi request của story
**When** thực thi
**Then** đi qua cổng `gemini/` với lớp ưu tiên `Job`, timeout 120 s cho mỗi Chunk và không fan-out khi timeout (AR-40)
**And** lỗi được phân loại với cờ có-thể-thử-lại: 5xx thử lại được; 400/404 và nội dung bị chặn thì không (FR-15)

**Given** response rỗng, JSON lỗi, timestamp âm/không hữu hạn/ngoài Chunk, `end < start`, hoặc Segment trùng
**When** chuẩn hoá kết quả
**Then** giữ Segment hợp lệ với `0 ≤ start < end ≤ duration`, sắp theo thời gian và loại bản trùng; chỉ xác nhận silence khi response hợp lệ thể hiện không có lời nói. Response lỗi/rỗng không xác định không được coi là Chunk thành công. Metadata phần chưa cứu được phải đủ cho story 2.5 tạo gap và retry mà không nhân đôi phần đã cứu (FR-13–15).

### Story 2.3: Lưu Phiên và Transcript bền vững

**Phụ thuộc:** 1.2, 2.1

As Minh,
I want Phiên, Transcript và Segment được lưu bền trong Container và không bao giờ ở trạng thái nửa vời,
So that mở lại app vẫn thấy đủ và một Job hỏng không để lại rác.

**Acceptance Criteria:**

**Given** migration mới
**When** app khởi động
**Then** tạo `sessions`, `transcripts`, `segments` đúng AR-43: `sessions.status ∈ {recording, finalizing, complete}`, `recovered`, `source_hash UNIQUE NULL`; `transcripts.variant ∈ {primary, retranscribe}`, `status ∈ {complete, partial}`; `segments.kind ∈ {text, gap}`, `gap_reason ∈ {chunk_failed, disconnected}` NULL, `speaker` NULL
**And** cột chính xác được chốt ở story này và migration chỉ tiến (AD-4, AD-16)

**Given** Job file đang chạy
**When** làm việc
**Then** mọi file trung gian nằm trong `media/.staging/<job-id>/`
**And** một transaction SQLite commit session + transcript + segments + tham chiếu media. File Proxy được ghi hoàn chỉnh rồi rename vào `media/<session-id>/proxy.<ext>` theo giao thức publish có phục hồi; SQLite không bao gồm thao tác filesystem. ADR của story quy định thứ tự publish/commit, rollback và dọn file không được DB tham chiếu sau crash (AD-16, AD-5, AD-19)

**Given** Job bị huỷ hoặc lỗi trước khi commit
**When** dọn dẹp
**Then** Job mới không để lại dòng DB; staging/file đã publish chưa có tham chiếu được dọn hoặc đánh dấu để boot dọn lại. Với Chạy lại/Transcribe lại, dữ liệu đã commit của Phiên cũ giữ nguyên (AD-16)

**Given** Transcript có gap `chunk_failed`
**When** lưu
**Then** `status = partial` khi và chỉ khi có gap `chunk_failed`; `partial` không bao giờ được lưu như `complete` (AD-16, FR-15)

**Given** Chạy lại đã dựng xong Transcript mới
**When** hoàn tất
**Then** Transcript mới thay `primary` một cách nguyên tử và Phiên giữ nguyên `id` (AD-16, FR-11)
**And** Transcript cũ vẫn nguyên vẹn nếu việc dựng bản mới bị huỷ hoặc lỗi

**Given** repo `db/`
**When** feature cần dữ liệu
**Then** SQL chỉ nằm ở `db/repo/<entity>.rs`, mốc thời gian lưu bằng epoch ms UTC và `start/end` là giây `f64` tuyệt đối (AD-4, AD-9)
**And** test kill tại ranh giới publish/commit rồi boot/reconcile xác nhận không còn transaction nửa vời hay file mồ côi trong `media/<session-id>/`

**Given** Proxy encode/publish thất bại nhưng kết quả Transcript đã có
**When** commit
**Then** vẫn commit Transcript với `proxy_missing`, giữ đúng trạng thái complete/partial; báo lỗi audio riêng, không biến lỗi Proxy thành lỗi Transcript (AD-19).

**Given** crash tại ranh giới ghi file, rename, DB commit hoặc dọn file cũ
**When** boot chạy lại nhiều lần
**Then** reconciliation idempotent giữ media đã được DB tham chiếu, dọn file mồ côi và không xoá bản Transcript/Proxy cũ đang có hiệu lực; không có dòng Phiên hiển thị như thành công trong khi commit chưa xong.

### Story 2.4: Job transcribe file — hàng đợi, tiến độ và huỷ

**Phụ thuộc:** 2.2, 2.3

As Minh,
I want thả file 90 phút vào, thấy tiến độ thật, huỷ được và rời màn mà Job vẫn chạy,
So that tôi không phải canh máy.

**Acceptance Criteria:**

**Given** actor `JobRegistry` (mpsc + oneshot, có `is_busy(session_id)`)
**When** nhiều Job được đưa vào
**Then** chỉ có **một** hàng đợi tuần tự cho Transcribe file (và về sau Chạy lại, Transcribe lại); không có bảng `jobs` và Job không sống qua restart (AD-2, AD-11)

**Given** `transcribe_start(path)`
**When** gọi
**Then** tạo Job UUIDv7 và chạy pipeline decode → chunk → Gemini → merge → lưu (2.1–2.3), gửi Chunk tuần tự, Segment không lùi thời gian (FR-13)
**And** chụp model, ngôn ngữ và `chunkMinutes` (mặc định 5 phút) lúc nhận Job; offset chỉ đọc ở UI/export, không tham gia decode, request hay thời gian lưu trữ

**Given** khởi động sau khi Job file bị crash
**When** routine `ipc/boot` chạy trong Epic 2
**Then** sau migrate DB, dọn staging và reconcile media theo story 2.3 trước khi nhận Job mới. Routine này được mở rộng ở 4.10 và 5.4, không tạo boot handler thứ hai; Job cũ không tự chạy lại (AD-18).

**Given** `jobs_subscribe(channel)`
**When** UI subscribe (kể cả sau remount)
**Then** nhận `Snapshot { seq, … }` rồi luồng `JobEvent` (progress, log, waitingQuota, result, error, cancelled) với `seq` đơn điệu; hụt `seq` thì subscribe lại (AD-3)

**Given** Job đang chạy
**When** cập nhật tiến độ
**Then** tính theo phút audio đã xử lý trên tổng ("32 / 90 phút · 36 %"), không theo số Chunk
**And** có dòng chi tiết Chunk hiện tại, số thứ tự key đang dùng (không lộ giá trị key) và số lần thử lại (FR-12, UX-DR30)

**Given** mọi key đang nghỉ
**When** Job chờ quota
**Then** phát `waitingQuota` và UI hiện "đang chờ quota" thay vì báo lỗi (FR-6, FR-12)

**Given** người dùng bấm Huỷ
**When** Job đang chạy
**Then** ngừng gửi Chunk mới trong ≤ 2 s, huỷ cả lúc chờ quota/decode/hash; phản hồi tới muộn không được commit. Cancel thắng trước điểm DB commit; sau commit trả rõ Job đã hoàn tất và giữ Phiên. Job đang chờ được bỏ khỏi hàng đợi, các Job khác tiếp tục; staging được dọn (FR-12)

**Given** Job đang chạy
**When** người dùng rời màn rồi quay lại
**Then** vẫn thấy Job và trạng thái hiện tại; Job không phụ thuộc vòng đời WebView (FR-12)

**Given** người dùng đóng app khi có Job chạy
**When** handler `ipc/close` xử lý
**Then** hiện dialog xác nhận; nếu tiếp tục đóng thì Job huỷ sạch, không chạy nền (FR-12, AD-18, UX-DR16)

**Given** chưa có key hợp lệ hoặc chưa có Consent
**When** gọi `transcribe_start`
**Then** chưa Consent bị chặn trước mọi luồng tạo Job; đã Consent thì hash/tra Existing trước gate key cho Job mới. Không có Phiên trùng và thiếu key → lỗi kèm lối tắt Settings, không tạo Job (FR-2, FR-4, AD-11)

**Given** file mẫu 90 phút và transcript v2 của cùng file bằng cùng model
**When** so sánh sau khi Job xong
**Then** độ lệch timestamp Segment ≤ 2 s và không mất đoạn nào (AR-53)

**Given** `jobs_subscribe` lấy snapshot đúng lúc có event mới
**When** đăng ký Channel
**Then** snapshot và cursor được chụp cùng thao tác đăng ký để không mất event giữa hai bước; UI bỏ event có `seq` đã áp dụng, subscribe lại khi hụt số, dọn subscription cũ khi remount (AD-3).

**Given** Job mới chưa commit Phiên
**When** tạo link tiến độ
**Then** JobRegistry cấp trước `session_id` dự kiến và ánh xạ tới `job_id`; `/session/:id` đọc JobRegistry trước DB. Sau cancel/restart khi cả hai không còn, hiện trạng thái “Tác vụ không còn” và link Home, không giả báo mất dữ liệu Phiên.

### Story 2.5: Chịu lỗi Chunk, Khoảng thiếu và Chạy lại

**Phụ thuộc:** 2.4

As Minh,
I want khi một đoạn thất bại app không giấu mà cho tôi chạy lại đúng phần thiếu,
So that transcript đầy đủ mà không tốn token cho cả file.

**Acceptance Criteria:**

**Given** một Chunk lỗi
**When** thử lại
**Then** tối đa 4 lần gửi thực tế tổng cộng cho một Chunk, gồm lần đầu và các lần đổi key; chỉ một tầng `gemini/` sở hữu bộ đếm. Chờ quota không tính là lần gửi, tổng chờ ≤ 180 s/Chunk. 5xx có thể retry trong ngân sách; 400/404/blocked fail ngay; timeout kết thúc request và không tự gửi lại cùng audio sang key khác (FR-6, FR-15, AR-40)

**Given** Chunk thất bại hẳn
**When** Job kết thúc
**Then** khoảng `[start, end]` của Chunk trở thành gap `chunk_failed` và Transcript có `status = partial`
**And** Job vẫn commit Phiên cùng Transcript có `status = partial` thay vì bỏ toàn bộ; `sessions.status` vẫn là `complete` (AR-30)

**Given** Transcript partial
**When** lưu và hiển thị
**Then** không bao giờ được coi là hoàn chỉnh hay cache như hoàn chỉnh; Phiên mang cờ `partial` cho Home (FR-15)

**Given** `transcribe_rerun(session_id, transcript_id, scope)` với `scope ∈ {missing, all, gap(gap_id)}`
**When** Chạy lại
**Then** dùng Proxy trong Container làm nguồn audio (không cần file nguồn) và đi qua cùng hàng đợi tuần tự (FR-10, AD-11)
**And** `missing` chỉ gửi lại các khoảng thiếu của Transcript đã chọn; `gap(gap_id)` được Rust tra và kiểm tra thuộc đúng Transcript/Phiên, không tin range do UI truyền. Giữ nguyên mọi Segment ngoài vùng retry, hợp nhất phần đã cứu trong vùng mà không trùng text/gap

**Given** Chạy lại xong
**When** hợp nhất
**Then** dựng Transcript mới rồi swap nguyên tử đúng variant đích: `primary` cho Phiên file, `retranscribe` cho bản Transcribe lại của Phiên live. Không bao giờ dùng thao tác này để ghi đè `primary` live; Tag, Ghi chú, Memo và ID Phiên giữ nguyên (FR-11, FR-26, AD-16)
**And** nếu vẫn còn khoảng thiếu thì Transcript mới tiếp tục là partial

**Given** Chạy lại bị huỷ hoặc lỗi
**When** kết thúc
**Then** Transcript đích cũ giữ nguyên. “Lỗi Job” là lỗi chặn hoàn tất như không đọc được nguồn/DB; Job đã xử lý hết audio nhưng còn Chunk fail là **kết quả partial**, được commit đúng trạng thái, không phải lỗi Job (FR-11, FR-15)

**Given** các kịch bản 429 xoay key, Chunk fail hẳn, JSON cắt cụt, key 401 giữa chừng
**When** chạy test với transport giả
**Then** kết quả đúng theo chính sách trên và không kịch bản nào để một Chunk biến mất âm thầm (AR-36, SM-C2)

**Given** Phiên đã có Job đang chạy hoặc đang chờ cho một Transcript
**When** bấm Chạy lại lần nữa hoặc cùng lúc ở hai cửa sổ thao tác UI
**Then** trả Job hiện có hoặc lỗi busy rõ ràng; không cho kết quả cũ tới muộn ghi đè lần chạy mới. Phiên live chỉ có gap `disconnected` dùng “Transcribe lại” story 4.11, không retry vào bản live (AD-11, AD-16).

### Story 2.6: Settings — Chunking, offset và ngôn ngữ transcribe

**Phụ thuộc:** 1.9

As Minh,
I want chỉnh độ dài Chunk, offset timestamp và ngôn ngữ transcribe,
So that phù hợp với file và cách họp của tôi.

**Acceptance Criteria:**

**Given** nhóm Chunking (xuất hiện ở story này)
**When** người dùng nhập số phút Chunk
**Then** mặc định 5, tối thiểu 1; giá trị nhỏ hơn 1 hoặc không phải số bị chặn tại chỗ kèm giải thích (FR-13, FR-39)

**Given** trường offset timestamp toàn cục (giây)
**When** người dùng đổi
**Then** offset chỉ áp dụng lúc hiển thị và export, không sửa dữ liệu Segment đã lưu (FR-13, AD-9)

**Given** nhóm Gemini
**When** thêm trường ngôn ngữ transcribe
**Then** có 4 lựa chọn auto/ja/vi/en với mặc định auto, áp dụng cho Transcribe file và Live (FR-16)
**And** `auto` không ép ngôn ngữ trong prompt hay `language_codes`

**Given** mọi trường mới
**When** hiển thị
**Then** có icon (?) tooltip và helper text bền (FR-39, UX-DR11)

**Given** một Job đang chạy
**When** người dùng đổi các giá trị này
**Then** model/ngôn ngữ/chunkMinutes của Job đó giữ nguyên snapshot lúc nhận Job; Job mới dùng cấu hình mới. Offset là tuỳ chọn hiển thị/export nên đổi tức thì cho cả Phiên đang xem, không chạy lại Gemini

### Story 2.7: Transcript detail — hiển thị, trình phát và click-to-seek

**Phụ thuộc:** 2.4, 2.5

As Minh,
I want xem transcript, nghe lại đúng đoạn và thấy rõ chỗ nào bị thiếu,
So that tôi kiểm chứng được nội dung và biết cần chạy lại phần nào.

**Acceptance Criteria:**

**Given** route `/session/:id`
**When** mở một Phiên
**Then** có header (nút quay lại, tên phiên, meta gồm badge loại, ngày, thời lượng, số Segment) và danh sách Segment dạng grid `56px | 1fr` (UX-DR14, UX-DR31)
**And** timestamp là mono tabular `HH:MM:SS` (bỏ giờ nếu < 1 h) đã cộng offset toàn cục; speaker không hiển thị (FR-34)

**Given** bộ badge memo, audio, partial, recover, live, file, token
**When** dựng component
**Then** đúng cặp nền/chữ theo DESIGN.md, `min-width` thay `width` cố định; badge memo/audio chỉ hiện ở detail (UX-DR12)

**Given** trình phát 64 px dùng Proxy trong Container
**When** người dùng phát, tạm dừng, click Segment hoặc kéo thanh seek
**Then** click Segment nhảy tới `start` gốc chưa cộng offset; highlight cũng so với thời gian media gốc. Segment đang phát có nền accent-soft và `aria-current`, seek phản hồi ≤ 500 ms với file 90 phút (FR-32, UX-DR21)
**And** thanh seek là `role=slider` nhảy ±5 s bằng ←/→, Space play/pause khi focus ở transcript, có chỉnh tốc độ và âm lượng, không tự phát audio

**Given** danh sách đang phát
**When** người dùng cuộn tay
**Then** tự cuộn theo Segment đang phát dừng ngay và hiện nút "Xuống dòng đang phát" để quay lại (UX-DR32)

**Given** Proxy hỏng hoặc thiếu
**When** mở Phiên
**Then** Transcript vẫn mở và tìm được; thanh phát hiện "Không có audio · Chọn lại file nguồn"
**And** Phiên file chỉ nhận nguồn khớp `source_hash`; sai hash báo tại chỗ và giữ Transcript. Với Phiên live không có source_hash, ưu tiên tái tạo Proxy từ Recording trong Container; Recording cũng mất/hỏng thì báo rõ, không ghép file bất kỳ vào transcript cũ (FR-10)

**Given** Transcript partial
**When** mở Phiên
**Then** có banner warning liệt kê từng khoảng thiếu (mm:ss–mm:ss) với nút "Chạy lại phần thiếu" và "Chạy lại toàn bộ", mỗi nút kèm badge "Tốn token Gemini" (FR-15, UX-DR12, UX-DR43)
**And** mỗi khoảng thiếu là một dòng nền warning nằm đúng vị trí thời gian trong luồng Segment kèm nút "Chạy lại khoảng này"

**Given** Job đang chạy cho Phiên chưa commit
**When** người dùng ở `/session/:id`
**Then** màn hiện tiến độ theo phút audio, panel log chi tiết mở được, nút Huỷ, và tự chuyển sang hiển thị Phiên khi Job commit (FR-8, FR-12)

**Given** cửa sổ 1024–1279 px
**When** hiển thị detail
**Then** chỗ dành cho panel phụ 360 px đóng trước và Transcript nhận phần dư (UX-DR23)

### Story 2.8: Nhận file — dialog, kéo thả, kiểm tra định dạng, phát hiện trùng

**Phụ thuộc:** 2.7

As Minh,
I want kéo file vào bất kỳ đâu hoặc chọn bằng dialog, được báo ngay nếu định dạng không hỗ trợ và không tốn token khi file đã transcribe,
So that tôi bắt đầu nhanh và không mất tiền oan.

**Acceptance Criteria:**

**Given** nút "Chọn file" ở header Home
**When** bấm
**Then** mở dialog hệ thống liệt kê định dạng hỗ trợ (mp3, m4a, wav, flac, ogg, aiff, caf, mp4, mov, mkv, webm) (FR-8, FR-9)

**Given** người dùng kéo file trên cửa sổ
**When** thả vào **bất kỳ đâu** trên Home hoặc Transcript detail
**Then** file được nhận, vùng drop-zone hiện trạng thái active (viền `accent-border`, nền `accent-soft`) và app chuyển sang màn Transcript của Job vừa tạo (FR-8, UX-DR15, UX-DR28)

**Given** nhiều file được thả cùng lúc
**When** app nhận
**Then** xếp hàng tuần tự, mỗi file một Phiên, số file đang chờ hiện trên card job (FR-8, AD-11)

**Given** file `avi`, `wmv`, `flv` hoặc `ts`
**When** thả hoặc chọn
**Then** hiện banner category "Định dạng" ngay tại chỗ với câu "Định dạng không hỗ trợ. Hãy chuyển sang mp4, m4a hoặc mp3." trước khi tạo Phiên, không lỗi im lặng (FR-9, UX-DR41)

**Given** app chạy trong sandbox
**When** nhận file qua dialog hoặc kéo thả
**Then** chỉ cần quyền đọc file do người dùng chọn trong phiên chạy và không xin thêm quyền nào (FR-8, NFR-7)

**Given** file có `source_hash` trùng một Phiên đã có
**When** `transcribe_start` kiểm tra nguồn và hash trước khi xin key/tạo Job mới
**Then** trả `Existing { session_id }`, app mở Phiên có sẵn, không tạo Job và không gọi Gemini
**And** UI nói rõ "Mở lại phiên có sẵn — không tốn token" (FR-11, AD-11, UX-DR43)

**Given** chưa có key hợp lệ
**When** người dùng bấm "Chọn file" hoặc thả file
**Then** nút khởi tạo transcribe bị vô hiệu kèm tooltip/lối tắt Settings; nếu vẫn thả file thì probe/hash chỉ ở máy: file trùng mở Phiên có sẵn, file mới báo thiếu key và không tạo Job. Danh sách/phát lại luôn dùng được khi đã Consent (FR-4, FR-11)

**Given** cùng nội dung đã nằm trong hàng đợi hoặc đang xử lý nhưng chưa có Phiên trong DB
**When** chọn/thả lại, kể cả hai yêu cầu đồng thời
**Then** JobRegistry giữ reservation theo source_hash và trả Job hiện có; không gửi Gemini hai lần rồi mới chờ UNIQUE constraint. Reservation được giải phóng khi Job huỷ/lỗi (FR-11, AD-11).

**Given** batch gồm file hợp lệ, file sai định dạng, trùng file, file rỗng hoặc video không có audio
**When** kiểm tra từng file
**Then** nêu kết quả riêng; file lỗi không tạo Phiên và không chặn file hợp lệ. Giữ quyền đọc/handle cho file đang chờ tới lúc xử lý; nguồn bị đổi nội dung hoặc biến mất trước decode phải báo lỗi, không lưu hash khác với audio đã gửi.

### Story 2.9: Home — danh sách phiên và card job

**Phụ thuộc:** 2.8

As Minh,
I want thấy mọi phiên của mình và Job đang chạy ngay ở Trang chủ,
So that tôi quay lại đúng chỗ, kể cả khi vừa rời màn giữa chừng.

**Acceptance Criteria:**

**Given** Trang chủ chưa có phiên
**When** hiển thị
**Then** có card "Kéo file vào đây / Chọn file" kèm danh sách định dạng hỗ trợ (card "Bắt đầu Live" được thêm ở Epic 4) (UX-DR28)

**Given** Trang chủ đã có phiên
**When** hiển thị
**Then** drop-zone mỏng nằm trên danh sách; mỗi dòng gồm tên, ngày theo múi giờ cục bộ, badge loại LIVE/FILE, thời lượng, badge "Thiếu N khoảng" và "Phục hồi" đứng ngay sau tên khi có; **không** hiển thị Model, số Segment hay cột Trạng thái (FR-27, UX-DR29, UX-DR12)
**And** danh sách sắp mới nhất trước, click dòng mở `/session/:id`, dùng virtual list và không infinite scroll

**Given** 500 phiên mẫu
**When** mở Trang chủ
**Then** danh sách hiển thị ≤ 1 s và app mở tới Trang chủ ≤ 2 s; trong lúc tải, khung sidebar và header hiện ngay còn vùng danh sách là skeleton 6 dòng khớp bố cục thật (FR-27, NFR-5, UX-DR28)

**Given** một Job đang chạy hoặc đang chờ
**When** người dùng ở Trang chủ
**Then** card job kiểu warning hiện ở đầu danh sách với tên file, tiến độ "32 / 90 phút · 36 %", dòng chi tiết Chunk/key/lần thử, số file đang chờ, nút "Mở" và "Huỷ" (FR-27, UX-DR30, UX-DR15)
**And** trạng thái "đang chờ quota" hiện khi mọi key đang nghỉ

**Given** một Job đang chạy
**When** người dùng chuyển sang màn khác
**Then** card job thu gọn ở sidebar vẫn hiện và click quay lại được Job (FR-27, UX-DR30, UX-DR24)

**Given** Job hoàn tất
**When** Phiên được commit
**Then** Phiên xuất hiện đầu danh sách mà không cần tải lại và card job biến mất

**Given** phiên có transcript partial
**When** hiển thị ở Trang chủ
**Then** dòng phiên có badge "Thiếu N khoảng" cho tới khi transcript đầy đủ (FR-15)

### Story 2.10: Tìm trong transcript, export và copy

**Phụ thuộc:** 2.7

As Linh tìm lại câu khách nói ba tuần trước,
I want tìm trong transcript, nhảy tới đúng đoạn và export gửi cho người khác,
So that tìm đúng câu trong 30 giây thay vì tua cả recording.

**Acceptance Criteria:**

**Given** một Transcript
**When** người dùng bấm `⌘F`/`Ctrl+F` và gõ từ khoá
**Then** hiện đếm `n/N`, highlight bằng màu `mark`, cuộn tới match; không phân biệt hoa thường và bỏ khoảng trắng thừa (FR-33, UX-DR33, UX-DR44)
**And** với Transcript ~700 Segment phản hồi ≤ 100 ms

**Given** danh sách kết quả tìm
**When** người dùng bấm Enter, Shift+Enter hoặc nút prev/next
**Then** chuyển match kế/trước theo vòng tròn (FR-33)

**Given** một Transcript
**When** người dùng chọn Export `.txt`, `.srt` hoặc `.json`
**Then** mở dialog lưu hệ thống và file áp dụng offset timestamp toàn cục; `.json` giữ speaker (FR-35)
**And** test khoá định dạng SRT (chỉ số, dòng thời gian `HH:MM:SS,mmm`) và nội dung `.txt`

**Given** Transcript partial
**When** export `.txt`
**Then** có ghi chú các Khoảng thiếu (FR-35)

**Given** nút Copy
**When** bấm
**Then** toàn bộ Transcript được copy vào clipboard và hiện toast "Đã copy" tự tắt sau 4 s với `aria-live=polite` (FR-35, UX-DR18)

**Given** Phiên có nhiều Transcript (về sau)
**When** export hoặc copy
**Then** lấy Transcript đang được chọn (FR-34)

**Given** thao tác tìm và export
**When** dùng bàn phím
**Then** thực hiện được đầy đủ không cần chuột (NFR-11)

**Given** query rỗng hoặc không có match
**When** tìm kiếm
**Then** hiện 0/0, vô hiệu prev/next và không cuộn; Enter trong ô ghi chú/tên phiên không kích hoạt tìm kiếm (UX-DR44).

**Given** offset làm timestamp hiển thị/export âm
**When** áp dụng
**Then** dùng cùng quy tắc `max(0, t + offset)` cho UI/export; SRT bỏ cue có `end ≤ start` sau chuẩn hoá và đánh lại số thứ tự. `.json` giữ metadata offset và các gap; `.txt`/Copy ghi chú gap; `.srt` chỉ chứa cue text hợp lệ và UI báo riêng rằng có khoảng thiếu. Đây là quy tắc chuẩn hoá export, không sửa thời gian trong DB.

<a id="epic-3"></a>

## Epic 3: Thư viện phiên, Ghi chú & Memo

Người dùng tìm lại phiên cũ theo tên và tag (UJ-4), đổi tên inline, xoá sạch dữ liệu, xem dung lượng lưu trữ và xoá toàn bộ; gõ ghi chú tự lưu; sinh, sinh lại và copy memo/議事録 theo template của mình, quản lý template trong Settings (UJ-3 cao trào).

### Story 3.1: Đổi tên và xoá phiên

**Phụ thuộc:** 2.9

As Linh,
I want đổi tên phiên ngay tại chỗ và xoá sạch phiên không còn cần,
So that thư viện gọn và không còn file mồ côi chiếm ổ đĩa.

**Acceptance Criteria:**

**Given** một dòng phiên ở Trang chủ hoặc header Transcript detail
**When** người dùng chọn Đổi tên trong menu ⋯ hoặc click vào tên
**Then** tên chuyển thành ô nhập inline; Enter lưu, Esc huỷ; tên rỗng bị từ chối, tên dài nhất 200 ký tự (FR-30, UX-DR29)
**And** menu ⋯ của dòng phiên có mục Đổi tên và Xoá, tới được bằng bàn phím và mở/đóng bằng Esc

**Given** người dùng chọn Xoá một Phiên
**When** dialog xác nhận (một trong 5 trường hợp được phép) hiện ra và người dùng đồng ý
**Then** toàn bộ dữ liệu liên quan (Transcript, Segment, Proxy, Recording, Memo, Ghi chú, liên kết Tag) bị xoá khỏi Container (FR-30, UX-DR16)
**And** dung lượng Container giảm tương ứng và test xác nhận không còn file mồ côi trong `media/<session-id>/`

**Given** Phiên đang có Job hoặc đang được ghi Live
**When** người dùng cố xoá
**Then** thao tác bị chặn bằng lời giải thích tại chỗ cho tới khi Job xong hoặc huỷ; việc hỏi `JobRegistry`/`LiveSession` được điều phối trong `ipc/` (FR-27, FR-30, AD-1)

**Given** Tag chưa gắn cho Phiên nào sau khi Phiên bị xoá
**When** Tag đó tồn tại
**Then** Tag vẫn tồn tại cho tới khi người dùng chủ động xoá Tag (FR-30)

**Given** dữ liệu đã xoá
**When** mở lại app
**Then** Phiên không xuất hiện lại và không có lỗi mở file thiếu

**Given** xoá Phiên chạy đồng thời với request Memo, export, tạo Proxy hoặc Job mới
**When** IPC kiểm tra busy và bắt đầu xoá
**Then** giữ quyền thao tác độc quyền theo session tới khi xong; huỷ/chặn writer đang chạy và từ chối kết quả tới muộn. Nếu xoá file/DB thất bại, báo category storage, giữ trạng thái để retry/boot dọn tiếp idempotent; không báo “Đã xoá” khi dữ liệu vẫn còn (FR-30, AD-1).

### Story 3.2: Tag — gắn, lọc và quản lý

**Phụ thuộc:** 3.1

As Linh,
I want gắn tag cho phiên rồi lọc theo tag để tìm lại theo khách hàng hay sprint,
So that thư viện 40 phiên vẫn tìm nhanh.

**Acceptance Criteria:**

**Given** migration mới
**When** app khởi động
**Then** tạo `tags` và `session_tags(session_id, tag_id)`; lưu tên hiển thị cùng khoá chuẩn hoá Unicode (trim + case-fold) có UNIQUE constraint. Không chỉ dựa vào `COLLATE NOCASE` để đáp ứng vi/en/ja; kiểm thử “DỰ ÁN”/“dự án”, chuỗi chỉ khoảng trắng và hai lần tạo đồng thời (FR-29; AR-43 là schema phác thảo)

**Given** popover Tag picker (component dùng chung, rộng 320 px)
**When** mở từ chip "+ Tag" ở Transcript detail, mục Gắn tag ở menu ⋯ của dòng phiên hoặc chip "+ N tag khác"
**Then** có ô tìm kiêm tạo mới và danh sách tag sắp theo số phiên; mode lọc Home dùng nhóm "Đang lọc", mode gắn cho Phiên/LiveSetup dùng "Đã chọn". Gắn Tag không tự đổi bộ lọc Home; link "Quản lý tag" dùng chung
**And** khi chưa có tag nào hiện gợi ý "Gõ để tạo tag đầu tiên" (FR-29, UX-DR19)

**Given** người dùng nhập tag
**When** lưu
**Then** chuẩn hoá trim và bỏ trùng không phân biệt hoa thường; tối đa 20 tag mỗi Phiên và 80 ký tự mỗi tag, vượt thì bị chặn tại chỗ có giải thích (FR-29)
**And** Tag tồn tại qua đổi tên, Chạy lại và Transcribe lại

**Given** hàng chip tag ở Trang chủ
**When** hiển thị
**Then** nằm trên **một hàng, không wrap**: tag đang lọc (có x) → 3–5 tag dùng nhiều nhất → chip "+ N tag khác"; chip "Chưa gắn tag" tách bằng vạch dọc (FR-29, UX-DR20)

**Given** người dùng chọn nhiều tag
**When** lọc danh sách
**Then** kết quả là AND của các tag; “Chưa gắn tag” loại trừ lựa chọn tag cụ thể: bật nó xoá các tag lọc, chọn tag cụ thể tắt nó. Query tên vẫn kết hợp AND và được giữ nguyên (FR-29)

**Given** chế độ Quản lý tag
**When** người dùng xoá một tag toàn cục
**Then** dialog xác nhận hiện ra và, khi đồng ý, tag bị gỡ khỏi mọi Phiên trong một thao tác (FR-29, UX-DR16)

**Given** popover đóng bằng Esc hoặc click ngoài
**When** đóng
**Then** focus trở về nút đã mở nó và mọi thao tác dùng được bằng bàn phím (UX-DR44)

### Story 3.3: Tìm phiên theo tên kết hợp lọc tag

**Phụ thuộc:** 3.2

As Linh,
I want gõ tên phiên để lọc danh sách ngay khi gõ, kết hợp với lọc tag,
So that thu hẹp về đúng phiên trong vài giây.

**Acceptance Criteria:**

**Given** ô tìm ở header Trang chủ
**When** người dùng gõ
**Then** danh sách cập nhật khi gõ, khớp không phân biệt hoa thường và bỏ khoảng trắng thừa, phản hồi ≤ 200 ms với 500 phiên (FR-28, UX-DR33)
**And** có nút xoá query

**Given** query và bộ lọc tag cùng bật
**When** danh sách lọc
**Then** kết quả là AND của cả hai; xoá query giữ nguyên bộ lọc tag (FR-28)

**Given** không có kết quả
**When** hiển thị
**Then** có trạng thái rỗng kèm nút "Xoá bộ lọc" (FR-28, UX-DR28)

**Given** danh sách đang lọc
**When** hiển thị footer
**Then** luôn có dòng dạng "6 / 38 phiên · lọc…" (UX-DR28)

**Given** phím tắt tìm
**When** người dùng bấm `⌘F`/`Ctrl+F` ở Trang chủ
**Then** focus vào ô tìm phiên (UX-DR44)

### Story 3.4: Settings — Lưu trữ trong Container

**Phụ thuộc:** 3.1

As Minh,
I want biết app đang chiếm bao nhiêu dung lượng và xoá sạch dữ liệu họp khi cần,
So that tôi kiểm soát dữ liệu trên máy công ty.

**Acceptance Criteria:**

**Given** nhóm Lưu trữ (xuất hiện ở story này)
**When** mở
**Then** hiển thị thanh dung lượng Media và DB, số phiên, và nút "Mở thư mục" khi hệ điều hành cho phép (FR-40, UX-DR39)
**And** không có tuỳ chọn thư mục cache (Q7)

**Given** nút "Xoá toàn bộ dữ liệu…"
**When** bấm
**Then** dialog xác nhận **hai bước** hiện ra, nút xác nhận cuối dùng `button-danger-soft` (FR-40, UX-DR16)
**And** khi đồng ý, mọi Phiên, Transcript, Proxy, Recording, Ghi chú, Memo và Tag bị xoá; số Phiên và dung lượng media về 0; DB đo lại thực tế (có thể còn settings/schema/WAL theo phạm vi đã chốt), không báo sai rằng DB phải bằng 0; không còn file media mồ côi

**Given** hành vi của "Xoá toàn bộ dữ liệu"
**When** thực hiện
**Then** trước khi triển khai phải chốt OQ9: đề xuất xoá dữ liệu họp, giữ settings/key/Consent/Template; dialog phải liệt kê chính xác phần xoá và phần giữ. Chưa có quyết định thì chưa nghiệm thu hành vi xoá toàn bộ, không mặc nhiên coi đề xuất này đã được duyệt

**Given** đang có Live hoặc Job
**When** người dùng bấm xoá toàn bộ
**Then** bị chặn kèm lời giải thích (AD-1)

**Given** app không tự dọn Phiên cũ
**When** dung lượng tăng theo thời gian
**Then** không có tác vụ nền nào tự xoá dữ liệu người dùng (FR-40)

**Given** xoá toàn bộ đang thực hiện
**When** có yêu cầu tạo Job/Live/Memo hoặc ghi dữ liệu mới
**Then** chặn writer mới tại Rust tới khi xoá xong; nếu lỗi giữa chừng báo rõ và cho retry idempotent, không để callback cũ tái tạo dữ liệu đã xoá. Hai bước xác nhận ở cùng một dialog, không chồng modal (AD-1, UX-DR16).

### Story 3.5: Ghi chú tự lưu

**Phụ thuộc:** 2.7

As Linh đang họp,
I want gõ ghi chú vào panel, biết khi nào đã lưu bền và phục hồi được phần đã lưu sau khi app đóng đột ngột,
So that ghi chú của tôi có sẵn để nhúng vào memo.

**Acceptance Criteria:**

**Given** migration mới
**When** app khởi động
**Then** tạo `notes(session_id PK, body, updated_at)` (AR-43)

**Given** panel Ghi chú (tab trong panel phụ 360 px của Transcript detail)
**When** người dùng gõ
**Then** tự lưu sau debounce ~800 ms qua `notes_save`, hiện chỉ báo "Đã lưu hh:mm" (FR-36, UX-DR35)

**Given** người dùng rời màn, đóng panel hoặc đóng app ngay sau khi gõ
**When** chưa tới hạn debounce
**Then** nội dung chờ được flush trước khi rời đi và không bị mất (FR-36)

**Given** component panel Ghi chú
**When** dùng ở Live (Epic 4)
**Then** dùng lại được nguyên vẹn cho một Phiên đang ghi, với hợp đồng "lưu bền trước khi Phiên finalize" (FR-36, UX-DR35)

**Given** một Phiên có ghi chú
**When** Phiên bị Chạy lại hoặc Transcribe lại
**Then** ghi chú giữ nguyên (FR-11, FR-26)

**Given** nội dung ghi chú
**When** ghi log hay chẩn đoán
**Then** bọc `Sensitive<T>` và không xuất hiện trong log (AR-29)

**Given** ghi chú thay đổi nhanh, lưu lỗi hoặc callback trả về sai thứ tự
**When** `notes_save` hoàn tất
**Then** dùng revision tăng dần theo Phiên, không cho bản cũ ghi đè bản mới; chỉ hiện “Đã lưu” sau ACK commit bền, lỗi giữ buffer và hiện “Chưa lưu” + Thử lại. Flush thất bại khi đóng bình thường phải cho người dùng giữ app mở để xử lý.

**Given** force-quit trong khoảng debounce hoặc trước ACK
**When** phục hồi
**Then** test phân biệt bản đã commit với ký tự chưa được lưu; debounce + flush lúc finalize không đủ chứng minh FR-36. OQ10 phải chốt cơ chế journal/ghi bền và cửa sổ mất dữ liệu được chấp nhận trước khi story được nghiệm thu; không tuyên bố “không mất mọi ký tự” từ test chỉ đóng app bình thường.

### Story 3.6: Template memo — quản lý trong Settings

**Phụ thuộc:** 1.9

As Linh,
I want tạo và sửa template memo theo cách công ty tôi viết 議事録,
So that memo sinh ra đúng mẫu ngay lần đầu.

**Acceptance Criteria:**

**Given** migration mới
**When** app khởi động
**Then** tạo `memo_templates(id, name, prompt, is_default)` và nạp bộ Template mặc định theo ngôn ngữ UI (vi/en/ja) (AR-43, FR-37)
**And** nội dung mặc định (lấy từ v2 hay viết lại — Open Question 8) được chốt và ghi lại ở story này

**Given** nhóm Memo trong Settings
**When** mở
**Then** hiển thị master-detail: danh sách mẫu bên trái với badge "Mặc định · vi" hoặc "Của bạn", nút "+ Thêm mẫu"; bên phải là editor tên và textarea prompt mono (FR-37, UX-DR39)
**And** đây là nơi duy nhất thêm, sửa, xoá Template memo

**Given** editor
**When** người dùng gõ
**Then** hàng kiểm tra `{transcript}` (bắt buộc) và `{notes}` (tuỳ chọn) cập nhật ngay; thiếu `{transcript}` thì nút Lưu vô hiệu và dòng kiểm tra chuyển đỏ kèm lời giải thích (FR-37)

**Given** Template mặc định
**When** người dùng chọn Xoá mẫu
**Then** nút bị vô hiệu; xoá mẫu của người dùng cần xác nhận inline ngay trong editor (không thêm dialog thứ sáu). Memo đã sinh vẫn giữ nội dung và tên/prompt snapshot dù Template bị xoá (FR-37–38, UX-DR16)

**Given** "Khôi phục mẫu mặc định"
**When** bấm
**Then** chỉ ghi lại các mẫu mặc định của ngôn ngữ UI hiện tại và **không** xoá mẫu do người dùng tạo (FR-37)

**Given** tên và id Template
**When** lưu
**Then** id do app tự sinh (UUIDv7) và tên không bao giờ dùng làm thành phần đường dẫn (NFR-12)

**Given** đổi ngôn ngữ UI hoặc khôi phục bộ mẫu mặc định
**When** nạp lại Template
**Then** mỗi mẫu có định danh ổn định và locale/origin; đổi locale không tự ghi đè mẫu người dùng đã sửa. Khôi phục chỉ ghi lại bộ mặc định được chọn, không nhân bản mẫu sau mỗi lần mở Settings (FR-37, FR-47).

### Story 3.7: Sinh, cache và sinh lại Memo

**Phụ thuộc:** 3.5, 3.6, 2.10

As Minh,
I want chọn template rồi sinh memo từ transcript và ghi chú, copy dán vào email cho khách,
So that từ file thô tới memo gửi được trong một buổi chiều.

**Acceptance Criteria:**

**Given** migration mới
**When** app khởi động
**Then** tạo `memos` với khoá cache `(session_id, template_id)`, body và thời điểm; thêm provenance: transcript_id/revision, notes revision nếu dùng, Template revision/snapshot và model đã dùng. AR-43 là phác thảo; các cột này cần để xác định “Memo sinh từ bản trước” (FR-38)

**Given** panel Memo trong tab của panel phụ 360 px
**When** hiển thị
**Then** có select Template, nút "Sinh / Sinh lại" kèm badge "Tốn token Gemini" và dòng nguồn "Sinh từ bản X + ghi chú · giờ" (FR-38, UX-DR34, UX-DR43)

**Given** người dùng bấm Sinh
**When** `memo_generate` chạy
**Then** đi qua cổng `gemini/` với lớp `Memo` và timeout 90 s; prompt thay `{transcript}` bằng Transcript đang chọn và, khi Template có `{notes}`, nhúng Ghi chú với header theo ngôn ngữ UI (FR-38, AR-40)
**And** Memo được lưu theo Phiên và Template, mở lại không gọi lại Gemini

**Given** Memo đã có
**When** người dùng bấm Sinh lại
**Then** chỉ khi request mới thành công và commit, Memo mới thay bản cũ cho cặp (Phiên, Template). Lỗi/huỷ giữ Memo cũ; request trùng được từ chối hoặc trả tác vụ hiện có, kết quả tới muộn không ghi đè request mới (FR-38)

**Given** Memo là Markdown
**When** render
**Then** dùng `marked` rồi `DOMPurify` để sanitize, link mở trình duyệt ngoài, không chạy script (FR-38, §6.4)
**And** có nút Copy và Tải `.md` qua dialog lưu; xong thì hiện toast nếu người dùng đã rời panel (UX-DR18)

**Given** lỗi quota, auth hoặc mạng khi sinh Memo
**When** hiển thị
**Then** lỗi hiện inline trong panel theo đúng category kèm hành động, không đụng tới Transcript hay Ghi chú (FR-38, AD-19, NFR-2)

**Given** Transcript của Phiên được Chạy lại hoặc Transcribe lại
**When** người dùng mở lại Memo
**Then** Memo cũ vẫn hiện kèm nhãn "Memo sinh từ bản trước" `[ASSUMPTION]` (FR-38, UX-DR34)

**Given** Phiên có Memo
**When** mở Transcript detail
**Then** hiện badge memo trong meta (UX-DR12)

**Given** request Memo đang chạy hoặc người dùng sửa Transcript/Ghi chú/Template sau khi bắt đầu
**When** xem tiến độ, bấm Huỷ hoặc nhận kết quả
**Then** có trạng thái đang sinh, timeout và cancel; chụp đầu vào lúc bắt đầu, lưu provenance đúng snapshot. Thay đổi đầu vào chỉ làm Memo cũ/stale, không tự gọi Gemini. Không dùng lại cache của bản live như thể đã sinh từ bản retranscribe (NFR-2, NFR-8, FR-38).

**Given** chưa Consent/chưa có key, Transcript không có text, hoặc chỉ có gap
**When** muốn sinh Memo
**Then** vô hiệu kèm lý do; Transcript partial có text vẫn dùng được nhưng prompt và dòng nguồn chỉ rõ các khoảng thiếu, không trình bày Memo như dựa trên bản đầy đủ.

<a id="epic-4"></a>

## Epic 4: Live transcribe bền vững

Người dùng họp online với transcript realtime từ nguồn system/mic/mixed; kết nối tự nối lại trong suốt và Recording không bao giờ đứt vì mạng; force-quit thì phiên vẫn được phục hồi; Dừng xong vào thẳng Transcript detail, Transcribe lại từ Recording (xem cạnh bản live) và tải Recording WAV/FLAC.

### Story 4.1: Nền capture audio — nguồn, mic, trộn và gate mềm

**Phụ thuộc:** 1.2

As Linh,
I want chọn nguồn âm thanh (hệ thống, mic hoặc cả hai) và đổi giữa buổi họp mà không ngắt,
So that tôi phát biểu bằng mic rồi quay lại thu cả cuộc họp mà phiên vẫn liền mạch.

**Acceptance Criteria:**

**Given** `audio/capture` như một port (trait) có bản giả cho test
**When** định nghĩa nguồn
**Then** hỗ trợ ba loại `system`, `mic:<tên>`, `mixed:<mic>` và mọi nguồn cho ra PCM16 mono 16 kHz theo chunk 100 ms (1600 mẫu) (AR-38, AR-42, AD-2)

**Given** `live_sources`
**When** gọi
**Then** trả danh sách mic với mic mặc định được chọn sẵn, hỗ trợ làm mới danh sách, và cho biết nguồn `system` có khả dụng trên OS hiện tại hay không (FR-17)

**Given** capture mic bằng cpal 0.18 (port từ v2)
**When** mở thiết bị
**Then** thu được PCM và mọi lỗi thiết bị hay quyền micro trả `AppError` category `permission` với hướng dẫn (AR-4, FR-17)

**Given** nguồn `mixed`
**When** đang chạy
**Then** tín hiệu system và mic được cộng và người dùng đổi giữa `system`, `mic`, `mixed` bằng gate mềm (thiết bị vẫn mở, nguồn bị tắt góp silence) (AR-42)
**And** hiệu lực của đổi nguồn ≤ 1 s và đồng hồ không bị gián đoạn (FR-17)

**Given** đồng hồ Phiên live
**When** tính thời gian
**Then** bằng số sample đã capture chia sample rate, không dùng wall clock (AD-9)

**Given** test với nguồn giả
**When** đổi nguồn liên tục
**Then** không mất và không lặp sample, đồng hồ đơn điệu tăng

**Given** đổi sang nguồn/thiết bị chưa mở hoặc chưa được cấp quyền
**When** `live_set_source` chạy
**Then** chuẩn bị nguồn mới trước rồi swap; thất bại giữ nguồn cũ và báo tại chỗ. Gate mềm chỉ áp dụng với nguồn đã mở và có quyền; không mở mic khi người dùng chỉ chọn system. Trộn nguồn khác sample rate có resample, đồng bộ sample và giới hạn biên độ; đồng hồ dựa trên output mix, không cộng gấp đôi số sample (FR-17, AD-9).

### Story 4.2: Capture âm thanh hệ thống trên macOS (Core Audio tap, spike S4)

**Phụ thuộc:** 4.1, 1.11

As Linh dùng MacBook họp Teams,
I want app thu được âm thanh cuộc họp mà chỉ xin quyền "System Audio Recording" chứ không phải Screen Recording,
So that tôi yên tâm cấp quyền và được duyệt trên Mac App Store.

**Acceptance Criteria:**

**Given** macOS ≥ 14.4
**When** tạo Core Audio process tap **global** (`CATapDescription`, `AudioHardwareCreateProcessTap` qua `objc2`/`coreaudio-sys`) loại trừ PID chính app
**Then** thu được âm thanh hệ thống và âm thanh do chính app phát (TTS) không bị thu lại (AR-42, FR-17)
**And** app chỉ xin "System Audio Recording" và Micro, không bao giờ xin Screen Recording, và không dùng ScreenCaptureKit (FR-17, AD-17, NFR-7)

**Given** build chạy trong sandbox của TestFlight (story 1.11)
**When** thu Zoom, Teams native và Google Meet trong Chrome
**Then** thu được cả ba trên ≥ 3 máy (Intel, Apple Silicon với 14.4 và 15.x) và kết quả ghi thành ADR S4 (AR-10)

**Given** chưa có quyền System Audio Recording
**When** người dùng bắt đầu ghi
**Then** app trả `AppError` category `permission` kèm liên kết mở đúng trang System Settings, không crash, và Phiên chưa bắt đầu cho tới khi có quyền hoặc người dùng chọn nguồn khác (FR-17)
**And** lần đầu chưa biết quyền phải cho phép thao tác mở tap để OS hiện prompt; chỉ kết luận từ chối từ kết quả/quyền do OS cung cấp. Silence đơn thuần là audio hợp lệ, không được suy ra thiếu quyền; cơ chế thử lại sau khi quay về từ Settings được ghi trong ADR

**Given** binary phải được ký
**When** chạy dev build
**Then** ký bằng cert của team `B2U85XPU55` để TCC không reset quyền mỗi lần build, và cách làm được ghi vào hướng dẫn phát triển (AR-42)

**Given** nguồn `mixed`
**When** dùng tap cùng mic (cpal)
**Then** hai luồng được cộng đúng và không xung đột thiết bị

### Story 4.3: Capture âm thanh hệ thống trên Windows (WASAPI loopback kép)

**Phụ thuộc:** 4.1, 1.12

As Minh họp Zoom trên Windows và họp Meet trong trình duyệt,
I want app thu được cả họp bằng app native lẫn họp trong trình duyệt,
So that transcript đủ bất kể tôi họp bằng gì.

**Acceptance Criteria:**

**Given** Windows 10 1809+ hoặc 11 (x64, và Arm64 nếu có)
**When** mở WASAPI loopback (crate `wasapi`) cho **cả** endpoint console lẫn endpoint communications rồi cộng hai luồng
**Then** thu được Zoom/Teams native (communications) và Google Meet trong Chrome (console) (AR-42, FR-17)
**And** khi hai endpoint là cùng một thiết bị thì chỉ capture một lần

**Given** app chạy trong package identity MSIX
**When** thu system audio
**Then** không cần quyền admin hay capability đặc biệt ngoài `microphone` cho mic (NFR-7, NFR-4)

**Given** quyền micro bị chặn trong Windows
**When** người dùng chọn nguồn có mic
**Then** trả `AppError` category `permission` với liên kết mở đúng trang Settings, không crash (FR-17, UX-DR47)

**Given** người dùng đổi thiết bị output mặc định giữa phiên
**When** thiết bị thay đổi
**Then** capture tự thích ứng hoặc trả lỗi thiết bị rõ ràng mà không làm dừng Recording đang ghi từ nguồn khác

**Given** kiểm thử chấp nhận
**When** thu Zoom, Teams và Meet trong Chrome trên máy sạch
**Then** cả ba đều thu được và ghi vào checklist kiểm thử (FR-17)

### Story 4.4: Gemini Live — transport WebSocket, resumption và reconnect (spike S3)

**Phụ thuộc:** 1.7

As Linh họp 60 phút,
I want kết nối tới model tự duy trì qua mọi lần server đóng hoặc mạng chập chờn,
So that transcript liền mạch và tôi không phải làm gì.

**Acceptance Criteria:**

**Given** `gemini/live` với transport là port (có bản giả)
**When** mở phiên ở chế độ **không dịch**
**Then** gửi setup có `inputAudioTranscription: {}` ở **top-level**, không có `outputAudioTranscription`, `generationConfig.responseModalities: ["AUDIO"]`, `translationConfig` với target = ngôn ngữ nguồn (hoặc `ja` khi `auto`) và không có `echoTargetLanguage`, cộng `sessionResumption` và `contextWindowCompression.slidingWindow` (AR-38)
**And** test khoá exact JSON shape của setup; chế độ có dịch được thêm ở Epic 5

**Given** audio từ capture
**When** gửi
**Then** mỗi 100 ms một message `realtimeInput.audio {data: base64 PCM16, mimeType: "audio/pcm;rate=16000"}` (AR-38)

**Given** server gửi `goAway` hoặc đóng kết nối định kỳ
**When** phiên còn chạy
**Then** app reconnect trong suốt bằng resumption handle, không phát lỗi ra UI và gửi lại chunk chưa được ack (FR-22, AR-38)

**Given** bất kỳ lỗi transport nào (timeout, EOF, DNS, TLS, close code khác setup-reject)
**When** mất kết nối
**Then** reconnect với backoff `min(30 s, (1 s × 2^n) × (1 + jitter))`, jitter trong ±20 %, **không giới hạn số lần** khi Phiên còn chạy và reset `n` sau `setupComplete` (FR-22, AR-39)

**Given** server từ chối setup (1007/1008 trước `setupComplete`, hoặc 401/403/404 ở handshake)
**When** xảy ra 5 lần liên tiếp
**Then** dừng thử và phát tín hiệu `SetupRejected` cho tầng trên (FR-22, AR-39)

**Given** mất kết nối kéo dài
**When** audio chưa được gửi
**Then** ring buffer giữ tối đa 60 s (600 chunk 100 ms); phần bị đẩy ra được báo dưới dạng khoảng mất kết nối `[start, end]` (FR-22)

**Given** close reason hoặc lỗi từ server
**When** phát lên tầng trên
**Then** được lọc redaction, không lộ key, URL hay transcript (FR-22, AR-29)

**Given** request Live
**When** cần key
**Then** đi qua cổng `gemini/` với lớp `Live` (ưu tiên cao nhất) và Consent gate (AD-6)

**Given** spike S3 chạy 60 phút thật với ≥ 6 lần đóng kết nối
**When** so sánh audio đã gửi và chunk đã ack
**Then** không mất chunk nào và kết quả ghi thành ADR S3 (AR-9)
**And** ADR xác nhận thực tế các hành vi mà tài liệu Live Translate không mô tả: `goAway`, resumption, sự kiện ngắt lời, và việc model có sinh audio (tốn token) ở chế độ không dịch khi ngôn ngữ là `auto` (AR-54)

**Given** chế độ không dịch
**When** model vẫn trả audio đầu ra
**Then** audio bị bỏ ngay, không phát và không đệm; chỉ transcript đầu vào được dùng (FR-19, AR-54)

**Given** setup thành công, resume handle hết hạn hoặc server không cung cấp ACK audio đủ rõ
**When** chạy spike S3
**Then** ADR phải chứng minh điểm xác nhận audio thực sự, mapping sample/sequence và cách deduplicate sau replay; không coi WebSocket send thành công là model ACK. Handle không còn hợp lệ → kết nối mới giữ đồng hồ cục bộ, replay phần còn giữ được, ghi gap cho phần không thể khôi phục. Không nghiệm thu “không mất/không lặp” chỉ bằng so số lần gửi.

**Given** 401/403, 429, 400/404 hoặc lỗi transport trong Live
**When** quyết định thử tiếp
**Then** áp chính sách key FR-6 trước: không thử lại key đã bị loại, 429 chờ cooldown/đổi key, không đếm transport vào 5 setup rejection. Setup rejection đếm các lần từ chối thực tế và reset sau setupComplete; hết key Auth báo ngay, Recording vẫn tiếp tục. Bộ đếm và retry chỉ do một tầng sở hữu, không nhân thành retry lồng nhau.

**Given** vòng reconnect đang chờ hoặc setup mới đang mở
**When** người dùng Dừng
**Then** huỷ sleep/setup và mọi generation đang chuẩn bị, không kết nối lại sau khi phiên kết thúc; test Dừng ở mọi trạng thái transport.

### Story 4.5: Recording bền và tạo Phiên live khi mở capture

**Phụ thuộc:** 2.3, 4.1, 4.2, 4.3

As Linh,
I want ghi âm được lưu liên tục và không bao giờ dừng vì lý do mạng,
So that dù chuyện gì xảy ra tôi vẫn còn nguyên buổi họp.

**Acceptance Criteria:**

**Given** `media/wav`
**When** ghi Recording
**Then** ghi WAV 16 kHz mono PCM16 liên tục vào Container, vá header mỗi ~160 000 byte (~5 s) để file luôn phát được (FR-23, AR-41)
**And** sau force-quit, file vẫn phát được; phần audio cuối không phát được dài tối đa 5 s tính đến lúc tiến trình bị kết thúc (test kill tiến trình)

**Given** thiết bị capture mở thành công
**When** bắt đầu Live, kể cả khi đang offline
**Then** dòng `sessions` (`kind = live`, `status = recording`) và file `media/<session-id>/recording.wav` được tạo ngay (FR-23, AD-10)

**Given** lỗi xảy ra trước khi thiết bị capture mở
**When** dọn dẹp
**Then** không để lại dòng Phiên hay file chỉ có header ở mọi nhánh lỗi (FR-23)

**Given** mất mạng, mất kết nối model, hết key hoặc lỗi Gemini
**When** đang ghi
**Then** Recording không dừng và không có khoảng trống trong file; chỉ người dùng bấm Dừng hoặc lỗi thiết bị audio mới dừng Recording (FR-23, NFR-3)

**Given** Recording được ghi từ nhánh capture riêng
**When** kết nối WS chậm hoặc chết
**Then** nhánh ghi WAV không đi qua task WS và không bị chặn (AD-10, AR-39)

**Given** Phiên live mới
**When** đặt tên
**Then** tên mặc định là nhãn thời gian cục bộ theo ngôn ngữ UI và tên file chỉ dùng ID tự sinh (FR-26, NFR-12)

**Given** tạo file/DB thất bại sau khi capture mở, ổ đĩa đầy, writer lỗi hoặc buffer ghi âm bị tràn
**When** không còn ghi bền được audio
**Then** dừng capture và chỉ báo “Đang ghi”, báo category `storage`, bảo toàn Recording/Segment đã ghi và đi luồng finalize/recovery an toàn. Không tiếp tục báo “đang ghi” khi đang bỏ sample; không tạo phiên header-only khi khởi tạo thất bại. Lỗi lưu trữ là ngoại lệ vật lý cần bổ sung vào nguồn FR-23, không phải lỗi mạng (C8).

**Given** bắt đầu offline với Consent hiện hành và key đã cấu hình, chưa bị loại Auth
**When** mở capture
**Then** không bắt buộc list-model/key-test online trước; Recording bắt đầu và WS nối sau. Chưa có key hoặc chưa Consent vẫn theo gate FR-2/FR-4, không tự mở một chế độ ghi âm mới ngoài phạm vi.

### Story 4.6: LiveSession — transcript realtime và trạng thái

**Phụ thuộc:** 4.4, 4.5

As Linh,
I want thấy transcript hiện dần theo lời khách nói và biết chính xác app đang ghi hay đang transcribe,
So that tôi theo dõi được buổi họp và không bị đánh lừa bởi trạng thái sai.

**Acceptance Criteria:**

**Given** actor `LiveSession` (mpsc + oneshot, tối đa một phiên, `is_busy`)
**When** gọi `live_start` với nguồn và ngôn ngữ
**Then** capture fan-out sang hai nhánh độc lập: WAV writer (4.5) và WS sender có ring buffer 60 s, không nhánh nào block capture (AD-2, AD-10, AD-11)
**And** gọi `live_start` khi đã có phiên đang chạy trả lỗi rõ ràng

**Given** `live_subscribe(channel)`
**When** UI subscribe (kể cả sau remount)
**Then** nhận `Snapshot { seq, … }` rồi luồng `LiveEvent` (ready, delta, turn, segment, gap, recording `{state}`, connection `{connecting | connected | reconnecting{sinceMs} | stopped}`, log, error, done, final) với `seq` đơn điệu xuyên suốt mọi generation (AD-3, AR-34)

**Given** text stream token-by-token từ model
**When** gặp dấu kết câu
**Then** tách thành Segment với timestamp theo đồng hồ capture cục bộ, và Segment mới hiện ≤ 2 s sau khi câu kết thúc trong điều kiện mạng bình thường (FR-18, AD-9, NFR-5)

**Given** Segment tích luỹ
**When** phiên đang chạy
**Then** được flush định kỳ (≤ 5 s, hằng số trong `live::params`) vào `segments`, và đây là nguồn duy nhất để phục hồi (AD-4)

**Given** khoảng mất kết nối do ring buffer tràn
**When** ghi vào Transcript
**Then** lưu gap `kind = gap`, `gap_reason = disconnected` đúng khoảng thời gian (FR-22, AD-16)

**Given** `LiveGeneration { id, … }`
**When** kết nối bị thay bằng generation mới
**Then** sự kiện mang id cũ bị bỏ và generation cũ drain trong ≤ 1 s (AD-10)

**Given** trạng thái ghi và trạng thái kết nối
**When** phát cho UI
**Then** là hai trường tách biệt; UI không bao giờ được báo "đang transcribe" khi không còn kết nối model sống (FR-22, AD-3)

**Given** test với capture giả và transport giả mô phỏng 60 phút
**When** chèn ≥ 6 lần đóng kết nối và một lần mất mạng 3 phút
**Then** reconnect ngắn trong giới hạn buffer/resumption không mất hoặc lặp Segment đã xác nhận. Với outage 180 s không ACK, ring buffer giữ 60 s cuối: gap chỉ phủ 120 s bị đẩy ra (cộng phần thực tế phát sinh tới lúc nối lại), không phủ cả 180 s nếu 60 s cuối được replay thành công; Recording giữ đủ audio đã capture trong toàn outage (FR-22, AD-10)

**Given** subscribe/remount khi WS đã sống hoặc đang reconnect
**When** lấy Snapshot
**Then** snapshot và đăng ký event được thực hiện nguyên tử với cursor; `connection = connected` chỉ sau setupComplete của generation đang active, mất kết nối phải chuyển ngay khỏi connected. `ready` một lần không đủ để dựng trạng thái sau remount. Đồng bộ bổ sung enum này vào AD-10/addendum §H trước khi sinh binding (C6).

**Given** câu đang stream chưa có dấu kết câu khi turn kết thúc, đổi generation hoặc Dừng
**When** flush
**Then** giữ phần text hợp lệ cuối với timestamp theo sample gốc, không mất hoặc nhân đôi khi nhận event tới muộn. Replay buffer dùng vị trí audio gốc, không gán timestamp bằng thời điểm nhận lại sau reconnect (AD-9).

### Story 4.7: Màn Live — bắt đầu và đang ghi

**Phụ thuộc:** 4.6, 3.2, 3.5, 2.6

As Linh,
I want chọn nguồn, bấm Bắt đầu ghi và theo dõi transcript cùng trạng thái ngay trong một màn,
So that mở app là họp được, đúng lời hứa "cài từ store, mở lên là dùng".

**Acceptance Criteria:**

**Given** sidebar và Trang chủ
**When** epic này hoàn tất
**Then** mục nav Live, card "Bắt đầu Live" ở Trang chủ trống và nút Live primary ở header Trang chủ xuất hiện và mở `/live` (UX-DR24, UX-DR28)

**Given** màn LiveSetup (card 680 px)
**When** hiển thị
**Then** có 3 radio card nguồn (Mic + Hệ thống [Khuyên dùng] kèm dropdown mic và nút làm mới, Chỉ hệ thống, Chỉ mic), ô Tag tuỳ chọn dùng lại Tag picker, select ngôn ngữ transcribe auto/ja/vi/en, ghi chú nói trước chỉ xin System Audio Recording + Micro và không xin Screen Recording (FR-16, FR-17, UX-DR36)
**And** nút "Bắt đầu ghi" là `button-lg` kèm phím tắt

**Given** thiếu quyền hệ thống
**When** người dùng ở LiveSetup
**Then** nếu OS đã xác nhận từ chối, hiện banner “Thiếu quyền hệ thống” với nút mở Settings và đường “Kiểm tra lại quyền”; chỉ nguồn bị từ chối mới bị chặn. Trạng thái chưa hỏi vẫn cho Bắt đầu để kích hoạt prompt OS; không chặn nguồn mic vì system bị từ chối hoặc ngược lại (FR-17)

**Given** chưa có key hợp lệ
**When** ở LiveSetup
**Then** "Bắt đầu ghi" vô hiệu kèm tooltip "Chưa có key" và link tới Settings (FR-4, UX-DR10)

**Given** màn Live đang ghi
**When** hiển thị
**Then** toolbar 64 px có pill "Đang ghi" (đỏ, đồng hồ, dot pulse 1.4 s) và pill kết nối Đang kết nối / Đang transcribe / Đang nối lại (+ thời gian đã chờ) / Đã dừng transcript, luôn là hai pill riêng, mỗi trạng thái có icon và chữ (FR-22, UX-DR13, UX-DR45)
**And** có select nguồn đổi được giữa phiên, toggle Ghi chú và nút **Dừng** (danger)

**Given** transcript gốc streaming
**When** hiển thị
**Then** danh sách Segment có caret ở dòng đang stream, tự cuộn theo dòng mới nhất, dừng ngay khi người dùng cuộn lên và hiện nút "Xuống dòng mới nhất" (FR-18, UX-DR32, UX-DR37)
**And** khi vừa bắt đầu và chưa có lời nào thì hiện dòng "Đang nghe…" không có spinner `[ASSUMPTION]` (UX-DR40)

**Given** panel Ghi chú 320 px
**When** vào màn Live
**Then** mở mặc định, dùng lại component của story 3.5, và mọi ghi chú được lưu bền (FR-36, UX-DR35)

**Given** phím tắt
**When** người dùng bấm `⌘⇧L` hoặc `Ctrl+⇧+L`
**Then** bắt đầu hoặc dừng Live; đăng ký trong `lib/keymap.ts` (UX-DR44)

**Given** `prefers-reduced-motion`
**When** bật
**Then** dot pulse tắt và caret tĩnh (UX-DR7)

**Given** route Live
**When** khai báo layout
**Then** đánh dấu ẩn Ad slot cho mọi trạng thái của route này (AD-13)

**Given** người dùng rời `/live` rồi quay lại trong lúc đang ghi
**When** render
**Then** subscribe lại phiên hiện có, không hiện LiveSetup hoặc tạo Phiên mới; sidebar có chỉ báo và lối trở lại Live. Phím Bắt đầu/Dừng không thực thi hai lần do key-repeat và không vượt qua dialog/permission gate.

**Given** Tag được chọn ở LiveSetup
**When** tạo Phiên thành công
**Then** gắn các Tag vào đúng session_id trong cùng bước tạo dữ liệu; mở capture thất bại không tạo Phiên rác hoặc liên kết Tag mồ côi (FR-29).

### Story 4.8: Live — mất kết nối, lỗi setup và trạng thái lỗi trên giao diện

**Phụ thuộc:** 4.7

As Linh khi Wi-Fi văn phòng rớt 3 phút,
I want thấy app vẫn đang ghi âm và tự chạy tiếp khi có mạng,
So that tôi không hoảng và không phải làm gì.

**Acceptance Criteria:**

**Given** mất mạng giữa phiên
**When** kết nối model rớt
**Then** pill ghi âm vẫn "Đang ghi" và đồng hồ chạy tiếp, pill kết nối chuyển "Đang nối lại (m:ss)" (bộ đếm wall-clock chỉ để hiển thị), kèm banner "Vẫn đang ghi âm — transcript sẽ tự chạy tiếp khi có mạng." (FR-22, AD-9, UX-DR42)

**Given** mạng có lại
**When** reconnect thành công
**Then** transcript tự chạy tiếp trong ≤ 30 s cộng thời gian thiết lập kết nối mà người dùng không phải bấm gì (FR-22)

**Given** khoảng mất kết nối vượt buffer 60 s
**When** hiển thị
**Then** một dòng "Mất kết nối mm:ss–mm:ss" nền `surface-sunken` nằm **trong luồng Segment** đúng vị trí thời gian (FR-22, UX-DR14)

**Given** server từ chối setup 5 lần liên tiếp
**When** tín hiệu `SetupRejected` tới UI
**Then** hiện banner danger với hai lựa chọn "Tiếp tục chỉ ghi âm" và "Dừng" (FR-22)
**And** chọn "Tiếp tục chỉ ghi âm" đặt pill kết nối là "Đã dừng transcript", Recording vẫn ghi và Phiên vẫn lưu được và Transcribe lại sau

**Given** Google trả lỗi category `model`
**When** hiển thị
**Then** banner tại chỗ có lối tắt tới Settings → Gemini và app không tự đổi model (FR-7, UX-DR40)

**Given** mọi key đang nghỉ hoặc bị từ chối
**When** hiển thị
**Then** banner category `quota` hoặc `auth` tại chỗ với hành động phù hợp, không toast (FR-6, UX-DR41)

**Given** mọi lý do lỗi hiển thị
**When** kiểm tra
**Then** không chứa key, URL hay nội dung transcript (FR-22, NFR-9)

**Given** người dùng mù màu
**When** phân biệt "đang transcribe" với "đang nối lại"
**Then** phân biệt được bằng icon và chữ, không chỉ màu (UX-DR45)

**Given** người dùng chọn “Tiếp tục chỉ ghi âm” sau setup bị từ chối
**When** tiếp tục phiên
**Then** dừng reconnect cho phiên đó và hiển thị rõ “Transcript đã dừng cho phiên này”; thay key/model trong Settings chỉ áp dụng lần bắt đầu mới. Đánh dấu khoảng không có transcript tới lúc Dừng bằng gap `disconnected`, giữ toàn bộ Recording và hướng dẫn Transcribe lại. Không tự gọi lại Gemini sau lựa chọn này.

### Story 4.9: Dừng Live và lưu Phiên

**Phụ thuộc:** 4.6, 4.7, 2.7

As Linh vừa họp xong,
I want bấm Dừng và vài giây sau thấy phiên đầy đủ, có thể nghe lại và export,
So that tôi chuyển ngay sang xử lý memo mà không lo mất gì.

**Acceptance Criteria:**

**Given** người dùng bấm Dừng
**When** `live_stop` điều phối trong `ipc/`
**Then** `LiveSession` flush Segment cuối, finalize Recording (`status = finalizing`), tạo Proxy FLAC, đặt `status = complete` và trả `session_id` (AD-1, AD-4, FR-26)

**Given** quá trình finalize
**When** đang chạy
**Then** hiện overlay nhỏ "Đang lưu phiên… finalize recording, tạo proxy" rồi điều hướng sang `/session/:id` của Phiên vừa lưu, không ở lại màn Live (FR-26, UX-DR38)
**And** Ad slot không hiện ở bất kỳ thời điểm nào trong luồng này

**Given** Phiên live được lưu
**When** mở ở Transcript detail
**Then** có cùng schema với Phiên file, badge LIVE, Recording phát lại được, transcript gốc và Ghi chú đầy đủ (FR-26)

**Given** tạo Proxy thất bại
**When** finalize
**Then** Phiên vẫn được lưu với Recording đã có và UI đi nhánh "Không có audio" của FR-10 (AD-19, NFR-2)

**Given** finalize gặp lỗi
**When** kết thúc
**Then** giữ Recording và Segment đã ghi bền, báo category phù hợp; lỗi finalize/Proxy không đổi complete/partial của Transcript. Chỉ đặt `sessions.status = complete` sau khi metadata tối thiểu commit được; nếu DB còn lỗi giữ trạng thái phục hồi được, không báo lưu thành công giả (AD-19)

**Given** thiết bị audio lỗi làm dừng ghi
**When** Recording dừng
**Then** đi cùng đường lưu Phiên như khi bấm Dừng (FR-23)

**Given** Ghi chú đang gõ
**When** bấm Dừng
**Then** Ghi chú được lưu trước khi finalize (FR-36)

### Story 4.10: Boot và đóng cửa sổ — phục hồi phiên mồ côi, chặn thoát

**Phụ thuộc:** 4.9, 2.4

As Linh sau khi app bị force-quit ở phút 50,
I want mở lại app thấy phiên đã nằm sẵn trong thư viện, và không lỡ đóng app khi đang ghi,
So that tôi không mất buổi họp dù có chuyện gì.

**Acceptance Criteria:**

**Given** routine boot duy nhất trong `ipc/boot`
**When** app khởi động
**Then** chạy theo thứ tự: migrate DB → phục hồi volume từ marker Ducking (móc nối, phần phục hồi được hiện thực ở Epic 5) → dọn `media/.staging` → phục hồi Phiên mồ côi (AD-18)

**Given** Phiên có `status ∈ {recording, finalizing}` lúc boot
**When** phục hồi
**Then** chuyển thành `complete` với `recovered = true`, giữ Recording phát được với phần audio cuối không phát được tối đa 5 s trước khi tiến trình bị kết thúc và Segment đã flush, tạo Proxy nếu thiếu (FR-24, AD-4)
**And** chạy nền, không hỏi người dùng, không chặn Trang chủ (mở tới Home ≤ 2 s) (FR-24, NFR-5)

**Given** Phiên phục hồi
**When** hiển thị ở Trang chủ
**Then** có badge "Phục hồi" (màu recover) và gợi ý Transcribe lại; Ghi chú đã lưu còn nguyên (FR-24, UX-DR40)

**Given** test force-kill tiến trình (SIGKILL trên macOS, terminate tương đương trên Windows) giữa lúc Live giả đang chạy
**When** mở lại app
**Then** Phiên nằm trong thư viện với Recording và Segment tích luỹ (SM-4)

**Given** handler đóng cửa sổ duy nhất trong `ipc/close`
**When** người dùng đóng app hoặc cửa sổ trong lúc Live đang chạy
**Then** hiện dialog "Phiên đang ghi — dừng và lưu trước khi thoát?"; nếu tiếp tục, app giữ tiến trình đủ lâu để finalize Recording rồi thoát (FR-25, AD-18, UX-DR16)

**Given** vừa có Live vừa có Job
**When** người dùng đóng app
**Then** finalize Live và huỷ sạch Job sau một lần xác nhận (AD-18)

**Given** thoát bằng menu, `⌘Q` hoặc nút đóng cửa sổ
**When** Live đang chạy
**Then** đi qua cùng handler xác nhận/finalize, giữ tiến trình tới khi dữ liệu tối thiểu được lưu; nếu OS shutdown/kill không cho đủ thời gian, lần boot sau dùng recovery FR-24, không giả định OS luôn cho chạy dialog và finalize vô hạn (FR-25)

**Given** recovery đang tạo Proxy cho Phiên mồ côi
**When** người dùng muốn xoá hoặc Transcribe lại Phiên đó
**Then** session được đánh dấu busy tới khi recovery commit; trước publish kiểm tra Phiên/revision vẫn còn. Recovery lặp lại sau crash không nhân đôi Transcript, không tái tạo dữ liệu đã xoá.

**Given** proxy encode chậm hoặc lỗi trong khi thoát
**When** Recording và metadata tối thiểu đã lưu bền
**Then** không giữ app vô hạn chỉ để tạo Proxy; lưu trạng thái proxy thiếu và để recovery/tái tạo tiếp tục ở lần mở sau. Lỗi không lưu được metadata được báo trước khi thoát tự nguyện (AD-19).

### Story 4.11: Transcribe lại từ Recording và xem cạnh bản live

**Phụ thuộc:** 4.9, 2.5, 2.10

As Linh sau buổi họp bị mất mạng 3 phút,
I want Transcribe lại từ Recording để có bản chất lượng cao hơn và xem cạnh bản live,
So that lấp được đoạn mất kết nối mà không mất bản live, memo hay tag.

**Acceptance Criteria:**

**Given** một Phiên live (kể cả Phiên phục hồi) có Recording
**When** người dùng chọn "Transcribe lại" kèm badge "Tốn token Gemini" trong menu ⋯ của Transcript detail
**Then** đưa một Job vào `JobRegistry` cùng hàng đợi tuần tự, dùng Recording hoặc Proxy làm nguồn và không cần file nguồn (FR-26, AD-11, FR-10)

**Given** Job Transcribe lại
**When** chạy
**Then** tuân toàn bộ FR-12 (tiến độ, huỷ) và FR-15 (Chunk lỗi thành Khoảng thiếu)
**And** kết quả tạo Transcript `variant = retranscribe` bên cạnh `primary` live; mỗi Phiên giữ tối đa một bản hiện hành mỗi variant. Chạy lần sau dựng candidate rồi swap nguyên tử bản retranscribe; huỷ/lỗi giữ cả hai bản cũ

**Given** Transcribe lại kết thúc với Khoảng thiếu (partial)
**When** lưu
**Then** bản live không bao giờ bị ghi đè hoặc xoá (FR-15, FR-26, AD-16)

**Given** Job bị huỷ hoặc lỗi
**When** kết thúc
**Then** bản live và mọi dữ liệu khác giữ nguyên

**Given** Phiên có hai Transcript
**When** mở Transcript detail
**Then** hiện segmented "Bản live · Transcribe lại · Cạnh nhau" và ghi rõ Export/Copy lấy bản đang chọn, hiện cả offset timestamp đang áp dụng (FR-34, UX-DR31)
**And** ở chế độ cạnh nhau hai cột đều nhau, mỗi cột giữ grid 56 px và click Segment ở cột nào cũng seek trình phát

**Given** Phiên có Memo, Tag, Ghi chú
**When** Transcribe lại
**Then** cả ba được giữ nguyên (FR-26)

**Given** khoảng "Mất kết nối" trong bản live
**When** người dùng xem bản Transcribe lại
**Then** đoạn đó có nội dung đầy đủ nếu Chunk tương ứng thành công (UJ-2)

**Given** chế độ Cạnh nhau
**When** chọn nguồn cho Export/Copy hoặc Sinh Memo
**Then** có lựa chọn nguồn rõ ràng “Bản live”/“Transcribe lại”, giữ lần chọn trước (mặc định Bản live); click seek không âm thầm đổi nguồn xuất. Retry khoảng thiếu của bản retranscribe gọi story 2.5 với đúng transcript_id.

**Given** bản live có gap `disconnected` nhưng không có gap `chunk_failed`
**When** hiển thị
**Then** luôn hiện gap và gợi ý Transcribe lại dù trạng thái kỹ thuật Transcript vẫn `complete` theo AD-16; nhãn complete không được diễn đạt thành “không thiếu lời nói”.

### Story 4.12: Tải Recording WAV hoặc FLAC

**Phụ thuộc:** 4.9

As Linh,
I want lưu Recording của phiên live ra ngoài để gửi hoặc lưu trữ,
So that tôi giữ được bản ghi gốc buổi họp.

**Acceptance Criteria:**

**Given** dòng phiên `live` có Recording ở Trang chủ
**When** mở menu ⋯
**Then** có mục "Tải recording"; phiên `file` không có mục này (FR-31, UX-DR29)
**And** cùng mục cũng có trong menu ⋯ của Transcript detail

**Given** người dùng chọn "Tải recording"
**When** dialog lưu hệ thống hiện
**Then** chỉ có hai lựa chọn định dạng WAV và FLAC, không có M4A hay AAC trên cả hai nền tảng (FR-31, mâu thuẫn C2, UX-DR47)

**Given** chọn FLAC
**When** lưu
**Then** Recording WAV được encode sang FLAC theo kiểu streaming, không nạp cả file vào RAM

**Given** lỗi ghi file (ổ đĩa đầy, không có quyền)
**When** lưu thất bại
**Then** trả `AppError` category `storage` với hành động gợi ý và không để lại file dở

**Given** xuất xong
**When** thành công
**Then** hiện toast "Đã lưu" tự tắt sau 4 s (UX-DR18)

**Given** encode/export Recording dài
**When** thực hiện
**Then** hiển thị tiến độ theo thời lượng đã ghi và nút Huỷ; huỷ đóng writer, xoá output tạm và giữ Recording gốc. Chỉ publish file đích sau khi encode hoàn tất, không làm hỏng file đích cũ nếu huỷ/lỗi; đang ghi/finalizing thì mục tải bị chặn kèm lý do (NFR-2).

<a id="epic-5"></a>

## Epic 5: Dịch realtime, TTS & Nhận diện lại

Người dùng đọc bản dịch song song ngay trong họp, đổi Target hoặc "Không dịch" giữa phiên mà không ngắt phiên, bấm Nhận diện lại khi khách đổi ngôn ngữ, và nghe bản dịch qua loa với volume Teams tự hạ (Ducking).

### Story 5.1: Dịch realtime song song và đổi Target giữa phiên

**Phụ thuộc:** 4.8

As Linh họp với khách Nhật,
I want thấy bản dịch tiếng Việt chạy song song với transcript gốc và đổi ngôn ngữ đích hoặc tắt dịch giữa chừng,
So that tôi xác nhận lại yêu cầu ngay trong họp mà không ngắt phiên.

**Acceptance Criteria:**

**Given** chế độ có dịch
**When** mở phiên Live
**Then** setup có `outputAudioTranscription` ở top-level, `translationConfig {targetLanguageCode, echoTargetLanguage: true}` và app xử lý các sự kiện `delta_translated`, `segment_translated` (AR-38, AR-34)
**And** test khoá exact JSON shape của setup ở chế độ có dịch

**Given** màn Live
**When** có Bản dịch
**Then** hiện segmented "Gốc · Dịch · Cả hai" với hai cột "Gốc (ja · auto)" và "Dịch (→ vi)"; ở chế độ Gốc hoặc Dịch chỉ còn một cột (FR-19, UX-DR37)

**Given** nhóm Live mới trong Settings
**When** người dùng chọn Target mặc định
**Then** giá trị được lưu và dùng làm Target khi bắt đầu phiên; LiveSetup có select "Dịch sang" gồm các ngôn ngữ và "Không dịch" (FR-19, FR-39)

**Given** phiên đang chạy
**When** người dùng đổi Target qua dropdown
**Then** app tạo `LiveGeneration` mới, chờ `setupComplete`, swap sender rồi drain generation cũ ≤ 1 s (AD-10)
**And** Transcript gốc không gián đoạn, không Segment lặp, timestamp không nhảy; Bản dịch mới bắt đầu từ câu kế tiếp (FR-19)

**Given** ngôn ngữ nguồn khai báo `ja` nhưng khách nói tiếng Anh hoặc trùng Target
**When** dịch
**Then** Target được chọn thì vẫn hiển thị nội dung ở cột Dịch kể cả khi lời nói đã ở ngôn ngữ đích; không bỏ trống cột này, không đồng nghĩa tự bật TTS (FR-19)

**Given** người dùng chọn "Không dịch"
**When** restart
**Then** setup không có `outputAudioTranscription`, target = ngôn ngữ nguồn (hoặc `ja` khi `auto`) và không có `echoTargetLanguage`, để model giữ im lặng khi tiếng nói đã ở ngôn ngữ đích (FR-19, AR-38, AR-54)
**And** view chỉ còn Gốc, nút TTS vô hiệu và mọi audio đầu ra lỡ nhận về đều bị bỏ, không phát
**And** với ngôn ngữ `auto` mà tiếng nói không phải `ja`, model có thể vẫn sinh audio; chi phí này được đo và ghi lại ở spike S3 và dòng "không tốn token cho dịch" trong UI chỉ được hiển thị khi đã xác nhận đúng (FR-19, UX-DR43)

**Given** phiên kết thúc
**When** lưu
**Then** chỉ transcript gốc được lưu; Bản dịch không có trong `segments` (FR-19, test khẳng định)

**Given** model dịch là bản preview
**When** Google trả lỗi category `model`
**Then** dùng lại banner tại chỗ của story 4.8 (FR-7)

**Given** đổi Target liên tiếp, đồng thời Nhận diện lại, kết nối mới thất bại hoặc Dừng giữa lúc setup
**When** chuyển generation
**Then** LiveSession tuần tự hoá chuyển đổi, chỉ lựa chọn mới nhất còn hiệu lực được swap; thất bại giữ generation/Target cũ và trả dropdown về giá trị thực. Dừng huỷ mọi candidate; deadline setup được chốt ở S3 để không chờ vô hạn. Tắt dịch/TTS dọn ngay audio chờ và phục hồi Ducking; audio/event của generation cũ không được phát sau swap.

**Given** chọn “Không dịch”
**When** kiểm chứng S3 với model thật
**Then** ghi bằng chứng cấu hình nào không yêu cầu output dịch/audio; payload AR-38 là ứng viên từ nguồn, không lấy việc bỏ `outputAudioTranscription` làm bằng chứng rằng model không sinh audio. Nếu model vẫn sinh audio bắt buộc, chặn nghiệm thu FR-19 và cập nhật quyết định nguồn trước khi triển khai, không hứa chi phí bằng 0 từ test JSON shape.

### Story 5.2: Nhận diện lại ngôn ngữ

**Phụ thuộc:** 5.1

As Linh khi khách chuyển sang tiếng Anh giữa họp,
I want bấm "Nhận diện lại" để model bắt lại ngôn ngữ mà không mất gì,
So that bản dịch tốt lại ngay mà phiên và Recording vẫn liền mạch.

**Acceptance Criteria:**

**Given** ngôn ngữ transcribe là `auto`
**When** ở màn Live
**Then** nút "Nhận diện lại" hiện và dùng được; với ngôn ngữ khác `auto` nút bị ẩn hoặc vô hiệu (FR-20, UX-DR37)

**Given** người dùng bấm "Nhận diện lại"
**When** app xử lý
**Then** mở kết nối model mới với ngữ cảnh trống, chờ `setupComplete`, swap sender dưới gate, đóng kết nối cũ trong ≤ 1 s sau khi kết nối mới sẵn sàng (FR-20, AD-10)
**And** sự kiện mang id generation cũ sau thời điểm chuyển bị bỏ

**Given** chuyển generation
**When** kiểm tra kết quả
**Then** không có Segment lặp, timestamp không nhảy (đồng hồ theo số sample), Phiên, Recording và Transcript đã có giữ nguyên (FR-20, AD-9)

**Given** kết nối mới không sẵn sàng
**When** chuyển thất bại
**Then** giữ kết nối cũ, hiện banner cảnh báo nhẹ tại chỗ và Phiên không gián đoạn (FR-20)

**Given** test với transport giả
**When** chuyển khi đang có Segment stream dở
**Then** không mất và không nhân đôi câu đang nói

### Story 5.3: Đọc bản dịch (TTS) trong tiến trình app

**Phụ thuộc:** 5.1, 4.2, 4.3

As Linh đeo tai nghe Bluetooth,
I want nghe bản dịch tiếng Việt qua loa của máy và đổi tai nghe giữa chừng vẫn nghe tiếp,
So that tôi hiểu khách mà không phải nhìn màn hình.

**Acceptance Criteria:**

**Given** nguồn giọng đọc duy nhất là audio đầu ra của model Gemini Live Translate (PCM 16-bit 24 kHz mono little-endian trong `server_content.model_turn.parts[].inline_data`)
**When** phát
**Then** `audio::playback` (cpal) giải mã base64 và phát trực tiếp trong tiến trình Rust, không đi qua IPC; UI chỉ nhận `speaking: bool` (FR-21, AD-10, AR-54)
**And** không có model TTS local, engine TTS của OS hay dịch vụ TTS cloud nào được thêm vào build (kiểm tra manifest) (NFR-4, AR-54)

**Given** audio đầu ra luôn được model sinh khi dịch và không có cách tắt từ phía app
**When** người dùng tắt toggle loa
**Then** app vẫn nhận nhưng **bỏ qua** audio (không phát, không đệm) và tooltip của toggle nói rõ "chỉ tắt việc phát, không giảm token dịch" (FR-21, AR-54, UX-DR43)

**Given** toggle loa TTS
**When** hiển thị
**Then** dùng `aria-pressed` phản ánh đúng bật/tắt và bị vô hiệu kèm tooltip khi đang "Không dịch" (FR-21, UX-DR37, UX-DR45)
**And** pill "Đang đọc" nhỏ ở đầu cột Dịch bật/tắt theo lúc TTS thực sự phát (FR-21, UX-DR40)

**Given** supervisor thiết bị output
**When** người dùng đổi thiết bị mặc định
**Then** poll mỗi 1 s và phát tiếp trên thiết bị mới trong ≤ 2 s
**And** stall 2.5 s được coi là thiết bị chết; thiết bị Bluetooth tắt đột ngột được phát hiện và chuyển trong ≤ 3 s (FR-21, AR-42)

**Given** macOS
**When** TTS phát trong lúc capture system
**Then** tap loại trừ PID của app nên âm thanh TTS không bị thu lại và không có vòng lặp dịch (FR-21, story 4.2)

**Given** Windows
**When** TTS phát trong lúc WASAPI loopback thu system
**Then** âm thanh TTS không đi vào cả Recording lẫn nhánh gửi model; cơ chế phải được spike Phase 0 chứng minh trên Windows 10 1809 và Windows 11, không chỉ OS mới. ADR phải chứng minh vẫn thu đủ lời họp gốc, không dùng tắt toàn bộ system capture khi TTS phát để vượt test (FR-17, FR-21)
**And** kiểm thử chấp nhận: bật TTS khi dịch một đoạn thì transcript gốc không chứa bản đọc lặp

**Given** model báo bị ngắt lời (`audio_interrupted`, hành vi quan sát ở v2, không có trong tài liệu Live Translate)
**When** đang phát TTS
**Then** phát dừng và dọn buffer ngay; hành vi này được xác nhận lại trong spike S3 (AR-54)

### Story 5.4: Ducking volume hệ thống

**Phụ thuộc:** 5.3, 4.10

As Linh họp Teams có TTS,
I want volume Teams tự hạ khi app đọc bản dịch và trả lại khi im,
So that tôi nghe rõ bản dịch mà không phải chỉnh tay.

**Acceptance Criteria:**

**Given** `audio/output_volume` (port từ v2) và TTS đang bật
**When** model bắt đầu nói
**Then** volume hệ thống hạ còn 30 % và phục hồi sau khi model im (FR-21)

**Given** người dùng tự chỉnh volume trong lúc đang Ducking
**When** app phát hiện thay đổi
**Then** app bỏ mức volume gốc đã lưu và giữ mức người dùng vừa chỉnh, không "đánh nhau" với thao tác đó (FR-21, AR-42)

**Given** chuẩn bị Ducking
**When** thay volume lần đầu
**Then** ghi bền marker `state/ducking.json` trước khi đổi volume, gồm device ID, mức gốc, mức app áp và hiệu lực phục hồi; lượt nói chồng nhau không ghi đè mức gốc bằng mức đã duck (AD-18)

**Given** app crash hoặc bị force-quit khi đang Ducking
**When** mở lại app
**Then** routine boot đọc marker **sau migrate DB, trước dọn staging/recovery**; chỉ phục hồi đúng device khi marker còn hiệu lực và volume chưa bị chỉnh ngoài ý muốn. Chỉnh volume tay vô hiệu hoá marker ngay; thiết bị vắng/volume đã đổi thì không ép volume cũ lên thiết bị khác. Xử lý marker rồi dọn theo kết quả (FR-21, AD-18)

**Given** TTS bị tắt, Dừng Live, đổi thiết bị hoặc đóng app
**When** đang Ducking
**Then** volume trên thiết bị cũ được phục hồi nếu marker còn hiệu lực; nếu người dùng đã chỉnh tay thì giữ mức mới. Đổi output tạo baseline mới cho thiết bị mới, không dùng volume của thiết bị cũ

**Given** test với backend volume giả lập nội bộ `audio/` (không thêm port kiến trúc thứ tư)
**When** chạy các kịch bản crash, chỉnh tay và nhiều lượt nói liên tiếp
**Then** volume cuối luôn đúng mức mong đợi

<a id="epic-6"></a>

## Epic 6: Cấu hình từ xa & Ad slot

Người dùng tuỳ chọn tải và áp dụng bộ cấu hình đề xuất (xem diff, không bao giờ đụng key); Ad slot house ads đúng chuẩn store (Sponsored, Báo cáo quảng cáo, Vì sao tôi thấy quảng cáo này), không tracking, tự fallback khi offline và **không bao giờ hiện ở màn Live**.

### Story 6.1: Module `remote/` — tải, xác minh chữ ký, cache, fallback

**Phụ thuộc:** 1.2

As đội build,
I want một cổng duy nhất tải nội dung từ xa có chữ ký, có cache và fallback,
So that ads và cấu hình đề xuất không thể bị chèn nội dung độc hại hay làm hỏng lõi.

**Acceptance Criteria:**

**Given** module `remote/`
**When** feature cần `ads.json`, ảnh Creative hoặc `recommended-settings.json`
**Then** chỉ tải qua `remote/` bằng GET, không cookie, không identifier, không header định danh người dùng (AD-14, FR-45)

**Given** nội dung tải về
**When** xác minh
**Then** kiểm chữ ký ed25519 (`ed25519-dalek`) bằng public key nhúng trong binary; chữ ký sai thì bỏ qua và dùng cache hoặc fallback nhúng sẵn (FR-46, AR-28)

**Given** cache
**When** lưu
**Then** đặt tại `cache/remote/<sha256-của-url>` (không dùng chuỗi từ server làm đường dẫn) và có hạn 24 h (AD-5, NFR-12)
**And** khi cache hết hạn mà tải lỗi hoặc chữ ký sai, dùng bản cache đã xác minh trước đó nếu còn, nếu không thì fallback nhúng sẵn `[ASSUMPTION]`

**Given** ảnh Creative
**When** tải
**Then** chỉ chấp nhận loại ảnh cho phép và kích thước ≤ 100 KB, vượt thì bỏ; không bao giờ tải hay thực thi script hoặc HTML (FR-46, AD-14)

**Given** `remote/`
**When** hoạt động
**Then** không bao giờ ghi settings (AD-8, AD-14) và chỉ tải khi có thao tác người dùng hoặc khi app mở/cache hết hạn, không có timer nền (NFR-8)

**Given** URL nguồn và public key
**When** cấu hình
**Then** là hằng số cấu hình; nơi host thật chốt cùng Open Question 6 và public key nhúng lúc build (AR-49)

**Given** test với server giả và đồng hồ giả
**When** chạy các kịch bản chữ ký đúng, chữ ký sai, bị sửa nội dung, ảnh quá lớn, JSON hỏng, cache hết hạn và offline
**Then** trả kết quả typed gồm nguồn dữ liệu (`fresh | verified-cache | embedded`) và lỗi đã phân loại/redact nếu có; không panic hoặc ảnh hưởng lõi. Ads có thể fallback; tải cấu hình đề xuất thất bại phải báo lỗi và không coi cache/fallback là bản tải mới thành công (AD-19)

**Given** JSON/ảnh/chữ ký từ remote
**When** tải và kiểm chứng
**Then** story 6.5 và client dùng cùng format envelope, version, bytes ký; ảnh có digest trong manifest đã ký hoặc chữ ký riêng, thay bytes tại URL không đổi phải bị từ chối. Chỉ fetch HTTPS tới nguồn đã cấu hình; kiểm tra redirect, timeout, giới hạn JSON, ảnh ≤ 100 KB khi stream và kích thước decode để tránh cạn RAM; MIME/bytes phải khớp loại ảnh cho phép.

**Given** URL click/report từ Creative
**When** mở bên ngoài
**Then** chỉ cho HTTPS, riêng report cho phép mailto; từ chối file/javascript/custom scheme và encode Creative ID như dữ liệu, không ghép thẳng vào URL (FR-46, NFR-12).

### Story 6.2: Đồng bộ cấu hình đề xuất

**Phụ thuộc:** 6.1, 1.9, 3.6

As Minh,
I want tải bộ cấu hình đề xuất, xem trước những gì sẽ đổi rồi mới áp dụng,
So that model và chunk luôn hợp lý mà key của tôi không bao giờ bị đụng tới.

**Acceptance Criteria:**

**Given** nhóm "Cấu hình đề xuất" trong Settings (xuất hiện ở story này)
**When** người dùng bấm "Tải cấu hình đề xuất"
**Then** `remote/` tải và xác minh chữ ký, `settings_recommended_preview` tính diff và UI hiển thị **inline ngay trong nhóm** (không thêm route) (FR-42, AD-8, UX-DR39)
**And** mỗi dòng dạng `nhãn · giá trị hiện tại → giá trị đề xuất`, thiết lập không đổi thì không liệt kê

**Given** diff đang hiển thị
**When** người dùng bấm "Áp dụng"
**Then** `settings_recommended_apply` áp dụng đúng các dòng đã liệt kê và hiện toast "Đã áp dụng N thiết lập"
**And** bấm "Huỷ" thì không đổi gì

**Given** bộ cấu hình có chứa key hoặc thông tin Consent
**When** tính diff hoặc áp dụng
**Then** phần đó bị bỏ qua hoàn toàn, không bao giờ liệt kê hay ghi đè (FR-42, AD-8); test khẳng định

**Given** bộ cấu hình có Template memo
**When** áp dụng
**Then** chỉ thêm hoặc cập nhật các mẫu mang id đề xuất và không xoá hay sửa mẫu do người dùng tạo `[ASSUMPTION]`

**Given** chữ ký sai hoặc mạng lỗi
**When** tải
**Then** hiện banner category tại chỗ và không thay đổi bất kỳ setting nào (FR-42, UX-DR39)

**Given** tệp có trường không nhận biết hoặc phiên bản mới hơn
**When** parse
**Then** bỏ qua trường lạ, không crash, và báo rõ nếu phiên bản không hỗ trợ

**Given** người dùng không bấm nút
**When** app chạy
**Then** không có request tải cấu hình nào phát sinh (NFR-8)

**Given** chữ ký hợp lệ nhưng giá trị sai kiểu/phạm vi, payload thay đổi hoặc người dùng sửa settings sau preview
**When** bấm Áp dụng
**Then** chỉ allow-list model/chunk/Template và chạy lại cùng validator local. Preview gắn digest payload + revision settings; stale preview phải tính lại và hiển thị diff mới, không áp tự động. Ghi toàn bộ diff hợp lệ trong một transaction; lỗi giữ nguyên settings. Bấm Huỷ vô hiệu hoá preview, không thể áp lại token cũ (FR-42, AD-8).

**Given** Template đề xuất đã được người dùng chỉnh
**When** tính diff
**Then** hiển thị xung đột và không tự ghi đè; phân biệt external ID với UUID nội bộ và origin, nội dung phải qua validator `{transcript}` như Template local. Không nhận đường dẫn hoặc ID làm tên file từ payload.

### Story 6.3: Pipeline Creative — chọn, xoay, giới hạn và cờ tắt quảng cáo

**Phụ thuộc:** 6.1

As chủ sản phẩm,
I want app chọn Creative đúng locale, xoay theo trọng số, giới hạn tần suất và tắt được từ xa,
So that quảng cáo tạo cross-promotion mà không làm hỏng trải nghiệm B2B.

**Acceptance Criteria:**

**Given** `ads/` đọc `ads.json` đã xác minh qua `remote/`
**When** parse Creative `{id, image, title, sponsor, url, locale[], start, end, weight}`
**Then** chỉ giữ các Creative khớp locale UI và còn trong thời hạn (FR-44)
**And** ảnh kích thước 300×100 hoặc 320×50

**Given** nhiều Creative hợp lệ
**When** `ads_next` được gọi
**Then** chọn xoay theo `weight` và mỗi Creative hiển thị tối đa một lần mỗi 10 phút (FR-44, AR-50)

**Given** ảnh Creative
**When** tải và cache
**Then** lưu trong Container với tên từ ID tự sinh hoặc hash, kích thước ≤ 100 KB (FR-46, NFR-12)

**Given** không còn Creative hợp lệ cả trong cache đã xác minh hoặc chưa có cache khi offline
**When** cần hiển thị và cờ quảng cáo cho phép
**Then** trả Creative mặc định nhúng sẵn giới thiệu trans-kun/Relipa (FR-44)

**Given** cờ `ads_enabled` từ server bằng false hoặc cờ cục bộ `is_premium` bằng true (chưa có UI)
**When** `ads_next` được gọi
**Then** không trả Creative nào và Ad slot ẩn hoàn toàn (FR-45)

**Given** impression và click
**When** ghi nhận
**Then** chỉ đếm cục bộ và không gửi đi đâu; redirect URL chỉ có tham số chiến dịch, không định danh người dùng (FR-45, AR-50)

**Given** `ads_report(id)`
**When** gọi
**Then** trả URL hoặc `mailto` có sẵn `id` Creative để mở báo cáo (FR-45)

**Given** lỗi bất kỳ trong `ads/`
**When** xảy ra
**Then** không ảnh hưởng transcript hay Recording (AD-19, NFR-2)

**Given** trọng số không hữu hạn/không dương, creative hết hạn, đã chạm cap hoặc ads đang bị tắt
**When** chọn Creative
**Then** bỏ phần tử không hợp lệ; không còn candidate dùng fallback nhúng, nhưng cờ tắt có ưu tiên cao hơn fallback. Cap áp cho Creative từ xa; fallback là nội dung chờ, không lặp cộng impression do remount. Một impression chỉ ghi khi thực sự hiển thị, timestamp cap lưu cục bộ qua điều hướng/restart; ở `/live` không gọi ads_next hoặc đếm impression.

**Given** cache đã xác minh còn Creative đúng locale/thời hạn
**When** mạng lỗi
**Then** dùng cache theo 6.1 trước rồi fallback nhúng; cờ ads_enabled=false đã xác minh vẫn có hiệu lực offline. TTL cache hết không được bỏ qua start/end của từng Creative.

### Story 6.4: Ad slot ở sidebar và tuân thủ store

**Phụ thuộc:** 6.3, 1.3

As Minh,
I want thấy một banner nhỏ, rõ là quảng cáo, báo cáo được và không bao giờ xuất hiện khi đang họp,
So that quảng cáo không làm tôi phân tâm hay mất lòng tin.

**Acceptance Criteria:**

**Given** sidebar 260 px
**When** hiển thị ở Trang chủ, Transcript detail hoặc Settings
**Then** Ad slot rộng 236 px nằm ở đáy sidebar, nền surface, viền 1 px, radius 10, chứa creative (ảnh + text tĩnh), nhãn "Sponsored", nút "Báo cáo quảng cáo" và link "Vì sao tôi thấy quảng cáo này" (FR-44, FR-45, UX-DR22)
**And** creative 300×100 co theo chiều rộng hoặc 320×50

**Given** người dùng bấm creative
**When** click
**Then** URL mở bằng trình duyệt ngoài; không script, không HTML từ server, không âm thanh, không interstitial, không che nội dung (FR-44, FR-46)

**Given** "Báo cáo quảng cáo"
**When** bấm
**Then** mở URL hoặc mailto có sẵn `id` Creative (FR-45)

**Given** "Vì sao tôi thấy quảng cáo này"
**When** bấm
**Then** hiện text tĩnh theo ngôn ngữ UI, không nhắc dữ liệu cá nhân (FR-45)

**Given** route `/live` ở mọi trạng thái (LiveSetup, đang ghi, đang lưu phiên)
**When** kiểm tra DOM
**Then** không có Ad slot; quy tắc do layout theo route thực thi, e2e test khẳng định (FR-44, AD-13, SM-5)

**Given** không có Creative hợp lệ, đang tải hoặc offline
**When** hiển thị
**Then** slot luôn có nội dung cùng kích thước (fallback nhúng sẵn) và layout không nhảy hay co lại (UX-DR22)

**Given** theme sáng và tối, ba ngôn ngữ vi/en/ja
**When** hiển thị
**Then** đạt tương phản AA, nhãn không bị cắt và nút Báo cáo tới được bằng bàn phím với `aria-label` (UX-DR45, UX-DR46)

**Given** cờ `ads_enabled = false` hoặc `is_premium = true`
**When** hiển thị
**Then** không còn Ad slot và layout sidebar không để lại khoảng trống lạ (FR-45)

### Story 6.5: Công cụ ký, xuất bản nội dung từ xa và smoke test hằng tuần

**Phụ thuộc:** 6.1, 1.7, 4.4

As chủ sản phẩm,
I want quy trình ký và xuất bản `ads.json` cùng `recommended-settings.json`, và smoke test tự động cho model,
So that nội dung từ xa an toàn và tôi biết sớm khi Google đổi model preview.

**Acceptance Criteria:**

**Given** công cụ ký (script)
**When** tạo `ads.json` hoặc `recommended-settings.json`
**Then** sinh đúng envelope/bytes ký đã chốt với story 6.1, gồm digest ảnh Creative; app xác minh được bằng public key nhúng. Private key nằm ngoài repo app, người giữ và cách cấp cho pipeline được ghi lại trước publish (AR-49)

**Given** nơi host trang tĩnh (Open Question 6: LP repo hiện tại hay LP mới, GitHub Pages hay Cloudflare Workers + KV)
**When** chốt
**Then** quy trình xuất bản `ads.json`, `recommended-settings.json` và Privacy Policy được ghi lại kèm URL cuối cùng cấu hình vào app (AR-49)

**Given** khoá ký bị lộ hoặc cần xoay
**When** áp dụng quy trình
**Then** quy trình xoay khoá được tài liệu hoá, gồm cách phát hành bản app mới với public key mới

**Given** workflow GitHub Actions chạy hằng tuần
**When** dùng key test riêng
**Then** kiểm tra model Live preview mở được setup và model transcribe mặc định trả được một Chunk mẫu (AR-49, bài học D8)
**And** khi lỗi thì thông báo cho chủ sản phẩm và có hướng dẫn cập nhật `recommended-settings.json` để người dùng đồng bộ model mới (FR-42)

**Given** server ads trả 500, JSON hỏng, chữ ký sai hoặc ảnh hỏng
**When** chạy test tích hợp trong lúc đang có Live và Job
**Then** transcript và Recording không bị ảnh hưởng và không đổi `status` của transcript (AD-19, NFR-2)

<a id="epic-7"></a>

## Epic 7: Sẵn sàng lên store & ra mắt

Đội build có bản Mac App Store (universal, sandbox) và Microsoft Store (MSIX x64, Arm64 nếu có máy test) đạt review: entitlement tối thiểu, hồ sơ submit đầy đủ (Privacy Policy 3 ngôn ngữ, screenshot, mô tả vi/en/ja, review notes và key demo), kiểm sandbox thật, WACK, đo NFR-5/10/11 và diễn tập luồng reviewer (UJ-5). Epic này không sở hữu FR mới; nó phủ NFR-4, 7, 10, 11 và AR-44..48, và thực hiện mục tiêu SM-1.

### Story 7.1: Build store-mac hoàn chỉnh và kiểm chứng sandbox thật

**Phụ thuộc:** Tất cả story Epic 1–6 (bản tích hợp cuối); phần spike store đã làm ở Phase 0.

As đội build,
I want bản Mac App Store đầy đủ chạy đúng trong sandbox thật với mọi tính năng,
So that không tính năng nào (đặc biệt Live) vỡ khi qua review.

**Acceptance Criteria:**

**Given** bản beta rỗng của story 1.11
**When** nâng lên build đầy đủ
**Then** entitlements cuối cùng chỉ gồm danh sách của AD-17 (sandbox, network client, audio input, file người dùng chọn, keychain access group nếu cần) và `NSAudioCaptureUsageDescription`, `NSMicrophoneUsageDescription`, universal, `minimumSystemVersion 14.4` (AR-44, NFR-7)
**And** workflow CI `store-mac` sinh `.pkg` đã ký và upload TestFlight end-to-end (AR-48)

**Given** bản build từ `/Applications` (không phải dev build)
**When** chạy checklist sandbox thật
**Then** đạt: mở file bằng dialog, relaunch xem lại và phát mọi Phiên từ Proxy, ghi Live system + mic từ Zoom, Teams và Google Meet trong Chrome, TTS không bị thu lại, lưu key trong Keychain, export bằng dialog lưu (FR-10, FR-17, NFR-7)
**And** không có prompt Screen Recording nào xuất hiện ở bất kỳ bước nào

**Given** binary và bundle
**When** kiểm tra tự động
**Then** không có updater, không có code tải về lúc chạy, không sidecar, không plugin cấm (NFR-4, AR-3)

**Given** TestFlight for Mac
**When** phát cho 5–10 người dùng nội bộ
**Then** phản hồi được thu thập và các lỗi chặn được xử lý trước khi submit (AR-53)

**Given** `codesign -d --entitlements`
**When** so với danh sách mong đợi
**Then** khớp chính xác và không thừa entitlement nào

### Story 7.2: Build store-win hoàn chỉnh và kiểm chứng MSIX

**Phụ thuộc:** Tất cả story Epic 1–6 (bản tích hợp cuối); phần spike store đã làm ở Phase 0.

As đội build,
I want gói MSIX đầy đủ chạy đúng trên máy Windows sạch và có proxy công ty,
So that Minh cài từ Microsoft Store là dùng được ngay.

**Acceptance Criteria:**

**Given** bản beta rỗng của story 1.12
**When** nâng lên build đầy đủ
**Then** MSIX x64 với capability chỉ có `microphone`, WACK pass và upload Store package flight (AR-45, NFR-7)
**And** Arm64 chỉ vào bản submit đầu nếu đã có máy test thật trước khi submit, nếu không thì submit x64 trước và bổ sung Arm64 sau (Open Question 7)

**Given** máy Windows 10 1809 và Windows 11 sạch
**When** cài từ package flight
**Then** chạy được toàn bộ luồng chính và cài/gỡ sạch mà không cần quyền admin (NFR-4)

**Given** thiếu WebView2 Evergreen runtime
**When** app khởi động
**Then** kiểm tra từ Rust/native **trước khi khởi tạo WebView**; nếu thiếu, hiện hướng dẫn native đủ vi/en/ja, không phụ thuộc Svelte/WebView2 để render lỗi. Chốt chiến lược cài runtime ở spike S6 phù hợp NFR-4; nếu chỉ cài được với admin trên máy mục tiêu thì chưa đạt, không đánh dấu pass (§7 PRD, UX-DR47)

**Given** ghi Live trên Windows
**When** họp bằng Zoom hoặc Teams native và Google Meet trong Chrome
**Then** thu được cả hai loại (endpoint communications và console) (FR-17)

**Given** máy có proxy Zscaler hoặc CA doanh nghiệp
**When** kiểm tra key và transcribe một file
**Then** hoạt động qua CA store hệ thống, hoặc hiện lỗi category "Mạng / CA" với hướng dẫn riêng, không phải lỗi chung chung (NFR-6, UX-DR41)

**Given** Credential Manager
**When** lưu, đọc, xoá key trong package identity
**Then** hoạt động đúng (FR-5)

### Story 7.3: Hồ sơ submit và tài liệu quyền riêng tư

**Phụ thuộc:** 1.5, 6.5

As chủ sản phẩm,
I want đầy đủ hồ sơ submit cho cả hai store,
So that review không bị trả về vì thiếu giấy tờ hay khai báo sai.

**Acceptance Criteria:**

**Given** Privacy Policy
**When** xuất bản
**Then** có bản vi/en/ja công khai cùng trang support/contact ở URL đã chốt (Open Question 6), nêu rõ: không tài khoản, không telemetry, dữ liệu họp chỉ gửi tới Google theo điều khoản Gemini API của chính người dùng và cách xoá dữ liệu cục bộ (§6.1, FR-40)
**And** hằng số URL Privacy Policy trong app (story 1.5) được đặt về URL thật

**Given** khai báo privacy
**When** điền Apple privacy nutrition label
**Then** "Audio Data / User Content gửi tới Google để cung cấp tính năng, không liên kết danh tính, không tracking; Advertising Data: none" và tương đương cho khai báo của Microsoft (AR-47, §6.1)

**Given** metadata store
**When** chuẩn bị
**Then** có tên app, category Productivity, age rating 4+, screenshot đúng kích thước từng store, mô tả vi/en/ja và `ITSAppUsesNonExemptEncryption=false` (AR-47)

**Given** key demo cho reviewer
**When** chuẩn bị
**Then** đã quyết định ai sở hữu, quota bao nhiêu và cách xoay khi hết hạn (Open Question 4); App Review Notes và Notes for certification có key demo và hướng dẫn 3 bước (AR-47)

**Given** rà soát nội dung app và hồ sơ
**When** kiểm tra
**Then** không nhắc tới kênh tải ngoài store hay license riêng; văn bản Consent khớp Privacy Policy (§6.2)

### Story 7.4: Kiểm chứng NFR, tiếp cận và các luồng chính trên máy sạch

**Phụ thuộc:** 7.1, 7.2, 7.3

As chủ sản phẩm,
I want số đo thật và chạy thử end-to-end trên máy sạch cả hai OS,
So that tôi biết sản phẩm đạt lời hứa trước khi submit.

**Acceptance Criteria:**

**Given** máy 4 nhân, mạng bình thường
**When** đo NFR-5
**Then** decode + cắt Chunk ≥ 20× realtime, app mở tới Home ≤ 2 s và độ trễ Live ≤ 2 s sau kết câu; số đo được ghi lại (NFR-5)
**And** ngưỡng nào không đạt được ghi thành mục xử lý hoặc quyết định điều chỉnh có duyệt của chủ sản phẩm

**Given** bản build store
**When** đo NFR-10
**Then** bundle ≤ 60 MB trên mỗi nền tảng và RAM ≤ 300 MB khi Live 60 phút (NFR-10)

**Given** kiểm tra tiếp cận
**When** chạy
**Then** bắt đầu/dừng Live, tìm kiếm và export dùng được hoàn toàn bằng bàn phím; focus ring hiện trên mọi control; `role=slider`, `aria-pressed`, `role=status`, `aria-live=polite` đúng chỗ; không icon-only button thiếu `aria-label` (NFR-11, UX-DR44, UX-DR45)
**And** tương phản WCAG AA ở cả hai theme (CI và kiểm tay), `prefers-reduced-motion` hoạt động (FR-48)

**Given** ba ngôn ngữ vi/en/ja
**When** duyệt mọi màn chính bằng mắt
**Then** không chuỗi nào bị cắt, thiếu key hay lệch nghĩa (FR-47, UX-DR46)

**Given** năm hành trình UJ-1 đến UJ-5 và các đường thất bại
**When** chạy trên máy sạch cả macOS và Windows theo checklist
**Then** đạt: UJ-1 tới transcript chạy trong ≤ 3 phút; UJ-2 gồm đổi Nhận diện lại, đổi nguồn, reconnect, mạng rớt 3 phút và force-quit vẫn phục hồi; UJ-3 gồm xoay key, Khoảng thiếu, Chạy lại phần thiếu và memo; UJ-4 tìm đúng câu ≤ 30 s; UJ-5 không màn trắng, crash hay quyền lạ (UX-DR48, SM-2, SM-4)

### Story 7.5: Submit, xử lý review và ra mắt

**Phụ thuộc:** 7.4

As chủ sản phẩm,
I want submit lên hai store, xử lý phản hồi review và chuyển người dùng v2 sang,
So that trans-kun lên store và người dùng cũ chuyển sang mà không mất năng lực cốt lõi.

**Acceptance Criteria:**

**Given** hồ sơ và build đã sẵn sàng
**When** diễn tập luồng reviewer
**Then** một người ngoài dự án đi hết UJ-5 chỉ với review notes và key demo, không cần hỏi thêm (§6.2, UX-DR48)

**Given** kết quả diễn tập đạt
**When** submit App Store Connect và Partner Center
**Then** lưu bằng chứng nộp từng store và xử lý phản hồi theo checklist; 1–2 vòng chỉ là dự trù, không phải điều kiện đủ để đánh dấu ra mắt (AR-53)

**Given** Apple từ chối vì app không dùng được nếu không có key hay dịch vụ ngoài và key demo không được chấp nhận
**When** điều kiện này xảy ra
**Then** chủ sản phẩm quyết định có kích hoạt phương án dự phòng key do Relipa cấp (ephemeral token, `KeyProvider`) hay không và quyết định được ghi lại; phương án này không được triển khai trong v3 nếu chưa kích hoạt (AR-51, addendum §K)

**Given** ngày ra mắt
**When** thông báo cho người dùng Transcriber-kun v2
**Then** thông điệp nói rõ trans-kun là app mới độc lập trên store, không import dữ liệu v2 và v2 ngừng phát hành sau 3 tháng (Q8, AR-53)
**And** nội dung trong app vẫn không nhắc kênh tải ngoài store

**Given** sau ra mắt
**When** theo dõi chất lượng
**Then** dùng báo cáo crash của store và phản hồi người dùng, không thêm telemetry vào app (NFR-1)

**Given** tiêu chí thành công định tính
**When** rà soát cuối
**Then** SM-1 đến SM-5 được chủ sản phẩm xem xét và ghi nhận đạt hay chưa đạt kèm lý do (§12)

**Given** review của một store chưa xong hoặc build bị từ chối
**When** cập nhật tiến độ ra mắt
**Then** phân biệt submitted / in-review / rejected / approved / published theo từng store; chỉ nghiệm thu SM-1 và story ra mắt khi cả hai listing phát hành, cài được từ store bằng tài khoản người dùng thông thường, kèm URL/version/bằng chứng. Không coi upload/flight là sản phẩm đã lên store.

<a id="coverage"></a>

## Ánh xạ yêu cầu tới story

Coverage là nơi chịu trách nhiệm, không phải bằng chứng yêu cầu đã pass. Story có gate mở chưa được tính là hoàn tất.

| FR | Story thực hiện/kiểm chứng | Nội dung |
|---|---|---|
| FR-1 | 1.4 | Chọn ngôn ngữ UI khi mở lần đầu |
| FR-2 | 1.5, 1.7 | Màn Consent trước mọi request tới Google |
| FR-3 | 1.6, 1.7, 1.8 | Nhập và kiểm tra API key trong Onboarding |
| FR-4 | 1.8, 2.8, 4.7, 3.7 | Trạng thái "chưa có key" thân thiện |
| FR-5 | 1.6, 1.11, 1.12 | Lưu key trong kho khoá OS |
| FR-6 | 1.6, 2.5, 4.4 | Key pool xoay vòng theo quota |
| FR-7 | 1.7, 1.9 | Chọn model theo mục đích, tải danh sách |
| FR-8 | 2.4, 2.8 | Nhận file qua dialog/kéo thả |
| FR-9 | 2.1, 2.8 | Định dạng hỗ trợ, lỗi tức thì |
| FR-10 | 2.1, 2.3, 2.7, 4.9 | Proxy phát lại trong Container, không phụ thuộc file nguồn |
| FR-11 | 2.3, 2.5, 2.8 | Phát hiện trùng theo hash, mở phiên có sẵn |
| FR-12 | 2.4, 2.9, 4.10 | Progress theo thời lượng, cancel, tiếp tục xem |
| FR-13 | 2.1, 2.2, 2.6 | Chunk, timestamp tuyệt đối, offset |
| FR-14 | 2.2 | Hai họ model transcribe |
| FR-15 | 2.2, 2.5, 2.7, 4.11 | Chịu lỗi từng Chunk, không giấu Khoảng thiếu |
| FR-16 | 2.6, 4.7 | Ngôn ngữ transcribe |
| FR-17 | 4.1, 4.2, 4.3, 4.7 | Chọn Nguồn audio, đổi giữa phiên |
| FR-18 | 4.6, 4.7 | Transcript realtime |
| FR-19 | 5.1 | Dịch realtime, Target đổi được, Không dịch |
| FR-20 | 5.2 | Nhận diện lại ngôn ngữ |
| FR-21 | 5.3, 5.4 | Đọc bản dịch (TTS) + Ducking |
| FR-22 | 4.4, 4.6, 4.8 | Kết nối bền, trong suốt |
| FR-23 | 4.5, 4.9 | Recording bền, sống sót qua crash |
| FR-24 | 4.10 | Phục hồi phiên mồ côi |
| FR-25 | 4.10 | Chặn thoát khi đang ghi |
| FR-26 | 4.9, 4.11 | Lưu phiên live, Transcribe lại, xem cạnh bản live |
| FR-27 | 2.9, 3.1 | Danh sách phiên và Job đang chạy (Home) |
| FR-28 | 3.3 | Tìm theo tên |
| FR-29 | 3.2, 4.7 | Tag |
| FR-30 | 3.1 | Đổi tên, xoá phiên |
| FR-31 | 4.12 | Tải Recording (WAV/FLAC) |
| FR-32 | 2.1, 2.7 | Trình phát và click-to-seek |
| FR-33 | 2.10 | Tìm trong Transcript |
| FR-34 | 2.7, 4.11 | Hiển thị Transcript (chế độ cạnh nhau hoàn thiện ở Epic 4 khi có Transcribe lại) |
| FR-35 | 2.10 | Export và copy |
| FR-36 | 3.5, 4.7, 4.9 | Ghi chú tự lưu |
| FR-37 | 3.6 | Template memo (CRUD) |
| FR-38 | 3.7 | Sinh, cache, sinh lại Memo |
| FR-39 | 1.9, 2.6, 3.6, 5.1 | Khung Settings + nhóm Chung/Gemini (trường Chunking bổ sung ở Epic 2, Memo ở Epic 3, Live ở Epic 4/5) |
| FR-40 | 3.4 | Lưu trữ trong Container |
| FR-41 | 1.2, 1.10 | Nhật ký chẩn đoán content-free, xuất qua allow-list |
| FR-42 | 6.1, 6.2 | Đồng bộ cấu hình đề xuất |
| FR-43 | 1.10, 7.3 | About & Privacy |
| FR-44 | 6.3, 6.4 | Hiển thị Creative (Ad slot) |
| FR-45 | 6.3, 6.4 | Tuân thủ store cho quảng cáo |
| FR-46 | 6.1, 6.5 | Toàn vẹn Creative (chữ ký, ảnh ≤ 100 KB) |
| FR-47 | 1.4, 7.4 | Bộ key i18n đồng bộ |
| FR-48 | 1.3, 1.9, 7.4 | Theme sáng/tối |

| NFR | Story chịu trách nhiệm | Bằng chứng cần có |
|---|---|---|
| NFR-1 | 1.2, 1.7, 1.10, 6.1, 6.3 | Transport spy + quét log/export; không gửi telemetry |
| NFR-2 | 2.3, 3.7, 4.9, 4.12, 6.5 | Fault injection; busy/progress/cancel cho tác vụ dài, không hỏng dữ liệu cũ |
| NFR-3 | 3.5, 4.4–4.6, 4.9–4.10 | Kill/relaunch và outage; phân biệt dữ liệu đã commit với đang chờ |
| NFR-4 | 1.1, 1.11–1.12, 7.1–7.2 | Manifest/bundle scan, máy sạch, WebView2 native preflight |
| NFR-5 | 2.1, 2.9, 4.6, 7.4 | Máy/fixture/phiên bản/điều kiện mạng và số đo lặp lại được |
| NFR-6 | 1.7, 4.4, 7.2 | REST và WS với CA/proxy doanh nghiệp, category Tls |
| NFR-7 | 1.11–1.12, 4.2–4.3, 7.1–7.2 | Bản ký sandbox/MSIX thật; entitlement/capability đúng nguồn |
| NFR-8 | 1.7, 2.2, 3.7, 5.1, 6.2 | Không gọi nền; gate S2/S3 cho payload và chi phí |
| NFR-9 | 1.2, 1.8, 1.9, 4.8, 7.4 | 8 category thống nhất và hành động sửa lỗi |
| NFR-10 | 2.1, 4.4, 7.4 | Bundle/RAM đo sớm ở spike, đo lại trên build submit |
| NFR-11 | 1.3, 2.10, 4.7, 6.4, 7.4 | Keyboard/focus/contrast light và dark |
| NFR-12 | 1.2, 2.3, 3.6, 6.1 | ID/path validation, payload không quyết định đường dẫn |

<a id="requirements"></a>

## Requirements Inventory

### Functional Requirements

*(Giữ nguyên định danh `FR-n` của PRD để truy vết với Architecture `binds: FR-1..FR-48`. Mục đánh dấu ★ là năng lực mới ở v3.)*

**4.1 Onboarding & Consent**

FR-1: Người dùng chọn ngôn ngữ UI (vi/en/ja) khi mở lần đầu; mặc định theo ngôn ngữ hệ thống, fallback `en`. Đổi ngôn ngữ áp dụng tức thì cho toàn bộ Onboarding; giá trị lưu và dùng cho Template memo mặc định, nhãn thời gian, nội dung Creative.

FR-2 ★: Màn Consent bắt buộc trước mọi tính năng gọi Gemini. Không có request nào tới Google trước khi Consent được ghi nhận (kể cả kiểm tra key). Màn nêu tên Google là bên nhận dữ liệu, có link Privacy Policy mở trình duyệt ngoài. Consent lưu bền cùng số phiên bản văn bản; chỉ hỏi lại khi phiên bản tăng. Từ chối → app chỉ xem Settings/About, có nút quay lại đồng ý. `[ASSUMPTION: một cấp Consent]`

FR-3: Nhập một hoặc nhiều Gemini API key ngay trong Onboarding; app kiểm tra bằng cách tải danh sách model và báo kết quả tại chỗ. Chấp nhận `AIza…` và `AQ.…`. Lỗi phân loại rõ: key không hợp lệ / mạng / CA công ty (proxy) / quota, kèm hướng dẫn ngắn. Key hợp lệ → hiện số model khả dụng; có "Bỏ qua, nhập sau".

FR-4: Trạng thái "chưa có key" thân thiện: mọi màn vẫn mở được; nút cần Gemini bị vô hiệu kèm lời giải thích và lối tắt tới nơi nhập key. Không màn trắng, không crash, không dialog lặp; danh sách Phiên và phát lại vẫn dùng được.

**4.2 Gemini API key & model**

FR-5 ★: API key lưu trong kho khoá OS (Keychain / Credential Manager), không nằm trong file cấu hình plaintext. File settings không chứa key; xoá key trong UI xoá khỏi kho khoá; kho khoá không truy cập được → lỗi hiểu được, không mất setting khác.

FR-6: Key pool xoay vòng theo loại lỗi: 429 → key nghỉ 60 s và thử key kế; 401/403 → loại key khỏi vòng tới khi người dùng sửa, thử key kế nếu còn; 400/404 và timeout → không xoay, báo lỗi. Một chính sách chung cho mọi luồng Gemini. Mọi key đang nghỉ → Job chờ tối đa 180 s/Chunk thay vì fail. Timeout không được gửi lại cùng request sang key khác. Hết key vì 401/403 → lỗi category Auth kèm lối tắt Settings.

FR-7: Chọn Model cho transcribe file, live và memo từ danh sách tải qua API (lọc theo năng lực) hoặc nhập tự do. Mặc định (Q10): `gemini-flash-lite-latest` cho transcribe/memo, `gemini-3.5-live-translate-preview` cho live. Danh sách live chỉ hiện model hỗ trợ Live. Tên ngoài danh sách vẫn được chấp nhận kèm cảnh báo nhẹ. Model bị Google trả lỗi category Model → cảnh báo tại chỗ + lối tắt Settings; Job/Phiên không tự đổi Model.

**4.3 Transcribe file**

FR-8: Nhận file qua dialog hệ thống hoặc kéo thả vào Home/Transcript; app chuyển sang màn Transcript và bắt đầu ngay. Chạy trong sandbox chỉ với quyền file người dùng chọn. Nhiều file → xử lý tuần tự, mỗi file một Phiên. `[ASSUMPTION: hàng đợi tuần tự]`

FR-9: Hỗ trợ audio mp3, m4a, wav, flac, ogg/vorbis, aiff, caf và video mp4/mov/mkv/webm (chỉ lấy track audio). `avi/wmv/flv/ts` bị từ chối ngay, trước khi tạo Phiên, với thông báo nêu định dạng nên chuyển sang. Track Opus trong webm/mkv: hỗ trợ hoặc từ chối rõ ràng theo kết quả spike S1 (không lỗi im lặng). Danh sách định dạng hiện trong dialog chọn file và trong thông báo lỗi.

FR-10 ★: Ngay khi Phiên được tạo, app tạo Proxy phát lại trong Container; mọi tính năng sau (phát, seek, Transcribe lại, export) dùng Proxy, không cần file nguồn. Xoá/di chuyển file nguồn không ảnh hưởng; relaunch trong sandbox vẫn phát được. Proxy hỏng/thiếu → Transcript vẫn mở, trình phát báo "không có audio" và cho chọn lại file nguồn để tạo lại.

FR-11: Phát hiện trùng theo hash nội dung file: Transcribe cùng file → mở ngay Phiên có sẵn, không gọi Gemini, không tốn token. "Chạy lại" là thao tác chủ động từ Transcript detail; giữ Tag, Ghi chú, Memo; Transcript cũ chỉ bị ghi đè sau khi bản mới hoàn tất; không tạo Phiên mới.

FR-12: Job hiển thị tiến độ theo phút audio đã xử lý trên tổng, có nút huỷ, có panel log; rời màn rồi quay lại vẫn thấy Job. Huỷ → ngừng gửi Chunk mới ≤ 2 s, dọn dữ liệu tạm, Phiên chưa hoàn chỉnh không được lưu. Home hiển thị Job đang chạy (FR-27). Đóng app khi Job chạy → hỏi xác nhận; tiếp tục đóng thì Job huỷ sạch (không chạy nền).

FR-13: Cắt Chunk `chunkMinutes` (mặc định 5, tối thiểu 1); Segment trả về theo `MM:SS` tương đối Chunk và được cộng offset tuyệt đối; người dùng đặt được offset timestamp toàn cục (giây) áp dụng lúc hiển thị/export. Segment không bao giờ lùi thời gian; Chunk quá lớn để gửi inline → tự chia nhỏ hơn (ví dụ 3 phút) và ghi log.

FR-14: Transcribe file dùng model Gemini generateContent thông thường, nhận JSON Segment 5–15 s và tự nhận diện tiếng Việt/tiếng Nhật xen kẽ mà không dịch. Live dùng model Live Translate riêng để nhận input transcript, output transcript và audio dịch. Không dùng model `*-transcribe` cho luồng file v3.

FR-15: Chịu lỗi từng Chunk, không giấu Khoảng thiếu: mỗi Chunk thử tối đa 4 lần theo chính sách Key pool; Chunk thất bại hẳn thành Khoảng thiếu; Transcript Partial hiển thị cảnh báo mốc thiếu và **không** được lưu như hoàn chỉnh. UI liệt kê từng Khoảng thiếu, cho "Chạy lại phần thiếu" (chỉ Chunk thiếu) hoặc "Chạy lại toàn bộ". Phiên Partial có badge `partial` ở Home. Không bao giờ ghi đè Transcript live bằng Transcript Partial. 5xx thử lại trong 4 lần; 400/404 hoặc nội dung bị chặn → fail ngay. JSON vỡ/cắt cụt → parser cứu Segment hợp lệ, phần không cứu được thành Khoảng thiếu.

FR-16: Chọn ngôn ngữ transcribe (auto/ja/vi/en, mặc định auto) trong Settings; áp dụng cho Transcribe file và Live.

**4.4 Live transcribe & dịch**

FR-17: Chọn Nguồn audio `system`, `mic:<tên>` hoặc `mixed:<mic>` trước khi bắt đầu; dropdown mic (preselect mặc định, nút làm mới); đổi Nguồn giữa phiên không ngắt Phiên, có hiệu lực ≤ 1 s, đồng hồ và Recording liên tục. macOS chỉ xin "System Audio Recording" + Micro, không bao giờ Screen Recording; âm thanh TTS của app không bị thu lại. Windows thu cả app họp native (endpoint communications) lẫn họp trong trình duyệt (endpoint console). Chấp nhận: thu được Zoom, Teams, Google Meet trong Chrome trên cả hai OS. Thiếu quyền → hướng dẫn cấp quyền trong app với đường dẫn System Settings, không crash; Phiên chưa bắt đầu tới khi có quyền hoặc chọn Nguồn khác.

FR-18: Transcript realtime stream token-by-token, tách câu khi gặp dấu kết câu, timestamp theo đồng hồ capture cục bộ; Segment tích luỹ được lưu tạm định kỳ để phục hồi. Segment mới hiện ≤ 2 s sau khi câu kết thúc (mạng bình thường). Tự cuộn theo mới nhất, dừng khi người dùng cuộn lên. `[ASSUMPTION: ngưỡng 2 s]`

FR-19: Dịch realtime với Target đổi được giữa phiên; view Gốc / Dịch / Cả hai; Target mặc định từ Settings; dropdown có "Không dịch". Speech đã ở Target vẫn hiện trong pane Dịch; Target được chọn thì luôn dịch kể cả khi ngôn ngữ nguồn khai báo trùng. Bản dịch **không** lưu vào Phiên. Đổi Target không làm gián đoạn Transcript gốc; Bản dịch mới bắt đầu từ câu kế. "Không dịch": không yêu cầu model sinh Bản dịch hay audio TTS, view chỉ còn Gốc, nút TTS vô hiệu.

FR-20: Nhận diện lại ngôn ngữ (chỉ khi ngôn ngữ transcribe là `auto`): mở kết nối model mới với ngữ cảnh trống, giữ Phiên, Recording, đồng hồ, Transcript. Không Segment lặp hay nhảy timestamp; sự kiện từ kết nối cũ sau thời điểm chuyển bị bỏ; kết nối cũ đóng ≤ 1 s sau khi mới sẵn sàng; thất bại → giữ kết nối cũ, báo lỗi nhẹ. Nút ẩn/vô hiệu khi ngôn ngữ ≠ auto.

FR-21: *(Chủ sản phẩm xác nhận 2026-09-22: nguồn giọng đọc **duy nhất** là audio do chính model Gemini Live Translate trả về; không dùng model TTS local hay dịch vụ TTS cloud nào.)* Đọc bản dịch (TTS) phát trong tiến trình app theo thiết bị output mặc định (kể cả đổi tai nghe giữa chừng); Ducking hạ volume hệ thống còn 30 % khi model nói và phục hồi sau. TTS không bị chính app thu lại. Màn Live có chỉ báo "đang đọc" bật/tắt theo lúc TTS phát; audio TTS không đi qua lớp UI. Người dùng tự chỉnh volume khi đang Ducking → app bỏ mức gốc đã lưu, giữ mức mới. Crash khi Ducking → lần mở sau phục hồi volume. Đổi thiết bị output → phát tiếp ≤ 2 s; Bluetooth tắt đột ngột → phát hiện và chuyển ≤ 3 s.

FR-22: Kết nối bền, trong suốt với người dùng: duy trì qua giới hạn thời gian phía server (resumption, nén ngữ cảnh, reconnect khi `goAway`) và mọi lần mất kết nối phía máy (mất mạng, chập chờn, đổi Wi-Fi, ngủ máy). Backoff luỹ tiến 1 s → 2 s → 4 s … tối đa 30 s, có jitter, **không giới hạn số lần** khi Phiên còn chạy; audio chưa ack được gửi lại. Phiên 60 phút với ≥ 6 lần reconnect không mất Segment. Ngưỡng 5 lần liên tiếp chỉ cho lỗi server từ chối setup (payload sai, key bị từ chối, model không tồn tại) → báo lỗi, cho chọn "tiếp tục chỉ ghi âm" hoặc dừng; Phiên vẫn lưu được. Audio thu khi mất kết nối được giữ tối đa 60 s; vượt quá → Transcript live ghi Khoảng thiếu "mất kết nối mm:ss–mm:ss" (audio vẫn có trong Recording). Chỉ báo kết nối phân biệt đang kết nối / đã kết nối (đang transcribe) / đang nối lại + thời gian chờ / đã dừng transcript; luôn riêng biệt với chỉ báo "đang ghi âm". Lý do lỗi được lọc (không lộ key/URL/transcript); UI không bao giờ hiện "đang transcribe" khi không còn kết nối sống.

FR-23: Recording WAV ghi liên tục vào Container, header vá định kỳ (~5 s) để luôn phát được; dừng → finalize và tạo Proxy. Recording và đồng hồ Phiên độc lập với mạng/key/lỗi Gemini; chỉ người dùng bấm Dừng, lỗi thiết bị audio hoặc lỗi lưu trữ không thể phục hồi mới dừng. Force-quit → file vẫn phát được, phần audio cuối không phát được dài tối đa 5 s trước khi tiến trình bị kết thúc. Phiên + file Recording được tạo khi mở capture thành công, kể cả offline; lỗi trước khi capture mở → dọn sạch, không để dòng Phiên hay file chỉ có header.

FR-24: Khi khởi động, quét Phiên live chưa finalize, đưa vào thư viện với Recording và Segment đã tích luỹ, tạo Proxy nếu thiếu. Badge "phục hồi", cho Transcribe lại; không hỏi người dùng; chạy nền, không chặn Home.

FR-25: Đóng app/cửa sổ khi Phiên live đang chạy → hỏi xác nhận; nếu tiếp tục, app giữ tiến trình đủ lâu để finalize Recording và lưu Phiên. Không mất Phiên khi đóng bình thường.

FR-26: Khi dừng Live: overlay "Đang lưu phiên…" (finalize Recording, tạo Proxy) rồi điều hướng sang Transcript detail; Phiên live cùng schema với Phiên file, tên mặc định là nhãn thời gian cục bộ. Transcribe lại từ Recording là một Job (tuân FR-12, FR-15), kết quả hiển thị cạnh bản live, giữ Memo/Tag/Ghi chú; bản live không bị xoá khi Transcribe lại Partial.

**4.5 Home — thư viện Phiên**

FR-27: Danh sách Phiên mới nhất trước; mỗi dòng: tên, ngày (múi giờ cục bộ), loại (file/live), thời lượng, Tag, badge partial / phục hồi (đứng ngay sau tên vì đòi hành động). **Không hiển thị Model và số Segment ở dòng phiên** (chủ sản phẩm chốt 2026-09-20 theo EXPERIENCE.md; badge memo / có audio chỉ hiện ở Transcript detail). Job đang chạy hiện ở đầu với tiến độ và lối quay lại. 500 Phiên hiển thị ≤ 1 s. Xoá Phiên đang có Job bị chặn tới khi Job xong/huỷ. `[ASSUMPTION: ngưỡng 500/1 s]`

FR-28: Tìm theo tên Phiên với nút xoá query; kết hợp lọc Tag. Khớp không phân biệt hoa thường và khoảng trắng thừa; cập nhật khi gõ ≤ 200 ms với 500 Phiên. Query + lọc Tag là AND; xoá query giữ lọc Tag. Không kết quả → trạng thái rỗng có nút xoá bộ lọc.

FR-29: Tag: nhiều Tag/Phiên (≤ 20, mỗi Tag ≤ 80 ký tự), chuẩn hoá trim + bỏ trùng không phân biệt hoa thường; lọc nhiều Tag (AND); lọc "Chưa gắn tag"; quick-picker Tag có sẵn; chế độ quản lý Tag để xoá một Tag khỏi mọi Phiên (một thao tác, có xác nhận). Tag tồn tại qua đổi tên, Transcribe lại, chạy lại.

FR-30: Đổi tên inline (rỗng bị từ chối, ≤ 200 ký tự, Esc huỷ, Enter lưu); xoá Phiên xoá toàn bộ dữ liệu liên quan (Transcript, Proxy, Recording, Memo, Ghi chú, liên kết Tag) khỏi Container sau xác nhận, không để file mồ côi; Tag không còn Phiên vẫn tồn tại tới khi người dùng xoá.

FR-31: Với Phiên live, lưu Recording ra ngoài dạng WAV hoặc FLAC qua dialog lưu (mục trong menu ⋯ của dòng Phiên). Không có lựa chọn M4A/AAC.

**4.6 Transcript detail**

FR-32: Trình phát dùng Proxy phát lại; click Segment → nhảy tới `start`; Segment đang phát được highlight; seek mượt trên macOS và Windows; seek tới bất kỳ điểm nào trong file 90 phút ≤ 500 ms. `[ASSUMPTION: ngưỡng; phụ thuộc spike S8]`

FR-33: Tìm trong Transcript: đếm match `n/N`, prev/next vòng tròn (Enter = next), highlight và cuộn tới match; không phân biệt hoa thường; Transcript 90 phút (~700 Segment) phản hồi ≤ 100 ms.

FR-34: Hiển thị Transcript: Segment có timestamp (đã cộng offset), `HH:MM:SS` (bỏ giờ nếu < 1 h); speaker không hiển thị ở v3. Phiên có hai Transcript (live + Transcribe lại) xem cạnh nhau hoặc chọn một; ở chế độ cạnh nhau click Segment ở cột nào cũng seek trình phát; export/copy lấy Transcript đang chọn.

FR-35: Export `.txt`, `.srt`, `.json` qua dialog lưu hệ thống; copy toàn bộ Transcript vào clipboard. Export áp dụng offset; `.json` giữ speaker; Phiên Partial export kèm ghi chú Khoảng thiếu trong `.txt`. `[ASSUMPTION]`

**4.7 Ghi chú & Memo**

FR-36: Ghi chú tự lưu (debounce ~800 ms) theo Phiên, có sẵn trong Live và Transcript detail; không mất Ghi chú khi force-quit trong Live (lưu trước khi Phiên finalize).

FR-37: Template memo: thêm/sửa/xoá/khôi phục mặc định; mỗi Template có tên và prompt chứa `{transcript}` bắt buộc, `{notes}` tuỳ chọn; bộ mặc định theo ngôn ngữ UI. Thiếu `{transcript}` bị từ chối kèm giải thích; khôi phục mặc định không xoá Template của người dùng.

FR-38: Sinh, cache, sinh lại Memo: chọn Template, Memo lưu theo Phiên và Template; hiển thị Markdown đã sanitize; copy và tải `.md`. Ghi chú nhúng vào prompt với header theo ngôn ngữ UI khi Template có `{notes}`. Lỗi Memo không ảnh hưởng Transcript; lỗi phân loại (quota/auth/mạng) hiển thị rõ. Memo cũ vẫn hiện khi Transcript chạy lại, kèm nhãn "Memo sinh từ bản trước". `[ASSUMPTION]`

**4.8 Settings, lưu trữ, chẩn đoán, About**

FR-39: Các nhóm setting: ngôn ngữ UI, theme (Theo hệ thống / Sáng / Tối); key (FR-5), model (FR-7), ngôn ngữ transcribe (FR-16); chunk phút, offset (FR-13); Target mặc định; Template memo (FR-37). Mọi trường có tooltip; giá trị không hợp lệ bị chặn tại chỗ; ô key mặc định ẩn ký tự, có nút hiện/ẩn và "Kiểm tra key". Không còn nhóm engine, Whisper, Copilot, Hệ thống/wizard, Updates, thư mục cache.

FR-40 ★: Lưu trữ trong Container: Settings hiển thị dung lượng (media, DB), nút mở thư mục (nếu OS cho phép), "Xoá toàn bộ dữ liệu" xác nhận hai bước. Không có tuỳ chọn thư mục cache (Q7); không tự dọn Phiên cũ. `[ASSUMPTION]`

FR-41: Nhật ký chẩn đoán content-free, xoay vòng; xuất gói qua allow-list; xoá được. Không bao giờ chứa Transcript, Bản dịch, Ghi chú, Memo, key; mọi chuỗi lỗi qua redaction (`AIza…`, `AQ.…`, URL, `authorization`). Test tự động quét log sau Phiên mẫu. Đếm cục bộ số Phiên, số lỗi theo category, crash (không nội dung), hiển thị trong mục Chẩn đoán; **không gửi thống kê đi đâu, không có opt-in.**

FR-42: Đồng bộ cấu hình đề xuất (tuỳ chọn): tải bộ cấu hình (model, chunk, template) từ endpoint của chúng ta; nội dung được ký và xác minh; không bao giờ ghi đè key; hiển thị diff trước khi áp dụng. `[ASSUMPTION: preview diff]`

FR-43: About & Privacy: version, tác giả, liên hệ hỗ trợ, link Privacy Policy, link mở lại màn Consent để xem lại.

**4.9 Quảng cáo tự vận hành (Ad slot)**

FR-44 ★: Ad slot hiển thị một Creative (300×100 hoặc 320×50) theo locale UI và thời hạn; click mở trình duyệt ngoài; xoay theo trọng số; mỗi Creative tối đa một lần hiển thị/10 phút. **Không hiển thị ở màn Live** (ẩn theo route). Vị trí đáy sidebar 260 px. Không có Creative hợp lệ và offline → creative mặc định nhúng sẵn (giới thiệu trans-kun/Relipa). Không che nội dung, không âm thanh, không interstitial.

FR-45 ★: Tuân thủ store cho quảng cáo: nhãn "Sponsored", nút "Báo cáo quảng cáo" (mở URL/mailto có sẵn `id` Creative), "Vì sao tôi thấy quảng cáo này" (text tĩnh theo ngôn ngữ UI). Không identifier, cookie, đo lường cá nhân; impression/click chỉ đếm cục bộ. Cờ `ads_enabled` từ server và `is_premium` cục bộ (chưa có UI) ẩn toàn bộ Ad slot.

FR-46 ★: Toàn vẹn Creative: `ads.json` được ký; chữ ký sai → bỏ qua, dùng cache/fallback; ảnh ≤ 100 KB, tải và cache trong Container, không tải script.

**4.10 Đa ngôn ngữ UI**

FR-47: Bộ key i18n đồng bộ: vi/en/ja cùng tập key; thiếu key ở bất kỳ ngôn ngữ nào bị chặn ở CI; đổi ngôn ngữ áp dụng tức thì, không cần khởi động lại.

FR-48 ★: Theme sáng và tối ngay trong bản submit đầu; mặc định theo hệ thống, ép được trong Settings → Chung; đổi tức thì và theo OS khi ở chế độ hệ thống; mọi cặp màu mang nội dung đạt WCAG AA ở cả hai theme.

### NonFunctional Requirements

NFR-1 Riêng tư: Dữ liệu họp chỉ đi tới Google Gemini bằng key người dùng. Không telemetry dưới bất kỳ hình thức nào (kể cả opt-in); chỉ đếm cục bộ. Không server trung gian. Nhật ký chẩn đoán content-free.

NFR-2 Không chặn lõi: Lỗi Memo, Proxy phát lại, Ad slot, đồng bộ cấu hình không được làm hỏng Transcript hay Recording. Mọi tác vụ dài có cancel, progress, lỗi có category ổn định để UI dịch được.

NFR-3 Bền phiên: Crash, force-quit, mất mạng, chập chờn, server đóng kết nối không làm mất Recording và Segment đã tích luỹ; Recording không bao giờ dừng vì lý do mạng.

NFR-4 Không phụ thuộc ngoài: Không binary ngoài, không tải/chạy code lúc chạy, không quyền admin, không self-update. Một tiến trình.

NFR-5 Hiệu năng: Decode + cắt Chunk ≥ 20× realtime trên máy 4 nhân; app mở tới Home ≤ 2 s; Live: độ trễ Transcript ≤ 2 s sau kết câu trong mạng bình thường. `[ASSUMPTION: các ngưỡng; đo ở Phase 0/2]`

NFR-6 Mạng doanh nghiệp: Dùng CA store hệ thống (proxy/Zscaler); lỗi TLS/CA có thông báo riêng, hướng dẫn được.

NFR-7 Sandbox & store: Chạy trong App Sandbox (macOS) và package identity (MSIX) với entitlement/capability tối thiểu: network client, mic, file do người dùng chọn. Không xin quyền Screen Recording.

NFR-8 Chi phí Gemini: Chỉ ba luồng tốn token: transcribe file, live, memo. Không gọi nền định kỳ. Chunk gửi inline, không upload trung gian.

NFR-9 Khả năng hiểu lỗi: Mọi lỗi người dùng thấy thuộc một category hiển thị (quota/auth/model/mạng/CA/định dạng/quyền hệ thống/lưu trữ) với hành động gợi ý; bao trùm cả lỗi ngoài Gemini; không stack trace, không lộ key/URL.

NFR-10 Kích thước & tài nguyên: App bundle ≤ 60 MB mỗi nền tảng; RAM khi Live ≤ 300 MB. `[ASSUMPTION: ngưỡng; đo ở Phase 0/2]`

NFR-11 Tiếp cận cơ bản: Điều hướng bàn phím cho các thao tác chính (bắt đầu/dừng Live, tìm kiếm, export); tương phản WCAG AA cho text ở cả theme sáng và tối. `[ASSUMPTION: mức tối thiểu]`

NFR-12 An toàn đường dẫn: Không dùng chuỗi do người dùng hoặc server cung cấp (tên Phiên, Tag, tên Template, id Creative) làm thành phần đường dẫn; mọi file trong Container đặt tên theo ID tự sinh.

### Additional Requirements

*(Từ Architecture Spine, addendum PRD và — khi không mâu thuẫn — bộ tham khảo docs/rebuild-v3. Đánh số AR-n để truy vết khi lập story.)*

**Nền tảng / starter**

- AR-1 **Không có starter template được chỉ định.** Architecture yêu cầu **repo mới** (greenfield) dựng theo *Structural Seed*: Tauri 2 + Vite + TypeScript 6 + Svelte 5 SPA một cửa sổ; `src/` (lib/stores, keymap.ts, bindings.ts sinh bởi tauri-specta, routes, components, i18n/{vi,en,ja}.json) và `src-tauri/src/` (ipc, core, settings, library, transcribe, live, memo, ads, gemini, media, audio, db, secrets, remote) + `tauri.conf.json`, `tauri.appstore.conf.json`, `tauri.msix.conf.json`, `.github/workflows/` (test mac+win, store-mac, store-win). → **Epic 1 Story 1 = dựng repo theo Structural Seed** (build được trên cả hai OS, binding tauri-specta, CI chạy test hai OS).
- AR-2 Bảng Stack có version được pin (Tauri 2.11.5, tokio 1.53.1, rusqlite 0.40.2 bundled + rusqlite_migration 2.6.0, keyring 4.2.0, reqwest 0.13.5 `rustls` platform verifier, tokio-tungstenite 0.30.0 `rustls-tls-native-roots`, symphonia 0.6.1, rubato 5.0.0, flacenc 0.5.1, hound 3.5.1, cpal 0.18.2, wasapi 0.24.0, objc2 0.6.4, coreaudio-sys 0.2.18, tracing 0.1.44, uuid 1.26.1 v7, sha2 0.11.0, ed25519-dalek 3.0.0, Svelte 5.57.0, Vite 8.3.0, TypeScript 6.0.3, marked 18.0.13, dompurify 3.4.15, vitest 5.0.1…). Dependency pin trong `Cargo.lock`/`package-lock.json`; RC (tauri-specta/specta 2.0.0-rc.25) pin **exact**.
- AR-3 Cấm: Python, ffmpeg, sidecar, `tauri-plugin-{shell,fs,updater,process,http,store}`. Thêm plugin/entitlement/capability mới = sửa Architecture Spine.
- AR-4 Port `audio/` từ v2 (cpal 0.16 → 0.18): capture, WASAPI dual loopback, playback supervisor, output_volume/ducking.
- AR-5 Frontend: chốt ở story nền tảng — thư viện i18n (`i18next` hay tự viết) và router client; migrate ≈ 400 key i18n từ v2 (bỏ nhóm setup/whisper/copilot); `marked` + `dompurify` qua npm; font IBM Plex bundle trong app.
- AR-6 Version một nguồn (`Cargo.toml` → `tauri.conf.json`), semver; migration DB chỉ tiến.

**Phase 0 — Spike bắt buộc trước khi cam kết kiến trúc (ADR cho từng spike)**

- AR-7 S1 Media thuần Rust: symphonia decode mp3/m4a/mp4/mov/mkv/webm/wav/flac (10 file mẫu, ≥ 20× realtime, chunk FLAC < 14 MB); quyết định Opus.
- AR-8 S2 Gemini inline: kiểm chứng `generateContent` với model tổng quát được chọn nhận `inline_data audio/flac` 5 phút, responseSchema và payload dưới giới hạn. Không triển khai Interactions/Files API cho transcribe file.
- AR-9 S3 Live WS Rust: port setup/resumption/goAway/reconnect sang tokio-tungstenite; chạy 60 phút liên tục không mất chunk.
- AR-10 S4 Core Audio process tap: global tap trong app Tauri **sandboxed**, loại trừ PID chính app, chạy cùng cpal mic; ghi được Zoom/Teams/Meet; prompt quyền "System Audio Recording Only"; không loop TTS; kiểm trên ≥ 3 máy (Intel, M-series, 14.4 và 15.x).
- AR-11 S5 MAS build: hello-world Tauri với entitlements sandbox + provisioning profile, `productbuild`, TestFlight for Mac; kiểm `keyring` 4 trong sandbox (entitlement `keychain-access-groups`).
- AR-12 S6 MSIX: `winapp`/`tauri-windows-bundle` cho x64 + Arm64, cài/gỡ sạch trên máy sạch, WACK pass, chiến lược WebView2.
- AR-13 S8 Proxy phát lại: `<audio>` phát FLAC trong WKWebView & WebView2 qua asset protocol với range request, seek mượt; nếu không → AAC native (AudioToolbox/Media Foundation) hoặc player trong Rust. *(S7 IAP không thuộc v3.)*
- AR-14 Spike phụ: tauri-specta rc.25 với Tauri 2.11 và `Channel<T>` typed; nếu gãy, bọc Channel bằng type viết tay (AD-3 giữ nguyên).

**Quy tắc kiến trúc (AD-1…AD-19)**

- AR-15 (AD-1) Phụ thuộc chỉ đi `ipc → feature → hạ tầng → core`; feature không import feature khác, dữ liệu chung đi qua repo `db/`; điều phối chéo feature chỉ ở `ipc/` (`session_delete` hỏi JobRegistry/LiveSession, `live_stop` → finalize → trả `session_id`, Transcribe lại lấy Recording từ repo đưa vào JobRegistry).
- AR-16 (AD-2) Actor (tokio task sở hữu state; `mpsc` lệnh + `oneshot` trả lời; có `is_busy(session_id)`) **chỉ** cho `LiveSession`, `JobRegistry`, `KeyPool`; không `Arc<Mutex<_>>` bọc state của chúng. Port (trait, có bản giả) chỉ ở ba biên: transport Gemini, nguồn audio, `KeyProvider`.
- AR-17 (AD-3) `live_subscribe(channel)`/`jobs_subscribe(channel)` trả `Snapshot { seq, … }` rồi stream qua `Channel`; `seq` đơn điệu theo instance stream; UI thấy hụt `seq` → subscribe lại; Rust bỏ Channel khi `send` lỗi; UI không bao giờ là nguồn sự thật; `emit` toàn cục chỉ cho thông báo tần suất thấp.
- AR-18 (AD-4) Một connection SQLite (WAL) do `db/` giữ, truy cập tuần tự; SQL chỉ trong `db/repo/<entity>.rs`; migration chỉ trong `db/migrations`. `sessions.status ∈ {recording, finalizing, complete}`; Segment live flush định kỳ vào `segments` là nguồn duy nhất để phục hồi; boot: Phiên còn `recording|finalizing` → `complete` + `recovered = true`.
- AR-19 (AD-5) ID Phiên/Transcript/Job = UUIDv7; `sessions.source_hash` (SHA-256 file nguồn) là cột unique riêng; file `media/<session-id>/<role>.<ext>` (role `proxy|recording`), `media/.staging/<job-id>/`, `cache/remote/<sha256-của-url>`, `state/ducking.json`.
- AR-20 (AD-6) Mọi request REST/WS tới Google đi qua `gemini/` (dùng `KeyProvider` → `KeyPool`, bộ phân loại lỗi chung); `gemini/` từ chối mọi request khi chưa có Consent phiên bản hiện hành (kể cả list models); `KeyPool` cấp key theo lớp ưu tiên `Live > Job > Memo` (Live nhận 429 → Job ngừng gửi Chunk mới tới khi cooldown hết); không gọi nền/timer; tham số định nghĩa một chỗ trong `gemini::params`.
- AR-21 (AD-7) Mọi lỗi qua IPC là `AppError { category, code, detail_redacted }`; `category ∈ {quota, auth, model, network, format, permission, storage, blocked}`; `code` = phân loại API (`Quota|Auth|Model|Request|Timeout|Network|Blocked|Shape`) + `Tls`; ánh xạ `code → category` chỉ ở `core/error`; UI dịch theo category/code, không hiển thị `detail`.
- AR-22 (AD-8) Settings trong bảng `settings` SQLite, chỉ ghi qua `settings/`; API key chỉ ở kho khoá OS qua `secrets/`; phiên bản Consent là hằng số compile-time đi cùng văn bản Consent; cấu hình đề xuất: `remote/` chỉ tải + xác minh, `settings/` tính diff (không gồm key/Consent) qua `settings_recommended_preview` và áp qua `settings_recommended_apply`.
- AR-23 (AD-9) Đồng hồ Phiên live = số sample đã capture ÷ sample rate (không phải wall clock); `Segment.start/end` = giây `f64` tuyệt đối; `created_at/updated_at` = epoch ms UTC; wall clock chỉ cho bộ đếm "đang nối lại (m:ss)"; offset (FR-13) và định dạng `HH:MM:SS` chỉ ở UI/export.
- AR-24 (AD-10) Topology Live: Phiên bắt đầu (tạo dòng `sessions` + file Recording) khi mở capture thành công kể cả offline; capture fan-out tới (1) WAV writer và (2) WS sender với ring buffer 60 s, không nhánh nào block capture; audio bị đẩy khỏi buffer thành gap `disconnected`; restart dùng `LiveGeneration { id, … }` (event mang id cũ bị bỏ, generation cũ drain ≤ 1 s); `LiveEvent` tách `recording { state }` và `connection { connecting | connected | reconnecting { sinceMs } | stopped }`; TTS đi thẳng `audio::playback`, UI chỉ nhận `speaking: bool`; đổi Nguồn audio là gate mềm trong capture.
- AR-25 (AD-11) `JobRegistry` giữ **một** hàng đợi tuần tự cho Transcribe file, Chạy lại, Transcribe lại; tối đa một `LiveSession` song song với Job; memo là request đơn lẻ, không vào hàng đợi; `transcribe_start` tính `source_hash` trước — trùng → `Existing { session_id }`, không tạo Job, không gọi Gemini; Job không sống qua restart (không có bảng `jobs`); `JobEvent` có `waitingQuota`.
- AR-26 (AD-12) Trình phát dùng asset protocol (`app.security.assetProtocol`) với scope **chỉ** `$APPDATA/media/**`; dữ liệu khác tới UI chỉ qua command.
- AR-27 (AD-13) Mỗi domain một store `src/lib/stores/<domain>.svelte.ts` (runes) bọc binding tauri-specta và thực thi AD-3; component không gọi `invoke`/binding trực tiếp; hiện/ẩn Ad slot chỉ quyết định ở layout theo route; phím tắt chỉ trong app (không global shortcut OS) đăng ký ở `src/lib/keymap.ts`; token theo DESIGN.md; dark mode bắt buộc.
- AR-28 (AD-14) `ads.json`, ảnh Creative, `recommended-settings.json` chỉ tải qua `remote/`: xác minh ed25519 bằng public key nhúng binary, cache theo AD-5, chữ ký sai → cache/fallback nhúng sẵn; không tải script/HTML; ảnh ≤ 100 KB; `remote/` không ghi settings.
- AR-29 (AD-15) Transcript, Bản dịch, Ghi chú, Memo, prompt, key bọc `core::Sensitive<T>` (`Debug`/`Display` = `[redacted]`); `tracing` có layer redaction (`AIza…`, `AQ.…`, URL, `authorization`); test chạy Phiên mẫu rồi grep log; xuất Nhật ký qua allow-list; bộ đếm chỉ cục bộ.
- AR-30 (AD-16) `sessions` 1–n `transcripts (variant primary|retranscribe, status complete|partial)`; Tag/Ghi chú/Memo gắn **session**; Khoảng thiếu = `segments.kind = gap` với `gap_reason ∈ {chunk_failed, disconnected}` dùng chung file/live; Transcript `partial` ⇔ có gap `chunk_failed`; Chạy lại dựng transcript mới rồi swap nguyên tử thay `primary`; Transcribe lại tạo `retranscribe`, không bao giờ thay bản live; Job file làm việc trong `media/.staging/<job-id>/`, commit dữ liệu và tham chiếu media trong một transaction SQLite, publish file theo giao thức phục hồi ở story 2.3, huỷ → xoá staging.
- AR-31 (AD-17) Một tiến trình, không subprocess/sidecar, không tải/chạy code; macOS chỉ Core Audio process tap (không ScreenCaptureKit/Screen Recording); entitlement/capability chỉ gồm: sandbox, network client, audio input, file người dùng chọn, keychain access group (macOS); `microphone` (MSIX).
- AR-32 (AD-18) Boot là một routine trong `ipc/`: migrate DB → phục hồi volume từ marker Ducking → dọn `media/.staging` → phục hồi Phiên mồ côi (nền, không chặn Home). Đóng cửa sổ do một handler trong `ipc/`: có Live → xác nhận → finalize rồi thoát; có Job → xác nhận → huỷ sạch; cả hai → finalize Live và huỷ Job.
- AR-33 (AD-19) Lỗi proxy, memo, ads, remote không bao giờ đổi `status` của transcript hay dừng Recording; Proxy lỗi → Phiên vẫn lưu, UI đi nhánh FR-10; finalize Live lỗi → Phiên vẫn lưu với Recording đã có.

**Quy ước nhất quán**

- AR-34 IPC: command `snake_case` `<domain>_<action>`; binding TS sinh bởi tauri-specta (không viết tay); serde `rename_all = "camelCase"`, enum tag `{ type, … }`; `Result<T, AppError>` ở mọi command; event `LiveEvent`/`JobEvent` (ready, delta, turn, segment, delta_translated, segment_translated, gap, recording, connection, speaking, log, error, done, final; progress, log, waitingQuota, result, error, cancelled) mọi event có `seq`.
- AR-35 Frontend: route `/onboarding`, `/home`, `/session/:id`, `/live`, `/settings/:group`; i18n key `<màn>.<khối>.<nhãn>`, 3 ngôn ngữ cùng key set, CI chặn thiếu; mỗi feature Rust có `mod.rs` public tối thiểu; tham số vận hành là hằng số một chỗ (`gemini::params`, `live::params`).
- AR-36 Test: port giả cho Gemini/audio/KeyProvider; test khoá exact JSON shape request Gemini; test grep log; `cargo test` chạy cả macOS và Windows trong CI; Rust test cho key pool, parser JSON/segment, chunker, WAV header, live event state machine, sanitizer; Vitest cho router, i18n key set, tag normalize; e2e WebDriver smoke cho luồng chính; smoke thủ công theo checklist store trên máy sạch cả hai OS trước mỗi submit.

**Gemini, media, audio (addendum §D–F)**

- AR-37 Gọi REST/WS thuần, không SDK. REST `POST /v1beta/models/{model}:generateContent`, header `x-goog-api-key`, chunk `inline_data {mime_type: audio/flac}` (giới hạn inline 20 MB đã tính base64 → chia nhỏ tiếp nếu > 14 MB, kiểm kích thước toàn payload theo story 2.1), `responseMimeType: application/json` + `responseSchema` mảng `{start:"MM:SS", end:"MM:SS", text}`. Thinking config chỉ áp dụng cho model ID đã kiểm chứng; alias/tên tự nhập bỏ cấu hình thinking thay vì đoán major version. Live Translate dùng WebSocket riêng ở AR-38. Test khoá exact JSON shape; test với key `AIza…` và `AQ.…`.
- AR-38 Live WS `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=…`: `inputAudioTranscription`/`outputAudioTranscription` ở **top-level setup** (không trong `generationConfig`); `responseModalities: ["AUDIO"]`, `translationConfig {targetLanguageCode, echoTargetLanguage: true}`; `sessionResumption` + `contextWindowCompression.slidingWindow`; audio `realtimeInput.audio {data: base64 PCM16, mimeType: "audio/pcm;rate=16000"}` mỗi 100 ms; `goAway` → reconnect trong suốt; resend chunk chưa ack. Tắt dịch = setup không có `outputAudioTranscription`, target = source (hoặc `ja` khi auto), không `echoTargetLanguage`.
- AR-39 Reconnect: mọi lỗi transport (timeout, EOF, DNS, TLS, close code ≠ setup-reject) → backoff `min(30 s, (1 s × 2^n) × (1 + jitter))`, jitter trong ±20 %, không giới hạn số lần, reset `n` sau `setupComplete`; `MAX_FAILURES=5` chỉ đếm server từ chối setup (1007/1008 trước `setupComplete`, 401/403/404 ở handshake); ring buffer audio pending 60 s (600 chunk 100 ms); capture → WAV writer không đi qua task WS; phát hiện mạng có lại bằng reconnect theo backoff, không poll OS network API.
- AR-40 Tham số vận hành: chunk max attempts 4; max wait 180 s/chunk; quota cooldown 60 s; timeout transcribe chunk 120 s, memo 90 s; timeout không fan-out; sanitizer redaction cho close reason.
- AR-41 Media pipeline: duration = symphonia `n_frames / sample_rate` (fallback decode đếm); demux track audio đầu tiên → decode → f32 mono streaming; buffer 16 kHz mono theo `chunk_seconds` → FLAC bytes; Proxy FLAC 16 kHz mono (≈ 55 MB/giờ; tuỳ chọn `aac-native` nếu S8 thất bại); Recording WAV 16 kHz mono PCM16 vá header mỗi ~160 000 byte; hash Phiên SHA-256 của file nguồn; `avi/wmv/flv/ts` không hỗ trợ.
- AR-42 Audio: macOS Core Audio process tap **global** loại trừ PID chính app, `NSAudioCaptureUsageDescription` + `NSMicrophoneUsageDescription`, không có API hỏi quyền trước (hệ thống hỏi khi tạo tap lần đầu), binary phải ký, TCC khoá theo Team ID → dev build cũng ký bằng cert team; Windows WASAPI loopback cả endpoint console + communications rồi cộng (cùng device → capture một lần); playback TTS trong Rust (cpal), supervisor poll thiết bị output mặc định mỗi 1 s, stall 2.5 s coi như chết; ducking 30 % với marker `state/ducking.json`; đổi Nguồn audio = gate mềm.

**Dữ liệu (addendum §G)**

- AR-43 Schema (cột chính xác chốt ở story `db/`): `sessions(id, kind file|live, title, source_hash UNIQUE NULL, source_name, status, recovered, created_at, duration_sec)`, `transcripts(id, session_id, variant, status, model, language, created_at)`, `segments(transcript_id, idx, start, end, kind text|gap, gap_reason NULL, text, speaker NULL)`, `tags(id, name UNIQUE COLLATE NOCASE)`, `session_tags`, `notes(session_id PK, body, updated_at)`, `memos(session_id, template_id, body, created_at)`, `memo_templates(id, name, prompt, is_default)`, `settings(key, value)`. Không có bảng `jobs`. Một chủ sở hữu schema (Rust); không import v2 (Q8).

**Store, build, phân phối (addendum §J, §I; doc 03 khi không mâu thuẫn)**

- AR-44 `store-mac`: overlay `tauri.appstore.conf.json` (entitlements: app-sandbox, network.client, device.audio-input, files.user-selected.read-write, keychain-access-groups nếu `keyring` cần; application-identifier `B2U85XPU55.com.transkun.app`; provisionprofile; `bundle.category`; `minimumSystemVersion` 14.4); universal; không updater; `ITSAppUsesNonExemptEncryption=false`; `productbuild` ký "3rd Party Mac Developer Installer"; upload `xcrun altool`; TestFlight for Mac. Kiểm thử sandbox thật (`codesign -d --entitlements`, chạy từ `/Applications`, mở file bằng dialog, relaunch xem phát lại được).
- AR-45 `store-win`: `winapp` CLI hoặc `tauri-windows-bundle` → MSIX x64 + Arm64 (Arm64 chỉ vào bản submit đầu nếu có máy test thật); Store ký; capability `microphone`; WACK; package flight; kiểm tra WebView2 Evergreen lúc chạy và hướng dẫn cài nếu thiếu; Windows 10 1809+/11.
- AR-46 Tài khoản/chứng chỉ: Apple Developer Program (team RELIPA `B2U85XPU55`), App ID `com.transkun.app` đăng ký mới, cert Apple Distribution + Mac Installer Distribution, provisioning profile Mac App Store Connect, App Store Connect API key; Microsoft Partner Center developer account, reserve tên, `Package.appxmanifest` (identity, publisher, capability), bộ icon assets.
- AR-47 Hồ sơ submit: screenshot theo kích thước từng store, mô tả vi/en/ja, category Productivity, age rating 4+, Privacy Policy URL (vi/en/ja) + support URL, review notes + key demo (Apple) / Notes for certification (Microsoft), privacy label ("Audio Data / User Content gửi tới Google, không liên kết danh tính, không tracking; Advertising Data: none"); app không nhắc kênh tải ngoài store.
- AR-48 CI GitHub Actions 2 job (macOS universal; Windows x64 + Arm64) + job test hai OS; secrets cert / App Store Connect API key / Partner Center. Môi trường: dev (ký cert team), beta (TestFlight / package flight), store.
- AR-49 Hạ tầng của chúng ta chỉ gồm trang tĩnh: Privacy Policy, `ads.json`, `recommended-settings.json` (host GitHub Pages LP repo hoặc Cloudflare Workers + KV — Open Question 6); private key ed25519 giữ ngoài repo app. Vận hành: smoke test tự động hằng tuần với key test cho model Live preview và model transcribe mặc định; lỗi → cập nhật `recommended-settings.json`.
- AR-50 Ad slot dữ liệu: Creative `{id, image ≤ 100 KB, title, sponsor, url, locale[], start, end, weight}`, 300×100 hoặc 320×50, cache 24 h, fallback Creative nhúng sẵn, cap 1 Creative/10 phút, redirect URL có tham số chiến dịch không định danh người dùng.
- AR-51 Chừa chỗ cho phiên bản sau (không hiện UI ở v3): trait `KeyProvider` (nguồn key khác), cờ `is_premium`; phương án dự phòng nếu Apple từ chối BYOK (addendum §K: backend ephemeral token Cloudflare Workers, ~2–3 tuần) **không triển khai** trừ khi kích hoạt.

**Câu hỏi mở ảnh hưởng story (PRD §13)**

- AR-52 OQ1 Opus trong webm/mkv (chốt sau S1) · OQ2 inline FLAC/JSON schema cho model generateContent đã chọn (chốt sau S2, ảnh hưởng FR-14) · OQ3 FLAC seek trong WebView (chốt sau S8, ảnh hưởng FR-32) · OQ4 ai sở hữu/xoay key demo cho reviewer · OQ5 tên "trans-kun" còn trống trên App Store Connect/Partner Center (reserve sớm) · OQ6 nơi host Privacy Policy & endpoint · OQ7 máy test Windows Arm64 · OQ8 nội dung Template memo mặc định 3 ngôn ngữ (lấy từ v2 hay viết lại).

**Quyết định bổ sung về TTS (chủ sản phẩm, 2026-09-22)**

- AR-54 TTS chỉ dùng audio đầu ra sẵn có của Gemini Live Translate (tham chiếu: https://ai.google.dev/gemini-api/docs/live-api/live-translate.md.txt): `responseModalities: ["AUDIO"]`, audio trả về là PCM 16-bit 24 kHz mono little-endian trong `server_content.model_turn.parts[].inline_data`; text tuỳ chọn qua `inputAudioTranscription`/`outputAudioTranscription`. **Không** thêm model TTS local, engine TTS của OS hay dịch vụ TTS cloud (giữ NFR-1, NFR-4, NFR-8). Hệ quả cần nhớ: audio đầu ra **luôn được model sinh** (không có cách tắt); chỉ im lặng khi `echoTargetLanguage = false` và tiếng nói đã ở ngôn ngữ đích. Vì vậy toggle TTS chỉ điều khiển việc **phát**, không tiết kiệm token; chế độ "Không dịch" phải tận dụng đúng hành vi im lặng này (AR-38). Tài liệu Live Translate không mô tả sự kiện ngắt lời, resumption hay `goAway`; các hành vi đó dựa trên quan sát ở v2 và phải được spike S3 xác nhận lại.

**Gợi ý thứ tự / chất lượng từ tài liệu tham khảo (doc 05 — không ràng buộc, dùng khi xếp epic)**

- AR-53 Lộ trình tham khảo: Phase 0 spike (tuần 1–2) → Phase 1 nền tảng → Phase 2 Transcribe file → Phase 3 quản lý phiên/tag/memo/notes → Phase 4 Live → Phase 6 store readiness + ads → Phase 7 ra mắt (v2 ngừng phát hành sau 3 tháng). Tiêu chí QA Phase 2: cùng file mẫu so với transcript v2 bằng cùng model → lệch timestamp Segment ≤ 2 s, không mất đoạn (text khác biệt chấp nhận). Rủi ro nên giảm sớm: submit bản beta rỗng tính năng để "thông đường" tài khoản/cert/review.

### UX Design Requirements

*(Trích từ DESIGN.md và EXPERIENCE.md. Khi mâu thuẫn với mockup, spine thắng; khi mâu thuẫn với PRD — xem bảng mâu thuẫn ở đầu tài liệu.)*

**Token & nền tảng thị giác**

UX-DR1: Triển khai token màu **light** dạng CSS variable đúng giá trị DESIGN.md: nền/bề mặt (`bg #F6F6F2`, `bg-sidebar #F0F0EB`, `surface #FFFFFF`, `surface-sunken #ECEDE8`, `border #E1E4E8`, `border-strong #CFD4DA`), chữ ba cấp (`text #171A1F`, `text-secondary #3C4551`, `text-muted #5B6470`), accent (`#0F766E`, hover `#0B5D57`, soft `#E6F3F1`, border `#B7DDD8`), ngữ nghĩa (danger `#B42318`/strong `#A11E1E`/soft `#FDE8E8`/border `#F0B8B3`; warning `#8A4B0A`/soft `#FDF0DC`/border `#F5D9A8`; info `#1E4E9B`/soft `#E8EEF7`; recover `#5B2FA3`/soft `#F0EAFB`; memo `#0B5D57`/soft `#E0F2F1`; token-soft `#FFF7E6`; mark `#FDE68A`; overlay `rgba(23,26,31,.45)`). Quy tắc dùng: accent chỉ = hành động chính/đang chọn; warning = "chưa xong hoặc sẽ tốn tiền"; danger = dừng/xoá; info = badge Audio; recover chỉ cho badge "Phục hồi"; mark chỉ cho highlight tìm kiếm. Không thêm màu thứ năm mà không bỏ màu đang có.

UX-DR2: Triển khai token màu **dark** thiết kế song song (không đảo màu từ light): `bg-dark #141517`, sidebar `#1A1B1E`, surface `#1F2024`, sunken `#2A2B30`, border `#2E3036`, text `#ECEDEF`, `text-muted-dark #A0A6B0`, `accent-dark #2DD4BF` cho **chữ/icon** trong khi nút primary giữ nền `#0F766E` chữ trắng, `mark-dark #7A6019`, cùng các token semantic dark còn lại. Các token gắn `[ASSUMPTION]` trong DESIGN.md (border-strong, text-secondary, accent-hover, danger/warning/info/recover/memo/token soft & border, overlay dark…) **cần chủ sản phẩm/UX duyệt trước khi code**. Tương phản đo **riêng từng theme**, kèm kiểm tra tự động WCAG AA.

UX-DR3: Cơ chế theme (FR-48): Theo hệ thống / Sáng / Tối; mặc định theo `prefers-color-scheme`, đổi tức thì, đổi theo OS khi ở chế độ hệ thống, ép được ở Settings → Chung.

UX-DR4: Typography: IBM Plex Sans (400/500/600/700) và IBM Plex Mono **bundle trong app** (không tải Google Fonts lúc chạy); fallback chữ Nhật `-apple-system, "Segoe UI", "Hiragino Sans", "Yu Gothic UI", "Noto Sans JP"`; thang 8 bậc 11/12/13/14/15/16/18/22 (không lớn hơn 22); segment transcript line-height 1.55; weight heading/label/badge 600, body 400, nút 500, không dùng 700 ngoài nhãn speaker; mono cho timestamp, đồng hồ phiên, tên model, key, dung lượng với `font-variant-numeric: tabular-nums` **bắt buộc**.

UX-DR5: Token spacing/radius/layout: spacing 4-based (4/8/12/16/20/24/32; padding chuẩn 16); radius sm 6 (badge) · md 8 (nút, input, segmented, nav, dòng segment) · lg 10 (card nhỏ, radio card, `button-lg`, menu, banner) · xl 12 (card) · 2xl 14 (drop-zone) · 3xl 16 (dialog) · full (chip tag, status pill); sidebar 260, header 64, panel detail 360, panel live 320, settings nav 220, settings label col 220, transcript gutter 56; cửa sổ tối thiểu 1024×680, mặc định 1280×800.

UX-DR6: Elevation phẳng: phân lớp bằng tông nền (bg → sidebar → surface → sunken), card viền 1px không shadow; **chỉ hai ngoại lệ** có `box-shadow`: nút đang chọn trong segmented control (`0 1px 2px rgba(17,24,39,.08)`) và menu/dialog/toast/popover (`0 10px 32px rgba(17,24,39,.12)`); `box-shadow` ngoài hai trường hợp là lỗi review PR (nên có lint/kiểm tra tự động).

UX-DR7: Motion mức 3/10: hover/active 150 ms ease; panel mở/đóng 200 ms ease-out; dot "đang ghi" pulse 1.4 s; caret streaming `1s steps(2)`; toast vào 200 ms/ra 140 ms; `prefers-reduced-motion` tắt pulse, caret tĩnh, bỏ animation panel.

UX-DR8: Icon: **Lucide một bộ duy nhất**, stroke 1.75, 16–18 px trong nút, 14 px trong badge/meta; cấm emoji làm icon; icon-only button luôn có `aria-label` mô tả hành động.

UX-DR9: Focus ring `2px solid {accent}` offset 2 px, cùng một ring trên mọi control ở cả light và dark; `Tab` order khớp thứ tự thị giác từng màn.

**Component**

UX-DR10: Nút: cao 36 px, padding-x 14, radius 8, weight 500, gap 8; năm biến thể (primary, secondary, ghost, danger — chỉ cho "Dừng" ở Live, danger-soft — cho Xoá/Huỷ) + `button-lg` (44 px, radius 10) cho Onboarding và "Bắt đầu ghi". Nút vô hiệu `opacity .45` **luôn kèm tooltip nêu lý do + lối tắt** tới được bằng bàn phím; nút cần Gemini khi chưa có key bị vô hiệu, **không bao giờ ẩn**; một CTA primary mỗi màn.

UX-DR11: Input & trường Settings: input cao 36, viền `border-strong`; label 13/600 + icon (?) tooltip **trên mọi trường Settings**; help text 12 px muted **bền, không phải placeholder**; giá trị không hợp lệ bị chặn tại chỗ (không đợi Lưu); bảng 2 cột `220px | control`. Ô API key: `type=password` + hiện/ẩn + "Kiểm tra key", kết quả trong vùng `role=status`.

UX-DR12: Bộ badge cao 20 px, radius 6, 11/600, dùng `min-width` thay `width`: memo, audio, partial, recover, live, file, và **token** ("Tốn token Gemini", nền `token-soft`, viền `warning-border`) đặt cạnh Sinh memo / Sinh lại / Transcribe lại / Chạy lại (toàn bộ và phần thiếu). Vị trí: "Thiếu N khoảng" và "Phục hồi" ngay sau tên phiên (đòi hành động); memo/audio chỉ hiện ở detail.

UX-DR13: Status pill cao 30 px, radius full, 13/600: `status-rec` (đỏ, dot pulse), `status-ok`, `status-warn`, `status-off`. Ở Live **hai pill riêng không bao giờ gộp**: (a) "Đang ghi" + đồng hồ; (b) kết nối — Đang kết nối / Đang transcribe / Đang nối lại (+ thời gian đã chờ) / Đã dừng transcript (làm rõ C6); trạng thái luôn có icon + chữ (không chỉ màu; người mù màu phân biệt được "đang transcribe" với "đang nối lại").

UX-DR14: Dòng transcript: grid `56px | 1fr`, gap 12, padding 8/12, radius 8, hover `#F1F2EE`, đang phát nền `accent-soft` **và** thuộc tính ngữ nghĩa (`aria-current`); timestamp mono 12 muted tabular; speaker ẩn (giữ trong data). Dòng đặc biệt nằm **trong luồng** đúng vị trí thời gian: "Thiếu mm:ss–mm:ss" nền `warning-soft` + nút "Chạy lại khoảng này"; "Mất kết nối mm:ss–mm:ss" nền `surface-sunken`. Chế độ cạnh nhau: hai cột đều, mỗi cột giữ grid 56 px.

UX-DR15: Card (viền 1 px, radius 12, không shadow), `card-job` (nền/viền warning), chip tag (cao 26, pill; chọn = `accent-soft` + `accent-border` + icon x), segmented control (nền sunken, radius 8, nút chọn nền trắng + bóng 1 px), drop-zone (radius 14, viền dashed `border-strong`; active: `accent-border` + `accent-soft`).

UX-DR16: Dialog rộng 480, radius 16, overlay `overlay`; chỉ dùng cho **đúng 5 việc**: xoá phiên · xoá toàn bộ dữ liệu (hai bước) · đóng app khi đang ghi · đóng app khi có job · xoá tag toàn cục; hành động nguy hiểm tách sang phải, `button-danger-soft`; modal chồng tối đa **một cấp**.

UX-DR17: Banner (khuôn lỗi/cảnh báo inline gần nơi xảy ra): padding 12/16, radius 10, một hàng icon + chữ + nút hành động; ba phần: tiêu đề = tên category · một câu nguyên nhân + cách sửa · một nút hành động; ba biến thể warning/danger/info; không tự tắt; nhiều banner cùng lúc → gộp theo mức nghiêm trọng danger > warning > info, tối đa **hai** banner hiển thị.

UX-DR18: Toast rộng 360, radius 10, góc dưới phải vùng nội dung (cách mép 24), tự tắt 4 s, `aria-live=polite`; chỉ cho **việc nền đã xong** (memo xong, export xong, "Đã áp dụng N thiết lập"); **không** dùng cho lỗi cần hành động.

UX-DR19: Tag picker (popover 320, radius 12) **một component dùng chung ba nơi** (Trang chủ, Transcript detail, LiveSetup): ô tìm kiêm tạo mới; nhóm "Đang lọc" lên đầu; danh sách toàn bộ tag sắp theo số phiên; link "Quản lý tag"; chuẩn hoá trim/bỏ trùng không phân biệt hoa thường, ≤ 20 tag/phiên, ≤ 80 ký tự/tag; trạng thái chưa có tag: gợi ý "Gõ để tạo tag đầu tiên" `[ASSUMPTION]`.

UX-DR20: Hàng chip tag ở Trang chủ: một hàng **không wrap**: tag đang lọc (có x) → 3–5 tag dùng nhiều nhất → chip "+ N tag khác" mở popover; chip "Chưa gắn tag" tách bằng vạch dọc; nhiều tag = AND.

UX-DR21: Trình phát cao 64, nền surface, viền trên 1 px: nút play/pause tròn 40; thời gian `HH:MM:SS / HH:MM:SS` mono tabular; thanh seek `role=slider` (track sunken, fill accent) phím ←/→ nhảy 5 s; tốc độ + âm lượng; Space play/pause khi focus ở transcript; không tự động phát audio; Proxy hỏng → "Không có audio · Chọn lại file nguồn".

UX-DR22: Ad slot component: rộng 236 ở đáy sidebar 260, nền surface, viền 1 px, radius 10; creative (ảnh + text tĩnh ≤ 100 KB) + nhãn "Sponsored" + nút "Báo cáo quảng cáo" + link "Vì sao tôi thấy quảng cáo này"; **không bao giờ để trống hay co lại làm layout nhảy** (luôn có creative nhúng sẵn làm fallback); ẩn theo route Live bằng cấu trúc layout (kể cả sau khi bấm Dừng khi còn ở route Live); không âm thanh, không interstitial, không tự động phát, không che nội dung.

**App shell, IA, màn hình**

UX-DR23: App shell: sidebar 260 cố định (logo, 3 nav Trang chủ / Live / Cài đặt, card job thu gọn, ad slot ở đáy); header màn 64 cao (tiêu đề trái, hành động phải); panel phụ 360 (detail) / 320 (Live) — trạng thái đóng/mở nhớ riêng theo màn. Quy tắc co: ≥ 1280 sidebar + main + panel phụ cùng mở; 1024–1279 panel phụ đóng trước, sidebar giữ 260, transcript nhận phần dư; < 1024 không hỗ trợ (cửa sổ min 1024×680). Transcript + bản dịch chiếm ≥ 60 % chiều rộng cửa sổ.

UX-DR24: Routing & điều hướng: `/onboarding`, `/home`, `/session/:id`, `/live` (LiveSetup → Live → Dừng → `/session/:id`), `/settings/:group`; deep link nội bộ cho card job và badge "1 job đang chạy"; phím `g h` về Trang chủ, `⌘⇧L` vào Live; rời màn **không bao giờ** làm mất job/phiên đang chạy.

UX-DR25: Onboarding 3 bước (chỉ lần đầu hoặc khi phiên bản Consent tăng): card 600 giữa màn, stepper "Ngôn ngữ · Dữ liệu · API key"; bước ngôn ngữ: 3 radio card vi/en/ja, card trùng ngôn ngữ hệ thống có badge "Theo hệ thống", đổi tức thì cả luồng; không có bước kiểm tra môi trường.

UX-DR26: Màn Consent: sơ đồ "Máy của bạn → (bằng key của bạn) → Google Gemini" nhấn mạnh không có server trung gian; ba gạch đầu dòng; nêu đích danh Google; link Privacy Policy mở trình duyệt ngoài; dòng "Văn bản đồng ý phiên bản N"; hai nút "Không đồng ý" (ghost) và "Đồng ý và tiếp tục" (primary); từ chối → chế độ chỉ xem Settings/About + banner quay lại đồng ý (không request nào tới Google).

UX-DR27: Bước API key: kết quả kiểm tra render trong `role=status` (thành công: số model + model mặc định; lỗi: category + một câu hướng dẫn, sửa được tại chỗ); "Bỏ qua, nhập sau" luôn có; app không bao giờ vào Home ở trạng thái "trắng".

UX-DR28: Trang chủ = thư viện: header (tiêu đề, ô tìm theo tên có nút xoá, "Chọn file", "Live" primary); trống → hai card ngang hàng "Kéo file vào đây / Chọn file" + "Bắt đầu Live" kèm danh sách định dạng; đã có phiên → drop-zone mỏng phía trên danh sách, kéo file vào **bất kỳ đâu trên màn** đều nhận; footer luôn hiện `6 / 38 phiên · lọc…` + "Xoá bộ lọc"; không kết quả → trạng thái rỗng + nút "Xoá bộ lọc"; cold load: khung sidebar + header render ngay, danh sách skeleton 6 dòng khớp bố cục `[ASSUMPTION: cần duyệt]`; virtual list (500 dòng ≤ 1 s), không infinite scroll.

UX-DR29: Dòng phiên: click mở `/session/:id`; đổi tên inline (Enter lưu, Esc huỷ, rỗng bị từ chối, ≤ 200 ký tự); menu ⋯: Đổi tên · Gắn tag · Tải recording (chỉ phiên `live` có Recording; WAV/FLAC) · Xoá; cột hiển thị: Tên · Ngày · Loại (LIVE/FILE) · Thời lượng · Tag; **bỏ** cột Model/Segment/Trạng thái.

UX-DR30: Card job: đầy đủ ở Trang chủ + thu gọn ở sidebar, thấy từ mọi màn; tiến độ theo phút audio thực `32 / 90 phút · 36 %`; dòng chi tiết (chunk hiện tại, key đang dùng, số lần thử lại); số file chờ trong hàng đợi; nút "Mở" và "Huỷ" (Huỷ dừng gửi chunk mới ≤ 2 s, dọn tạm, không lưu phiên dở); trạng thái "đang chờ quota"; xoá phiên đang có job bị chặn.

UX-DR31: Transcript detail: header (quay lại, tên inline, meta, chip tag + "+ Tag", ô tìm, Export menu txt/srt/json, Copy, ⋯ Transcribe lại/Tải recording/Xoá); trình phát; banner Partial (liệt kê từng khoảng thiếu + "Chạy lại phần thiếu" / "Chạy lại toàn bộ", chỉ khi transcript đang xem là partial); segmented "Bản live · Transcribe lại · Cạnh nhau" chỉ khi phiên có 2 transcript (ghi rõ export/copy lấy bản đang chọn, hiện offset đang áp dụng); panel phải 360 với tab Memo | Ghi chú.

UX-DR32: Tự cuộn (Live + Transcript detail): cuộn theo dòng mới nhất, **dừng ngay khi người dùng cuộn lên**, hiện nút "Xuống dòng mới nhất" để quay lại; không auto-scroll đè thao tác cuộn.

UX-DR33: Ô tìm: Trang chủ (theo tên) ≤ 200 ms/500 phiên; trong transcript ≤ 100 ms/~700 segment; không phân biệt hoa thường, bỏ khoảng trắng thừa; đếm `n/N`, prev/next vòng tròn, `Enter` = next, `Shift+Enter` = prev, highlight `mark` + cuộn tới match; `⌘F`/`Ctrl+F`; query + lọc tag là AND, xoá query giữ lọc tag.

UX-DR34: Panel Memo: select template + nút "Sinh / Sinh lại" kèm badge "Tốn token Gemini"; dòng nguồn "Sinh từ bản X + ghi chú · giờ"; Markdown sanitize, link mở trình duyệt ngoài; footer Copy / Tải `.md`; lỗi memo **inline trong panel**, không đụng transcript; memo cũ sau khi transcript chạy lại kèm nhãn "Memo sinh từ bản trước".

UX-DR35: Panel Ghi chú: textarea tự lưu debounce ~800 ms, chỉ báo "Đã lưu hh:mm"; tab ở Transcript detail, **mở mặc định** ở Live; phải lưu trước khi phiên finalize.

UX-DR36: LiveSetup: card 680; 3 radio card nguồn (Mic + Hệ thống [Khuyên dùng] có dropdown mic + nút làm mới; Chỉ hệ thống; Chỉ mic); ô Tag tuỳ chọn (dùng Tag picker chung, hàng "Gần đây"); select "Ngôn ngữ transcribe" (auto/ja/vi/en) và "Dịch sang" (… / Không dịch); ghi chú nói trước chỉ xin System Audio Recording + Micro (không Screen Recording); nút "Bắt đầu ghi" `button-lg` + phím tắt; khi OS đã xác nhận thiếu quyền → banner category "Thiếu quyền hệ thống" + nút mở System Settings và kiểm tra lại quyền; trạng thái chưa hỏi vẫn cho Bắt đầu để kích hoạt prompt, nguồn khác không bị chặn; chưa có key → nút vô hiệu + tooltip + link Settings.

UX-DR37: Live đang ghi: toolbar 64 gồm pill Đang ghi (đồng hồ, dot pulse), pill kết nối, segmented "Gốc · Dịch · Cả hai", select nguồn (đổi giữa phiên ≤ 1 s), select target (có "Không dịch"), nút "Nhận diện lại" (ẩn khi ngôn ngữ ≠ auto), toggle loa TTS (`aria-pressed`, vô hiệu khi Không dịch), toggle Ghi chú, nút **Dừng** (danger); hai cột "Gốc (ja · auto)" và "Dịch (→ vi)" (chế độ Gốc/Dịch chỉ 1 cột); dòng đang stream có caret; pill "Đang đọc" ở đầu cột Dịch khi TTS phát; trạng thái mới bắt đầu: hai cột rỗng + "Đang nghe…" không spinner `[ASSUMPTION]`; banner trạng thái chỉ khi cần (mất mạng: "Vẫn đang ghi âm — transcript sẽ tự chạy tiếp khi có mạng"; server từ chối setup 5 lần: "Tiếp tục chỉ ghi âm" / "Dừng"; model lỗi: lối tắt Settings); **không có Ad slot** ở mọi trạng thái.

UX-DR38: Overlay "Đang lưu phiên… finalize recording, tạo proxy" sau khi Dừng, rồi điều hướng sang `/session/:id` (không ở lại Live read-only).

UX-DR39: Settings: nav trái 220 với 9 nhóm (Chung · Gemini · Chunking · Live · Memo · Lưu trữ · Chẩn đoán · Cấu hình đề xuất · Giới thiệu & Quyền riêng tư); Gemini: key, model file/live/memo (select + "Tải danh sách"; lỗi → banner tại chỗ, **không** xoá giá trị đang cấu hình), ngôn ngữ transcribe; **Memo**: master-detail (danh sách mẫu với badge "Mặc định · vi"/"Của bạn", "+ Thêm mẫu", "Khôi phục mẫu mặc định" chỉ ghi lại mẫu mặc định; editor inline tên + textarea prompt mono, hàng kiểm tra `{transcript}` bắt buộc / `{notes}` tuỳ chọn cập nhật khi gõ, thiếu `{transcript}` → nút Lưu vô hiệu + dòng kiểm tra đỏ, "Xoá mẫu" vô hiệu với mẫu mặc định, xoá mẫu của người dùng cần xác nhận) — **nơi duy nhất** CRUD Template memo; **Lưu trữ**: thanh dung lượng Media/DB, số phiên, "Mở thư mục", "Xoá toàn bộ dữ liệu…" (danger-soft, xác nhận hai bước); **Chẩn đoán**: "Xuất gói nhật ký" (allow-list, dialog lưu), "Xoá nhật ký", bộ đếm cục bộ — **không** có checkbox thống kê (xem mâu thuẫn C1); **Cấu hình đề xuất**: nút "Tải cấu hình đề xuất" → xác minh chữ ký → diff **inline trong nhóm này** (`nhãn · giá trị hiện tại → giá trị đề xuất`, chỉ liệt kê thiết lập đổi, không bao giờ liệt kê/ghi đè key) → "Áp dụng" (primary) / "Huỷ" → toast "Đã áp dụng N thiết lập"; chữ ký sai/mạng lỗi → banner tại chỗ, không đổi gì; **Giới thiệu & Quyền riêng tư**: version, liên hệ, Privacy Policy, "Xem lại văn bản đồng ý".

**State, lỗi, giọng điệu**

UX-DR40: Danh mục state patterns phải có đủ: chưa có key (banner warning ở Trang chủ "Chưa có API key hợp lệ — các tính năng cần Gemini đang tắt" + link "Nhập key"); từ chối Consent; Trang chủ trống; tìm/lọc không kết quả; job đang chạy; transcript partial (badge + banner + dòng trong luồng); phiên phục hồi (badge "Phục hồi" + gợi ý Transcribe lại); proxy hỏng; thiếu quyền hệ thống; model bị Google từ chối (cảnh báo tại chỗ, không tự đổi model); hết key 401/403; mọi key đang nghỉ 429 (job "đang chờ quota"); đang đọc TTS; Không dịch; cold load; đang tải danh sách model; Live vừa bắt đầu; chưa có tag; ad slot không creative; đang lưu phiên.

UX-DR41: Taxonomy lỗi 8 category hiển thị — **Quota · Key bị từ chối · Model · Mạng/CA · Định dạng · Quyền hệ thống · Lưu trữ · Nội dung bị chặn** — mỗi lỗi theo khuôn "tiêu đề = category · một câu nguyên nhân + cách sửa · một nút hành động" (Quota: Thử lại/chờ tự động + gợi ý key thứ hai; Key: → Settings→Gemini; Model: → Settings→Gemini chọn model khác; Mạng/CA: hướng dẫn proxy/CA; Định dạng: nêu định dạng nên chuyển sang; Quyền hệ thống: mở đúng trang System Settings; Lưu trữ: → Settings→Lưu trữ; Nội dung bị chặn: nêu chunk/khoảng ảnh hưởng, không thử lại). Category ổn định để i18n dịch được, bao trùm cả lỗi ngoài Gemini; lỗi ở một luồng không làm hỏng luồng khác.

UX-DR42: Voice & tone microcopy: bình tĩnh, cụ thể, nói thẳng chuyện gì đang xảy ra và làm gì tiếp; ba quy tắc cứng — (1) không bao giờ lộ stack trace, API key, URL endpoint, nội dung transcript; (2) mọi lỗi có một hành động; (3) số liệu cụ thể thắng tính từ ("32 / 90 phút · 36 %", "2:10"). Bảng Do/Don't của EXPERIENCE.md là chuẩn microcopy (ví dụ "Đang ghi âm · Đang nối lại (2:10)", "Mất kết nối 45:02–48:10", "Đã lưu 14:32", "Thiếu 3 khoảng · 12 phút").

UX-DR43: Minh bạch chi phí & quyền riêng tư trong UI: badge "Tốn token Gemini" trên mọi nút kích hoạt ba luồng tốn token; mở lại phiên có sẵn (trùng hash) **nói rõ không gọi Gemini**; Consent đi trước mọi byte; privacy mặc định không tài khoản/telemetry/tracking.

UX-DR44: Ràng buộc tương tác toàn cục: phím tắt `⌘⇧L`/`Ctrl+⇧+L` (Bắt đầu/Dừng Live), `⌘F`/`Ctrl+F`, `Enter`/`Shift+Enter`, `Space`, `←/→`, `Esc` (đóng panel/popover/dialog, huỷ đổi tên inline), `Enter` (lưu đổi tên); **cấm ở mọi nơi**: infinite scroll, hover-only affordance, modal chồng quá một cấp, icon-only button không `aria-label`, toast cho lỗi cần hành động, tự động phát audio, auto-scroll đè lên thao tác cuộn.

UX-DR45: Accessibility floor: WCAG 2.2 AA cho text, đo riêng light và dark; focus ring nhìn thấy trên mọi control; trạng thái không chỉ dựa vào màu (icon + chữ); kết quả kiểm tra key trong `role=status`; toast `aria-live=polite`; thanh seek `role=slider`; toggle TTS `aria-pressed`; nút vô hiệu giải thích qua tooltip tới được bằng bàn phím; segment đang phát có nền accent-soft lẫn thuộc tính ngữ nghĩa.

UX-DR46: Đa ngôn ngữ ở tầng layout: nút và badge dùng `min-width` (không `width` cố định) để chứa ja/vi; timestamp và số luôn mono tabular; **ngôn ngữ UI ≠ ngôn ngữ transcribe ≠ target dịch**; UI không giả định một phiên chỉ có một ngôn ngữ; ngôn ngữ UI kéo theo Template memo mặc định, nhãn thời gian, văn bản Consent, text Ad slot, header nhúng ghi chú vào prompt memo.

UX-DR47: Khác biệt nền tảng: phím `⌘` (macOS) vs `Ctrl` (Windows); link mở đúng trang System Settings/Settings theo OS; Windows: kiểm tra WebView2 lúc chạy và hướng dẫn cài nếu thiếu; Tải recording chỉ WAV/FLAC (không lựa chọn M4A — xem mâu thuẫn C2); kiểm thử chấp nhận thu được Zoom, Teams, Google Meet trong Chrome trên cả hai OS.

UX-DR48: Kiểm tra Key Flows (UJ-1…UJ-5 + đường thất bại) làm tiêu chí chấp nhận cấp epic: onboarding ≤ 3 phút tới transcript chạy (UJ-1); họp 60 phút với reconnect/đổi nguồn/Nhận diện lại/TTS và không mất dữ liệu khi force-quit (UJ-2); file 90 phút với xoay key, Khoảng thiếu, Chạy lại phần thiếu, memo (UJ-3); tìm lại câu ba tuần trước ≤ 30 s (UJ-4); reviewer Apple đi hết vòng sản phẩm không màn trắng/crash/quyền lạ (UJ-5).
