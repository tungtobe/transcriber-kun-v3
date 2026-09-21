---
name: 'Review đối kháng — Architecture Spine trans-kun v3'
type: adversarial-review
target: ../ARCHITECTURE-SPINE.md
lens: 'Tấn công xương sống như một đối thủ: dựng hai epic một cấp dưới cùng tuân thủ mọi AD nhưng vẫn build ra thứ không tương thích'
created: '2026-09-18'
---

# Review đối kháng — Architecture Spine trans-kun v3

**Phương pháp:** với mỗi cặp epic (Onboarding/Settings, Transcribe file, Live, Library/Home/Transcript detail, Memo, Ads, Diagnostics, Store build), giả định hai đội build độc lập, mỗi đội đọc đúng chữ của AD liên quan và tự suy ra phần AD không nói tới theo cách hợp lý nhất cho epic của mình. Tìm chỗ hai suy luận "hợp lệ" đó đụng nhau.

Tổng cộng 8 phát hiện. Không phát hiện nào đòi tuning nhỏ — mỗi cái là một khoảng trống thật trong spine (thiếu Rule, thiếu schema, hoặc chữ AD tự mâu thuẫn phạm vi).

---

## F1 — [CAO] "Khoảng thiếu" (gap) không có hình dạng dữ liệu chung giữa Transcribe file và Live

**Bối cảnh:** FR-15 (file) và FR-22 (live) đều tạo "Khoảng thiếu" — Chunk lỗi hẳn (file) hoặc mất kết nối > 60s buffer (live) — và cả hai đều hứa "Chạy lại phần thiếu" / "Transcribe lại lấp được". Nhưng ER diagram trong spine chỉ có `TRANSCRIPTS`, `SEGMENTS`, `TAGS`, `NOTES`, `MEMOS`, `MEMO_TEMPLATES`, `SETTINGS` — **không có bảng hay cột nào đại diện Khoảng thiếu**. AD-4 chỉ nói `segments` là nguồn phục hồi duy nhất, không nói gap được biểu diễn thế nào trong đó.

**Hai thiết kế tuân thủ AD-4/AD-9 nhưng khác nhau:**
- Đội Transcribe file: gap là **suy ra ngầm** — không insert gì cả, "Chạy lại phần thiếu" tính khoảng trống bằng cách so `segments.start/end` liên tiếp với độ dài Chunk kỳ vọng, tìm lỗ hổng.
- Đội Live: gap là **row tường minh** — insert một `Segment` đặc biệt (vd. cột `kind = 'gap'` hoặc text đặc biệt) mang lý do "mất kết nối mm:ss–mm:ss" (vì UI live cần hiển thị lý do, không chỉ khoảng thời gian trống).

**Hậu quả:** logic "Chạy lại phần thiếu" dùng chung trong `JobRegistry` (AD-11: một hàng đợi cho Transcribe file, Chạy lại, Transcribe lại) phải hiểu **một** hình dạng gap. Nếu nó được viết theo kiểu "dò lỗ hổng ngầm" (khớp với Transcribe file), nó sẽ bỏ qua gap tường minh của Live (đã có row nên không phải "lỗ"), khiến FR-22 "Transcribe lại lấp được" phần mất kết nối **không hoạt động**. Nếu viết theo kiểu "tìm row kind=gap", nó sẽ không tìm thấy lỗ hổng ngầm của Transcribe file. Đây là chỗ đụng "shared-data shape" đúng nghĩa: hai epic tuân thủ đúng chữ AD-4 nhưng sinh ra hai biểu diễn không ai đọc được của bên kia.

**AD fix tối thiểu:** thêm vào AD-4 (hoặc AD mới) — Khoảng thiếu là một loại `Segment` tường minh với `kind ∈ {text, gap}` (hoặc bảng `gaps` riêng khoá theo `transcript_id + start + end + reason`), dùng chung cho cả file và live; `reason` là enum (`chunk_failed | disconnected`) để UI hiển thị đúng câu chữ theo nguồn gốc.

---

## F2 — [CAO] AD-4 chỉ định nghĩa mồ côi cho "Phiên live", không cho Job file bị crash giữa chừng

