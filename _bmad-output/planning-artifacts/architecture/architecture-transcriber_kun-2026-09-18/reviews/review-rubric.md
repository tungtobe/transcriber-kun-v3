# Review — ARCHITECTURE-SPINE.md (trans-kun v3) theo rubric "good spine"

Đối tượng: `../ARCHITECTURE-SPINE.md` (memlog: `../.memlog.md`; PRD: `../../../prds/prd-transcriber_kun-2026-09-17/prd.md`)
Ngày review: 2026-09-18

## Verdict

Spine ở mức tốt: paradigm rõ, 15 AD có Binds/Prevents/Rule đầy đủ, Capability map phủ **toàn bộ FR-1..47 không trùng không hở** (partition sạch), version stack hầu hết được xác minh trong memlog, và việc port audio v2 được khoanh đúng phạm vi trong một module không cần ratify brownfield. Điểm yếu thật sự nằm ở **NFR**: 8/12 NFR (NFR-3–8, 10, 11) không xuất hiện trong Capability map và một số (NFR-4 không phụ thuộc ngoài, NFR-7 entitlement tối thiểu, NFR-11 accessibility, phần "không gọi nền định kỳ" của NFR-8) không có Rule/AD nào thực sự khoá — đây đúng là các điểm mà hai epic có thể tự diễn giải khác nhau. Envelope vận hành cũng thiếu hẳn "release/versioning" và có "testing strategy" chỉ ở mức rất mỏng.

## Tóm tắt theo mức độ

| Mức | Số lượng |
|---|---|
| Critical | 0 |
| High | 2 |
| Medium | 7 |
| Low | 4 |

---

## Findings