**Bối cảnh:** AD-4 nêu rõ bằng tiếng Việt: *"Phiên live còn `recording|finalizing` là mồ côi"* — chữ "Phiên live" giới hạn phạm vi câu Rule, dù dòng `Binds` của AD-4 ghi "mọi feature". Nhưng `transcripts.status` là **một cột dùng chung** cho cả Transcribe file lẫn Live (không có bảng status riêng cho file job).

**Hai thiết kế tuân thủ chữ AD-4 nhưng khác nhau:**
- Đội Live: chỉ cần viết routine khởi động quét `transcripts` có `status ∈ {recording, finalizing}` VÀ `source = live` (theo đúng câu chữ "Phiên live"), đánh dấu `partial + recovered=true`.
- Đội Transcribe file: đọc cùng câu chữ, kết luận "vậy Job file không cần cơ chế phục hồi mồ côi ở tầng kiến trúc" — vì Job vốn dĩ không sống qua restart (AD-11: `JobRegistry` là bộ nhớ, "Job không sống qua restart"), nên không viết startup reconciliation cho `transcripts` của file job.

**Hậu quả:** nếu file epic ghi `transcripts.status = finalizing` giữa lúc xử lý (để hiện progress ở Home — "Job đang chạy" theo PRD §Home) rồi app crash, row đó **kẹt vĩnh viễn ở `finalizing`**, không `recovered`, không hiện cảnh báo Partial — vi phạm chính NFR mà AD-4 sinh ra để tránh ("hai epic hiểu mồ côi khác nhau"). Đây chính là pair "recovery vs live" nhưng lộ ra ở lớp rộng hơn: enum trạng thái dùng chung nhưng Rule phục hồi chỉ nói một nhánh.

**AD fix tối thiểu:** sửa câu Rule AD-4 thành "Bất kỳ `transcripts` nào còn `recording|finalizing` lúc khởi động là mồ côi — áp dụng cho cả Phiên live và Job file", và chỉ rõ startup reconciliation là **một routine duy nhất trong `db/` hoặc `ipc/`** quét toàn bảng, không phải logic riêng của từng feature.

---

## F3 — [CAO] Xoá Phiên trong khi Job đang chạy — `library/` không có kênh hợp lệ nào để biết Job còn sống

**Bối cảnh:** AD-1: feature không import feature khác, chỉ chia sẻ qua repo trong `db/`. AD-11: `JobRegistry` là **nguồn sự thật trong bộ nhớ, không có bảng `jobs`**. `library/` sở hữu xoá Phiên (FR-27–31).

**Hai thiết kế tuân thủ tuyệt đối từng AD nhưng khác nhau:**
- Đội Library: xoá = xoá row `transcripts` + file trong `media/<id>/` qua repo, đúng theo AD-1 ("feature chỉ gọi hàm repo"). Không thể kiểm tra Job vì Job không có mặt trong `db/` (AD-11), và không được gọi thẳng `transcribe::` (AD-1 cấm feature-to-feature).
- Đội Transcribe: `JobRegistry` giả định transcript id nó đang xử lý **luôn tồn tại** suốt vòng đời Job (giả định hợp lý vì không ai nói ngược lại), nên khi Job xong, nó `INSERT/UPDATE segments` bằng repo bình thường, không kiểm tra transcript còn tồn tại hay không.

**Hậu quả:** người dùng xoá Phiên trong khi Job (Chạy lại/Transcribe lại) đang chạy nền → Job tiếp tục ghi `segments` cho một `transcript_id` đã bị xoá (lỗi FK nếu có ràng buộc, hoặc "hồi sinh" một row người dùng tưởng đã xoá nếu không có FK cứng) — đúng loại "hai owner của một entity, hai luồng ghi state xung đột" mà lens này nhắm tới. Điều đáng chú ý: đây không phải sơ suất code, mà là **kiến trúc tự chặn đường** để library/ làm đúng việc, vì AD-1 + AD-11 cộng lại không chừa lối nào khác ngoài `ipc/`.

**AD fix tối thiểu:** thêm câu vào AD-1 hoặc AD-11: "Lệnh xoá Phiên (`library_delete_session`) được điều phối trong `ipc/`: gọi `transcribe::cancel_job(transcript_id)` (hủy nếu đang chạy/trong hàng đợi, no-op nếu không có) **trước khi** gọi `library` xoá row + file." `JobRegistry` cần expose `cancel(id) -> bool` qua handle actor (đã có sẵn kênh mpsc theo AD-2, chỉ thiếu việc gọi).

---

## F4 — [TRUNG BÌNH-CAO] Transcribe lại: row mới hay ghi đè row cũ — không ai chốt, mà Library và Transcribe cần câu trả lời khác nhau

**Bối cảnh:** ER diagram có tự-quan-hệ `TRANSCRIPTS ||--o| TRANSCRIPTS : retranscribed_from`, gợi ý Transcribe lại tạo **row con mới**. Nhưng AD-5 (định danh) chỉ nói ID Phiên là UUIDv7 tự sinh và `source_hash` unique cho Phiên **file** — không nói gì về việc Transcribe lại có sinh ID mới hay giữ nguyên ID cũ và chỉ thay `segments`.

**Hai thiết kế tuân thủ AD-5/AD-11 nhưng khác nhau:**
- Đội Transcribe: đọc ER diagram, dựng Transcribe lại = tạo `transcripts` row mới với `retranscribed_from = old_id`, `source_hash` copy hoặc null (không unique nữa vì AD-5 chỉ bắt unique cho Phiên file gốc) — mỗi lần Transcribe lại là một transcript độc lập trong `JobRegistry` hàng đợi (khớp AD-11 "Transcribe lại" là một loại job riêng).
- Đội Library: xây rename/Tag/Ghi chú/Memo trên giả định **một Phiên = một `transcript_id` ổn định suốt vòng đời**, vì đó là mô hình đơn giản nhất khớp AD-1 ("feature chỉ gọi repo", không có gợi ý nào về multi-row) và khớp FR-27–31 (rename/xoá thao tác trên "Phiên", số ít).

**Hậu quả:** nếu Transcribe lại thực sự tạo row mới nhưng Library không được thiết kế để hiển thị nhóm cha-con, người dùng thấy **hai Phiên trùng lặp** trong Home sau khi bấm "Transcribe lại", Tag/Ghi chú/Memo gắn ở bản cũ "biến mất" khỏi bản mới. Ngược lại nếu Transcribe ghi đè in-place, `retranscribed_from` trong ER diagram vô nghĩa và lịch sử bản gốc mất — mâu thuẫn trực tiếp với việc spine đã vẽ quan hệ tự tham chiếu.

**AD fix tối thiểu:** thêm Rule vào AD-4 hoặc AD-11: "Transcribe lại tạo `transcripts` row mới (`retranscribed_from = <id gốc>`), Tag/Ghi chú/Memo Template áp dụng lại được chọn copy hay không tại UI; `library_list` mặc định gộp nhóm cha-con thành một entry, hiện bản mới nhất." Chốt rõ để cả hai epic build đúng cùng một mô hình.

---

## F5 — [TRUNG BÌNH-CAO] `recommended-settings.json`: AD-14 chỉ định nghĩa fetch/verify, không định nghĩa ai *áp* giá trị vào `settings`

**Bối cảnh:** AD-14: `remote/` tải + xác minh ed25519 + cache. AD-8: `settings/` là chủ ghi duy nhất bảng `settings`. Không AD nào nói **áp dụng** recommended-settings vào bảng `settings` là việc của module nào, hay chính sách merge (ghi đè toàn bộ / chỉ điền chỗ trống / người dùng phải bấm "Áp dụng").

**Hai thiết kế tuân thủ chữ AD-14 + AD-8 nhưng khác nhau:**
- Đội Remote: đọc AD-14 thấy `remote/` "tải" recommended-settings — vì đã có cache + verify sẵn trong `remote/`, tiện nhất là `remote/` gọi thẳng `db::settings::upsert(...)` sau khi verify (vẫn qua repo trong `db/`, không vi phạm chữ AD-1).
- Đội Settings: đọc AD-8 thấy "settings/ sở hữu", nghĩ nhiệm vụ của mình là expose command `settings_apply_recommended()` do UI gọi tường minh, tự quyết chính sách merge (chỉ điền field người dùng chưa từng đổi).