### 1. [High] Checklist #6 — 8/12 NFR vắng mặt trong Capability → Architecture Map
**Finding:** Bảng Capability → Architecture Map chỉ nhắc NFR-1, NFR-2, NFR-9, NFR-12 (ở dòng "xuyên suốt") và NFR-8 chỉ xuất hiện lẻ trong trường `Binds` của AD-6, không có trong bảng. NFR-3, NFR-4, NFR-5, NFR-6, NFR-7, NFR-10, NFR-11 hoàn toàn không được trace tới AD nào trong bảng — và với NFR-4, NFR-5, NFR-7, NFR-11, không có AD nào trong toàn bộ tài liệu thực sự khoá chúng (xem finding #2, #3, #6 bên dưới). Đây là 8 trong 12 NFR không được "phủ" theo đúng nghĩa checklist yêu cầu ("covers the spec's capabilities... via the Capability map").
**Suggested fix:** Bổ sung các dòng vào Capability map (hoặc mở rộng dòng "xuyên suốt") cho NFR-3 (→ AD-4, AD-10), NFR-4 (→ AD mới hoặc note trong Design Paradigm), NFR-6 (→ Stack: reqwest rustls-tls-native-roots), NFR-7 (→ AD mới về entitlement), NFR-8 (→ AD-6, làm rõ cả phần "không gọi nền định kỳ"). Với NFR-5, NFR-10, NFR-11 nếu chủ động không cần AD ở tầm kiến trúc, nêu rõ trong `Deferred` kèm lý do tại sao chúng không phải điểm phân kỳ giữa epic (giống cách đã làm tốt với mục i18n/router), thay vì im lặng.

### 2. [High] Checklist #1/#7 — NFR-11 (accessibility) là điểm phân kỳ thật nhưng không có Rule nào
**Finding:** NFR-11 yêu cầu "điều hướng bàn phím cho các thao tác chính (bắt đầu/dừng Live, tìm kiếm, export); tương phản đạt WCAG AA". Đây chính xác là loại điểm phân kỳ spine cần khoá: nhiều epic/màn hình (Live, Home/Library, Settings, Transcript detail) đều tự cài phím tắt và tab-order riêng nếu không có quy ước chung — dễ dẫn tới xung đột phím tắt hoặc focus-order không nhất quán giữa các epic build độc lập. AD-13 (frontend) chỉ nói về store theo domain và design token, không đả động keyboard nav/WCAG. Không có AD nào bind NFR-11.
**Suggested fix:** Thêm một Rule (mở rộng AD-13 hoặc AD mới) quy định: nơi đăng ký global keyboard shortcuts là một chỗ duy nhất (tránh mỗi component tự `addEventListener`), danh sách phím tắt cốt lõi cố định, và tokens tương phản AA lấy từ DESIGN.md là bắt buộc — hoặc đưa vào Deferred với gate rõ ràng ("chốt trước epic Home/Live UI").

---

### 3. [Medium] Checklist #1/#7 — NFR-4 (không phụ thuộc ngoài, một tiến trình) không có Rule enforceable
**Finding:** NFR-4 ("Không binary ngoài, không tải/chạy code lúc chạy, không quyền admin, không self-update") chỉ được phản ánh gián tiếp qua câu mở đầu Design Paradigm ("một tiến trình") và việc Stack không liệt kê ffmpeg/Python. Không có AD/Rule nào ngăn một epic sau này thêm sidecar/subprocess hay updater — đây từng là nguồn lỗi lớn ở v2 (theo `docs/rebuild-v3/02-bai-hoc-va-luu-y.md` được kế thừa).
**Suggested fix:** Thêm một dòng Rule ngắn (có thể gộp vào Design Paradigm hoặc một AD hạ tầng) khẳng định: không spawn process ngoài, không tải code lúc runtime, cập nhật app chỉ qua store — và liệt kê NFR-4 vào Capability map.

### 4. [Medium] Checklist #1/#7 — NFR-7 (entitlement/sandbox tối thiểu) không có chủ sở hữu
**Finding:** NFR-7 yêu cầu entitlement tối thiểu (network client, mic, file người dùng chọn; không Screen Recording). Structural Seed chỉ liệt kê tên file cấu hình (`tauri.appstore.conf.json`, `tauri.msix.conf.json`) mà không có Rule nào nói ai sở hữu chúng hay việc thêm entitlement mới phải qua đâu. Đây là điểm phân kỳ thật: một epic tương lai (vd. chia sẻ màn hình, hoặc auto-update check) có thể âm thầm yêu cầu entitlement mới vi phạm điều kiện store.
**Suggested fix:** Thêm Rule ngắn: entitlement/capability chỉ được thêm vào `tauri.conf.json`/overlay qua review kiến trúc, danh sách entitlement tối thiểu liệt kê tường minh (giống cách AD-12 khoá asset protocol scope).

### 5. [Medium] Checklist #2 — AD-6 không thực sự khoá phần "không gọi nền định kỳ" của NFR-8
**Finding:** `Binds` của AD-6 liệt kê NFR-8, nhưng Rule của AD-6 chỉ nói về cổng Gemini chung và ưu tiên quota (Live > Job > Memo), không có câu nào cấm gọi API định kỳ/nền (vd. tự động poll `listModels`, health-check). NFR-8 nói rõ "Không có gọi nền định kỳ" — phần này chưa được Rule nào khoá dù đã được khai Binds.
**Suggested fix:** Thêm một câu vào Rule của AD-6: "`gemini/` không tự phát request định kỳ/nền; mọi request phải bắt nguồn từ hành động người dùng hoặc job đang chạy."

### 6. [Medium] Checklist #1 — Không có mẫu chung cho cancel/progress của tác vụ dài (NFR-2)
**Finding:** NFR-2 yêu cầu "Mọi tác vụ dài có cancel, progress, lỗi có category ổn định". Phần category đã khoá tốt bằng AD-7, nhưng cancel/progress cho Job file, Transcribe lại, sinh Memo không có quy ước chung nào (không có convention IPC cho "huỷ", không có event `progress` chuẩn trong bảng Event ở Consistency Conventions). AD-2 chỉ định nghĩa handle giao tiếp qua `mpsc`/`oneshot` nhưng không có pattern "cancel message" bắt buộc. Hai epic (Job vs Memo) dễ tự chọn cách huỷ khác nhau (abort future vs message vs flag).
**Suggested fix:** Thêm vào AD-2 hoặc Consistency Conventions một dòng: lệnh huỷ tác vụ dài luôn là command `<domain>_cancel(id)` gửi message tới actor/registry sở hữu; tiến độ (progress) phát qua Channel event đã có ở AD-3, không tạo cơ chế riêng.

### 7. [Medium] Checklist #4 — `tauri-build` trong Stack không có entry version trong memlog
**Finding:** Bảng Stack ghi `Tauri / tauri-build / @tauri-apps/cli / @tauri-apps/api | 2.11.5 / 2.6.3 / 2.11.4 / 2.11.1`. Dòng `(version)` trong `.memlog.md` (2026-09-18) xác nhận `tauri 2.11.5`, `@tauri-apps/cli 2.11.4`, `@tauri-apps/api 2.11.1` nhưng **không** có `tauri-build 2.6.3` ở đâu cả. Đây là tech được nêu tên (named tech) nhưng không verified-current theo đúng nghĩa checklist #4.
**Suggested fix:** Thêm `tauri-build 2.6.3` vào dòng `(version)` của memlog (hoặc chạy lại xác minh crates.io) trước khi coi Stack là chốt.

### 8. [Medium] Checklist #7 — Dimension "release/versioning" hoàn toàn im lặng
**Finding:** Không có chữ nào trong spine về semver app, cách gắn version build với migration DB, changelog, hay quy trình submit lại store khi có bản vá. Đây là một trong các dimension checklist #7 liệt kê tường minh; im lặng hoàn toàn (không phải deferred có gate, không phải "đã quyết").
**Suggested fix:** Thêm ít nhất một dòng quyết định tối thiểu, vd: "App version = tag git; migration `db/migrations` không bao giờ sửa migration đã phát hành, chỉ thêm mới; mỗi submit store là một tag" — hoặc đưa vào Deferred với gate rõ ràng.

### 9. [Medium] Checklist #7 — "Testing strategy" chỉ khoá 3 test cụ thể, không phải một chiến lược
**Finding:** Dòng `Test` trong Consistency Conventions chỉ nêu: port giả cho 3 boundary, test JSON shape Gemini, test grep log. Không có gì về: cargo test vs vitest tách thế nào, CI có chặn merge khi test fail không, và đặc biệt — `audio/` port từ v2 có 2 nhánh nền tảng riêng (CoreAudio tap macOS, WASAPI Windows) nhưng không có quy ước nào về việc test/CI phải chạy trên cả hai OS trước khi merge. Đây là rủi ro phân kỳ thật (một dev chỉ test trên máy mình).
**Suggested fix:** Thêm câu về CI matrix mac+win chạy `cargo test`/`vitest` bắt buộc trước merge, và ít nhất một dòng về việc audio platform-specific code cần test trên đúng OS (không mock chéo).

---

### 10. [Low] Checklist #2 — AD-1 và AD-13 chỉ enforceable qua review, không có gate tự động
**Finding:** AD-1 ("Feature không import feature khác") và AD-13 ("component không gọi `invoke` trực tiếp") là các Rule hợp lý nhưng không có cơ chế tự động (lint rule, `cargo` boundary check, eslint `no-restricted-imports`) được nêu tên — khác với AD-7 (khoá bằng type system) hay AD-15 (có test grep log cụ thể). Về bản chất vẫn "enforceable" qua code review, nhưng yếu hơn các AD khác trong cùng tài liệu.
**Suggested fix:** Cân nhắc thêm một dòng ở Consistency Conventions: lint/CI check cấm `feature/` import `feature/` khác, và cấm gọi `invoke`/binding trực tiếp ngoài `stores/`. Không bắt buộc, nhưng nên nhất quán với mức enforceability của các AD khác.

### 11. [Low] Checklist #3 — Mục Deferred "diễn giải F3" thiếu gate rõ ràng như các mục Deferred khác
**Finding:** Mục Deferred "Diễn giải 'Recording chỉ tạo sau khi kết nối thành công' (F3)" ảnh hưởng trực tiếp logic `live/` (Live có được bắt đầu offline hay không) — nếu chưa chốt trước khi epic Live bắt đầu build, hai phần khác nhau của epic (capture vs WS connect) có thể giả định khác nhau. Các mục Deferred khác trong danh sách đều có gate rõ ràng (vd. "chốt trước epic chẩn đoán", "chờ spike S1/S2/S8"), riêng mục này chỉ nói "Chủ sản phẩm xác nhận" mà không gắn với epic/thời điểm cụ thể.
**Suggested fix:** Thêm gate: "phải chốt trước khi bắt đầu epic Live" để nhất quán với các mục Deferred khác.

### 12. [Low] Checklist mermaid — erDiagram: quan hệ tự tham chiếu và entity đơn độc nên rà lại khi render
**Finding:** `TRANSCRIPTS ||--o| TRANSCRIPTS : retranscribed_from` là quan hệ tự tham chiếu, và `SETTINGS` là entity đứng một mình không có quan hệ/attribute. Cú pháp Mermaid hợp lệ, nhưng self-relationship trong `erDiagram` có lịch sử render không ổn định ở một số phiên bản Mermaid renderer (lặp vòng đè lên entity). Không phải lỗi cú pháp, chỉ là rủi ro hiển thị.
**Suggested fix:** Xác nhận render thử trên renderer thực tế (GitHub/mermaid.live) trước khi coi sơ đồ là final; nếu lặp/đè, cân nhắc bỏ quan hệ tự tham chiếu ra khỏi ER diagram và ghi chú bằng text thay vì cạnh đồ thị.

### 13. [Low] Checklist #6 — Dòng "xuyên suốt" trong Capability map gộp AD mơ hồ, khó truy vết từng NFR
**Finding:** Dòng `NFR-1/NFR-2/NFR-9/NFR-12 | xuyên suốt | AD-1, AD-5, AD-7, AD-15` gộp 4 NFR vào 4 AD mà không rõ NFR nào ↔ AD nào. Cụ thể AD-1 (hướng phụ thuộc) không rõ ràng là cơ chế khoá NFR-1 (privacy) hay NFR-2 (không chặn lõi) — quan hệ hợp lý nhưng gián tiếp, không tường minh như các dòng khác trong bảng (vd. dòng FR-8–16 → AD-2/5/9/11 rõ từng AD làm gì).
**Suggested fix:** Tách dòng này thành các dòng riêng theo từng NFR (NFR-1 → AD-15; NFR-2 → AD-1, AD-14; NFR-9 → AD-7; NFR-12 → AD-5) để dễ truy vết khi audit sau này.

---

## Điểm tốt đáng ghi nhận (không phải finding, nhưng nên giữ nguyên khi sửa)

- Capability map phủ FR-1..47 thành một phép chia đúng, không trùng không hở (5+2+9+6+4+5+4+3+5+3+1 = 47).
- 15/15 AD đều có `Binds` / `Prevents` / `Rule` tách bạch; hầu hết Rule đủ cụ thể để chấm dứt tranh cãi giữa hai epic (vd. AD-4 định nghĩa hẳn máy trạng thái `transcripts.status`, AD-9 định nghĩa đơn vị thời gian tuyệt đối).
- Đa số mục Stack có version khớp với entry `(version)` trong memlog ngày 2026-09-18 — chỉ thiếu `tauri-build` (finding #7).
- Việc port audio v2 được khoanh đúng một module (`audio/`) sau một port-boundary theo AD-2, không cần ratify brownfield toàn repo — đúng tinh thần greenfield.
- Không có template placeholder (`TODO`/`TBD`/`{{...}}`) sót lại trong tài liệu.
- Deferred phần lớn có lý do rõ vì sao không gây phân kỳ (vd. router, thư viện i18n) — đúng tinh thần checklist #3, trừ 1 mục nêu ở finding #11.