**Hậu quả:** nếu `remote/` tự ghi thẳng, nó âm thầm ghi đè cấu hình người dùng đã tinh chỉnh mỗi lần app fetch định kỳ (vi phạm kỳ vọng ngầm "settings do người dùng kiểm soát" và AD-8 muốn `settings/` là **chủ duy nhất của quyết định ghi**, không chỉ chủ của bảng). Hai đội đều đúng luật nhưng một trong hai sẽ có bug "cấu hình tự đổi không rõ lý do" — đúng loại lỗi khó debug nhất trong review lens này.

**AD fix tối thiểu:** thêm câu vào AD-14: "`remote/` không bao giờ ghi bảng `settings`; nó chỉ trả dữ liệu đã verify. `settings/` quyết định khi nào và merge thế nào (mặc định: chỉ điền field chưa từng bị người dùng đổi; ghi đè toàn bộ chỉ khi người dùng bấm nút riêng)."

---

## F6 — [TRUNG BÌNH] File marker Ducking: không AD nào định vị trí/chủ sở hữu

**Bối cảnh:** addendum §D: ducking 30% "với marker file phục hồi sau crash"; FR-21: crash khi đang Ducking → lần mở sau phục hồi volume gốc. Spine không nhắc "marker file" ở đâu cả — không trong AD-5 (chỉ định nghĩa file trong `media/<session-id>/`), không trong AD-10, không trong Structural Seed.

**Hai thiết kế tuân thủ (im lặng của spine cho phép cả hai) nhưng khác nhau:**
- Đội Audio (hạ tầng, `audio::playback`): tự viết marker + tự đọc/xoá lúc `audio::playback` khởi tạo ở lần mở app kế tiếp — coi ducking-crash-recovery là chuyện nội bộ của `audio/`, không ai cần biết.
- Đội Live (chủ trì "phục hồi sau crash" nói chung, vì đã phải viết routine quét mồ côi theo AD-4 lúc khởi động): coi luôn "phục hồi volume" là một bước trong cùng routine khởi động của `live/`, tự đọc file marker theo đường dẫn nó tự chọn.

**Hậu quả:** nếu hai đội không phối hợp path/format của marker (khả năng cao vì không AD nào ép), một bên viết, bên kia đọc sai chỗ (hoặc không ai đọc) → FR-21 "phục hồi volume sau crash" thất bại âm thầm — không lỗi, không log, chỉ là volume người dùng vẫn thấp 30% mãi mãi. Mức nghiêm trọng thấp hơn các F1-F4 vì phạm vi hẹp (một file), nhưng đúng kiểu lỗi "hai owner của cùng effect, im lặng" mà lens yêu cầu tìm.

**AD fix tối thiểu:** thêm một dòng vào AD-10: "Marker file ducking là tài sản riêng của `audio::playback` — path cố định `$APPDATA/audio_ducking.marker` (ngoài `media/`), chỉ `audio/` được đọc/ghi/xoá; `live/` không đụng vào, chỉ gọi `audio::playback::recover_on_startup()` một lần lúc app khởi động."

---

## F7 — [TRUNG BÌNH] `seq` trong AD-3 không đồng nhất phạm vi giữa Live (có generation) và Job (không có)

**Bối cảnh:** AD-3: mỗi event mang `seq` tăng đơn điệu "theo instance", UI hụt seq → subscribe lại. AD-10 (chỉ cho Live): restart kết nối dùng `LiveGeneration{id}`, "event mang id cũ bị bỏ". AD-13: mỗi domain có một store, cùng "thực thi AD-3" — ngụ ý một cách hiểu `seq` dùng chung cho mọi domain.

**Hai thiết kế tuân thủ AD-3 nhưng khác nhau:**
- Store `live.svelte.ts`: buộc phải hiểu `seq` là đơn điệu **trong phạm vi một `LiveGeneration`**, vì AD-10 nói rõ event của generation cũ bị bỏ — reconnect (không phải bug) hợp lệ làm `seq` "nhảy lùi" theo generation mới, không phải một "gap" thật.
- Store `transcribe.svelte.ts` (Job): không có khái niệm generation nào được AD nào nhắc tới cho Job; đội này viết logic gap-detection tổng quát nhất theo đúng chữ AD-3 ("tăng đơn điệu theo instance" = tăng đơn điệu tuyệt đối, hụt là phải subscribe lại, không có ngoại lệ).

**Hậu quả:** nếu một dev viết chung một helper `subscribeWithGapDetection()` dùng cho cả hai store (hợp lý theo AD-13 "mỗi domain store thực thi AD-3" — ngụ ý cùng cơ chế), copy đúng logic từ Job (không biết generation) sang Live, mỗi lần reconnect sẽ bị hiểu nhầm là "mất event" → vòng lặp subscribe lại vô ích (không hỏng nặng, nhưng đúng kiểu "hai state-mutation path xung đột trên cùng một khái niệm").

**AD fix tối thiểu:** thêm vào AD-3: "`seq` reset về từ đầu subscribe mỗi khi `<domain>_subscribe` được gọi lại (kể cả do đổi generation); phía UI luôn coi `seq` là đơn điệu-trong-phiên-subscribe-hiện-tại, không đơn điệu tuyệt đối qua các lần subscribe. Domain có khái niệm generation (Live) phải tự phát `restarting` event (đã có trong bảng Convention) trước khi seq reset, để store phân biệt 'reset vì generation' với 'gap thật'."

---

## F8 — [THẤP-TRUNG BÌNH] Phiên bản Consent "hiện hành" — nguồn nào là chủ, compile-time hay remote?

**Bối cảnh:** AD-8: "Consent (phiên bản đã đồng ý) là một settings" — chỉ nói *lưu* consent của người dùng. AD-6: `gemini/` từ chối request khi chưa có "Consent phiên bản hiện hành" — nhưng con số "phiên bản hiện hành" để so sánh đến từ đâu không được spine nói tới. Đồng thời AD-14 nói `remote/` là kênh duy nhất tải nội dung từ trang tĩnh Relipa, nơi cũng host Privacy Policy.

**Hai thiết kế tuân thủ nhưng khác nhau:**
- Đội Settings/Onboarding: "phiên bản hiện hành" là hằng số Rust compile-time trong `settings/` (đơn giản, khớp AD-8 "Settings do Rust sở hữu"), bump mỗi lần release đổi Privacy Policy.
- Đội Remote/Compliance: vì Privacy Policy host trên trang tĩnh (AD-14) và ads/recommended-settings đã có cơ chế versioned-config-từ-xa sẵn, "hợp lý" nhất là consent-version cũng nằm trong `recommended-settings.json` để đổi được **không cần release app**.

**Hậu quả:** nếu gate ở AD-6 (`gemini/`) được implement đọc từ hằng số compile-time nhưng đội pháp lý/compliance kỳ vọng đổi version từ xa (vì đó là lý do `remote/` tồn tại), một thay đổi Privacy Policy sau khi ship không có tác dụng — người dùng cũ tiếp tục dùng app với consent version cũ mãi mãi, không bị chặn lại để xin đồng ý mới. Ngược lại nếu version đến từ remote nhưng offline/signature-fail rơi về "cache hoặc fallback nhúng sẵn" (AD-14), cần chốt fallback đó **không được cao hơn** version cache cũ (else block oan người dùng đã đồng ý bản hiện tại, chỉ vì offline).

**AD fix tối thiểu:** thêm câu vào AD-6 hoặc AD-8: "'Phiên bản Consent hiện hành' là hằng số compile-time trong `settings/` (không qua remote/) — chốt kiến trúc đơn giản, tránh phụ thuộc mạng vào luồng gate an toàn nhất của app." (Hoặc nếu PM muốn remote-driven, chốt ngược lại — nhưng phải chốt một trong hai, không để ngỏ.)

---

## Tổng kết mức độ nghiêm trọng

| # | Cặp epic | Mức độ |
|---|---|---|
| F1 | Transcribe file vs Live — hình dạng "Khoảng thiếu" | CAO |
| F2 | Transcribe file vs Live — phạm vi Rule mồ côi AD-4 | CAO |
| F3 | Library (xoá Phiên) vs Transcribe (JobRegistry) | CAO |
| F4 | Transcribe lại vs Library — row mới hay ghi đè | TRUNG BÌNH-CAO |
| F5 | Remote vs Settings — ai áp recommended-settings | TRUNG BÌNH-CAO |
| F6 | Audio vs Live — chủ marker file ducking | TRUNG BÌNH |
| F7 | Frontend store Live vs Job — phạm vi `seq` | TRUNG BÌNH |
| F8 | Settings/Onboarding vs Remote — nguồn version Consent | THẤP-TRUNG BÌNH |
