# Review epics — 2026-09-20

Đã review toàn bộ 7 epic / 55 story, đối chiếu PRD, addendum, Architecture Spine, DESIGN và EXPERIENCE; bộ rebuild-v3 dùng làm bối cảnh. Đã sửa trực tiếp [epics.md](epics.md), giữ nguyên ID story và phạm vi 7 epic. Các tài liệu nguồn không bị sửa trong lượt này.

Bốn lens đã chạy: adversarial, edge-case-hunter, structure, prose (prose dựa trên structure). Không chạy verification-gap vì đầu vào là tài liệu, không phải code. Các finding trùng giữa lens được giữ để thấy cùng vấn đề được phát hiện độc lập. Bản máy đọc: [JSON](epics-review-2026-09-20.json).

Review và sửa tài liệu đã hoàn tất; trạng thái triển khai là `revised-with-open-decisions`. Các gate OQ2, OQ9, OQ10, dark tokens, S3 và Windows TTS cùng câu hỏi nguồn có chủ trì/mốc chốt trong epics. Không có yêu cầu nào được đánh dấu đã triển khai hoặc đã pass chỉ nhờ sửa AC.

Sau lượt đối chiếu cuối, FR-22/FR-23 và UX-DR36 cũng đã được đồng bộ với hợp đồng trạng thái `connected`, flow xin quyền lần đầu và ngoại lệ lỗi lưu trữ Recording.

Đây là review tính nhất quán và khả năng nghiệm thu từ nguồn trong repo. Chưa chạy spike, gọi Gemini, build app, xác minh version thư viện hay chứng nhận store; các bằng chứng đó được chỉ định cho story liên quan.

## Adversarial

### 1. AR-7–14; 4.2, 4.4, 5.3

- Điều kiện: Spike bắt buộc bị đặt sau nhiều epic triển khai.
- Sửa/guard: Đã thêm Phase 0 cho S1–S6/S8 và chống thu TTS Windows; tích hợp giữ ở epic cũ. Nguồn: addendum §L, Spine Deferred.
- Hệ quả nếu giữ nguyên: Phát hiện rủi ro nền tảng khi đã xây phần lớn sản phẩm.

### 2. 2.3, 4.10

- Điều kiện: Transaction được mô tả bao gồm cả DB và filesystem; cleanup quá muộn.
- Sửa/guard: Đã tách transaction DB/publish media, thêm reconciliation idempotent và boot cleanup ngay ở 2.4. Nguồn: AD-16/18.
- Hệ quả nếu giữ nguyên: Crash để file mồ côi hoặc DB trỏ tới file chưa tồn tại.

### 3. 2.5, 4.11

- Điều kiện: Retry phần thiếu luôn thay primary dù đích là retranscribe.
- Sửa/guard: Đã thêm transcript_id và validation variant; retry chỉ swap bản đích. Nguồn: FR-15/26, AD-16.
- Hệ quả nếu giữ nguyên: Ghi đè bản live cần giữ.

### 4. 2.7, 4.9

- Điều kiện: Sửa Proxy live đòi file nguồn cùng hash vốn không tồn tại.
- Sửa/guard: Đã tách phục hồi file theo hash và live từ Recording. Nguồn: FR-10/23/24.
- Hệ quả nếu giữ nguyên: Có Recording nhưng không thể nghe lại.

### 5. 4.6, 4.7

- Điều kiện: Contract connection thiếu trạng thái kết nối thành công.
- Sửa/guard: Đã bổ sung connected sau setupComplete; C6 ghi việc đồng bộ AD-10/addendum §H.
- Hệ quả nếu giữ nguyên: Snapshot/remount không dựng được trạng thái đang transcribe trung thực.

### 6. 4.6

- Điều kiện: Test outage ba phút phủ gap cả phần audio còn giữ trong buffer.
- Sửa/guard: Đã tách reconnect ngắn và outage dài; gap chỉ phủ phần bị đẩy khỏi buffer. Nguồn: FR-22, AD-10.
- Hệ quả nếu giữ nguyên: Bỏ audio có thể replay hoặc gap chồng transcript.

### 7. 4.2, 4.7

- Điều kiện: Silence bị coi là thiếu quyền; nút vô hiệu có thể chặn prompt lần đầu.
- Sửa/guard: Đã cho mở tap khi quyền chưa biết, chỉ chặn nguồn OS xác nhận từ chối. Nguồn: addendum §F, FR-17.
- Hệ quả nếu giữ nguyên: Lần đầu không thể cấp quyền hoặc nguồn im lặng bị báo lỗi sai.

### 8. 3.5

- Điều kiện: Debounce/flush khi đóng không chứng minh không mất notes lúc force-quit.
- Sửa/guard: Đã thêm revision, ACK bền và test kill; OQ10/C7 giữ quyết định durability còn mở. Nguồn: FR-36.
- Hệ quả nếu giữ nguyên: Mất ký tự chưa lưu nhưng story vẫn bị đánh dấu done.

### 9. 3.7

- Điều kiện: Schema Memo không có nguồn để hiển thị bản transcript và nhãn stale.
- Sửa/guard: Đã thêm provenance transcript/notes/template/model. Nguồn: FR-38, EXPERIENCE Panel Memo.
- Hệ quả nếu giữ nguyên: Memo bị gán sai bản nguồn sau restart.

### 10. 3.1, 3.4, 3.7

- Điều kiện: Xoá dữ liệu không tính request Memo ngoài JobRegistry.
- Sửa/guard: Đã thêm writer guard/generation và bỏ kết quả muộn sau xoá. Nguồn: AD-11, FR-30/40.
- Hệ quả nếu giữ nguyên: Dữ liệu đã xoá xuất hiện lại.

### 11. 3.6, 1.9

- Điều kiện: Template mặc định chỉ được nạp theo locale lúc migration.
- Sửa/guard: Đã thêm locale/origin/identity ổn định và quy tắc đổi locale/restore. Nguồn: FR-1/37/47.
- Hệ quả nếu giữ nguyên: Đổi ngôn ngữ không đổi đúng bộ mẫu hoặc sinh bản trùng.

### 12. 6.1, 6.2

- Điều kiện: remote nuốt mọi lỗi nhưng preview phải báo lỗi tải/chữ ký.
- Sửa/guard: Đã trả typed provenance/freshness/error; ads fallback, preview không giả báo fresh. Nguồn: AD-14/19, FR-42.
- Hệ quả nếu giữ nguyên: Người dùng áp cấu hình cũ tưởng vừa tải thành công.

### 13. 6.1, 6.3, 6.5

- Điều kiện: Chữ ký ảnh không có format tương ứng ở công cụ ký.
- Sửa/guard: Đã ràng buộc bytes ảnh bằng digest trong manifest ký hoặc chữ ký riêng; client/publisher chung format. Nguồn: AD-14, FR-46.
- Hệ quả nếu giữ nguyên: Ảnh bị thay vẫn hiển thị dưới manifest hợp lệ.

### 14. 2.4, 2.8

- Điều kiện: Gate key chạy trước nhánh mở file trùng không cần Gemini.
- Sửa/guard: Đã cho hash/Existing sau Consent nhưng trước key gate của Job mới; drop file trùng vẫn mở local. Nguồn: FR-4/11, AD-11.
- Hệ quả nếu giữ nguyên: File đã transcribe không mở lại được sau khi xoá key.

### 15. 3.7, 4.12, NFR-2

- Điều kiện: Memo và encode/export dài thiếu cancel/progress.
- Sửa/guard: Đã thêm busy/cancel/deadline, giữ bản cũ và dọn output tạm. Nguồn: NFR-2.
- Hệ quả nếu giữ nguyên: Người dùng không biết tác vụ còn chạy và không huỷ được.

### 16. 1.4, 1.5, 1.8

- Điều kiện: Consent từ chối xung đột route guard và smoke test tới Home.
- Sửa/guard: Đã tách trạng thái chưa onboard/từ chối/đồng ý, sửa đích smoke test. Nguồn: FR-2, EXPERIENCE.
- Hệ quả nếu giữ nguyên: Vòng lặp onboarding hoặc vào màn không được phép.

## Edge-case hunter

### 1. 2.4

- Điều kiện: Cancel tới lúc request hoàn tất hoặc Job còn chờ.
- Sửa/guard: Đã quy định điểm commit, bỏ kết quả muộn, huỷ Job chờ theo ID.
- Hệ quả nếu giữ nguyên: Job đã huỷ vẫn tiêu token hoặc tạo Phiên.

### 2. 2.8

- Điều kiện: Hai file trùng vào hàng đợi trước commit đầu tiên.
- Sửa/guard: Đã thêm reservation source_hash trong JobRegistry.
- Hệ quả nếu giữ nguyên: Gửi Gemini hai lần trước UNIQUE constraint.

### 3. 2.3

- Điều kiện: Crash giữa publish file và DB commit.
- Sửa/guard: Đã thêm thứ tự publish/commit và boot reconciliation idempotent.
- Hệ quả nếu giữ nguyên: DB thiếu media hoặc file mồ côi.

### 4. 2.5, 4.11

- Điều kiện: Retry gap trong retranscribe của Phiên live.
- Sửa/guard: Đã kiểm ownership transcript_id và swap đúng variant.
- Hệ quả nếu giữ nguyên: Primary live bị ghi đè.

### 5. 2.5

- Điều kiện: Chunk settings đổi trước khi retry.
- Sửa/guard: Đã retry theo gap tuyệt đối, giữ Segment ngoài vùng đích.
- Hệ quả nếu giữ nguyên: Trùng nội dung hoặc bỏ sót gap.

### 6. 1.7, 2.2

- Điều kiện: Alias mặc định hoặc tên model tự nhập không có major version.
- Sửa/guard: Đã yêu cầu capability mapping kiểm chứng S2/S3, không đoán từ tên.
- Hệ quả nếu giữ nguyên: Model hợp lệ nhận cấu hình không hỗ trợ.

### 7. 2.2

- Điều kiện: Timestamp lỗi, không hữu hạn hoặc ngoài Chunk.
- Sửa/guard: Đã validate thời gian, giữ Segment hợp lệ và đánh dấu phần chưa cứu.
- Hệ quả nếu giữ nguyên: Seek/export có timestamp vô nghĩa.

### 8. 2.6, 2.7, 2.10

- Điều kiện: Offset làm timestamp âm hoặc cue SRT rỗng.
- Sửa/guard: Đã dùng chuẩn hoá chung, SRT bỏ cue end<=start; seek dùng thời gian gốc.
- Hệ quả nếu giữ nguyên: SRT lỗi hoặc seek lệch audio.

### 9. 2.7

- Điều kiện: Proxy live thiếu nhưng Recording còn.
- Sửa/guard: Đã cho tái tạo từ Recording trong Container.
- Hệ quả nếu giữ nguyên: Không khôi phục được player dù audio còn.

### 10. 3.2

- Điều kiện: Chọn tag cụ thể cùng Chưa gắn tag.
- Sửa/guard: Đã làm hai lựa chọn loại trừ nhau, giữ query tên.
- Hệ quả nếu giữ nguyên: Bộ lọc không thể có kết quả.

### 11. 3.5

- Điều kiện: Force-quit trước deadline debounce.
- Sửa/guard: Đã nêu ACK bền, test recovery window; OQ10 chưa được tự coi đã duyệt.
- Hệ quả nếu giữ nguyên: Ghi chú vừa gõ biến mất.

### 12. 3.1, 3.4, 3.7

- Điều kiện: Memo hoàn tất sau xoá owner hoặc request sinh lại mới.
- Sửa/guard: Đã guard revision/generation và giữ quyền ghi theo session.
- Hệ quả nếu giữ nguyên: Dữ liệu xoá sống lại hoặc Memo cũ ghi đè mới.

### 13. 3.7

- Điều kiện: Đầu vào đổi lúc Memo đang sinh.
- Sửa/guard: Đã snapshot và lưu provenance của đầu vào thực tế.
- Hệ quả nếu giữ nguyên: Nhãn nguồn và stale không đúng.

### 14. 4.5

- Điều kiện: WAV/DB lỗi sau khi capture mở.
- Sửa/guard: Đã dừng an toàn, bỏ chỉ báo ghi sai, giữ bytes bền; C8 cần đồng bộ nguồn.
- Hệ quả nếu giữ nguyên: UI báo đang ghi trong khi mất audio.

### 15. 4.4

- Điều kiện: Resume handle hết hạn hoặc ACK không rõ.
- Sửa/guard: Đã yêu cầu S3 chứng minh ACK, replay/dedup và fallback kết nối mới.
- Hệ quả nếu giữ nguyên: Replay mất hoặc nhân đôi lời nói.

### 16. 4.8

- Điều kiện: Sửa key sau khi chọn tiếp tục chỉ ghi âm.
- Sửa/guard: Đã nói rõ transcript dừng hết phiên; key mới cho phiên mới, gap tới Dừng.
- Hệ quả nếu giữ nguyên: Hành vi tiếp tục transcript mơ hồ.

### 17. 4.6, 4.9

- Điều kiện: Dừng khi câu cuối chưa có dấu kết câu.
- Sửa/guard: Đã flush phần text hợp lệ cuối và chặn event muộn.
- Hệ quả nếu giữ nguyên: Mất câu cuối.

### 18. 4.10

- Điều kiện: Recovery tạo Proxy đồng thời xoá hoặc retranscribe.
- Sửa/guard: Đã reserve busy, kiểm session/revision trước publish.
- Hệ quả nếu giữ nguyên: Recovery tạo lại file đã xoá.

### 19. 5.1, 5.2

- Điều kiện: Target đổi nhanh, setup lỗi hoặc Dừng khi đang chuẩn bị.
- Sửa/guard: Đã tuần tự hoá, giữ latest intent, rollback và cancel candidate khi Dừng.
- Hệ quả nếu giữ nguyên: Target UI sai hoặc WS mở lại sau Dừng.

### 20. 5.1, 5.3

- Điều kiện: Tắt dịch khi TTS generation cũ còn chờ.
- Sửa/guard: Đã gắn generation, dọn buffer và phục hồi Ducking ngay.
- Hệ quả nếu giữ nguyên: Vẫn đọc ngôn ngữ cũ sau khi tắt.

### 21. 5.4

- Điều kiện: Đổi output hoặc chỉnh tay trước crash.
- Sửa/guard: Đã lưu device/validity và vô hiệu marker khi chỉnh tay.
- Hệ quả nếu giữ nguyên: Phục hồi volume sai thiết bị hoặc sai ý người dùng.

### 22. 6.2

- Điều kiện: Settings đổi sau preview hoặc giá trị ký không hợp lệ.
- Sửa/guard: Đã gắn digest/revision, validate local, preview lại và apply transaction.
- Hệ quả nếu giữ nguyên: Ghi đè lựa chọn mới hoặc lưu cấu hình lỗi.

### 23. 6.1

- Điều kiện: Ảnh URL bị thay hoặc redirect sang scheme khác.
- Sửa/guard: Đã kiểm digest, HTTPS/redirect và allow-list scheme ngoài.
- Hệ quả nếu giữ nguyên: Hiển thị nội dung bị thay hoặc mở URL nguy hiểm.

### 24. 6.3

- Điều kiện: Hết Creative đủ cap hoặc weight không hợp lệ.
- Sửa/guard: Đã validate weight, fallback, persist cap và định nghĩa impression.
- Hệ quả nếu giữ nguyên: Selector lỗi hoặc điều hướng lách cap.

## Structure và prose

Tài liệu giúp chủ sản phẩm và lập trình viên triển khai các story từ bộ kế hoạch nguồn. Mô hình phù hợp: Reference/Database, với phần định hướng Strategic/Context ở đầu. Giữ giọng tài liệu kỹ thuật tiếng Việt, thuật ngữ và nhãn BDD tiếng Anh; chỉ sửa câu làm mơ hồ hành vi.

`word_metrics.py` đo bản gốc 28.236 từ: Overview 275; FR 3.080; NFR 381; AR 2.798; UX 3.565; FR Coverage 504; Epic List 973. Không có mục tiêu cắt ngắn. Các đề xuất cấu trúc gồm 7 mục, 1 mục PRESERVE; giảm dự kiến 0 từ (0%), không đánh đổi mất ngữ cảnh. Việc bổ sung contract và gate làm bản sửa dài hơn; không tuyên bố giảm độ dài.

| Pass | Original Text | Revised Text | Changes |
|---|---|---|---|
| structure | Overview → Requirements Inventory → Epic List | MOVE Epic List lên trước; inventory xuống cuối có anchor. | Áp dụng. 9.824 từ inventory không còn chắn đường tới epic; tác động riêng việc di chuyển 0 từ. |
| structure | Frontmatter chỉ hai bước nhưng tuyên bố complete | QUESTION trạng thái review/readiness rõ ràng. | Áp dụng reviewedAt/reviewStatus; không sửa lịch sử stepsCompleted thành đã nghiệm thu. |
| structure | Phụ thuộc nằm rải trong 973 từ Epic List | QUESTION bảng thứ tự và prerequisite mỗi story. | Áp dụng cho 55 story, phân biệt spike Phase 0 với tích hợp. |
| structure | Qn/OQn và ASSUMPTION rải rác | MERGE chỉ mục quyết định và story bị chặn. | Áp dụng bảng OQ1–OQ10 cùng gate spike/nguồn; giữ assumption inline. |
| structure | FR Coverage Map 504 từ dạng dòng liên tiếp | MOVE thành bảng FR \| story \| nội dung. | Áp dụng 48 FR, bổ sung 12 NFR. |
| structure | Nguồn trong YAML, tiêu đề epic bị lặp | QUESTION liên kết nguồn và anchor chi tiết. | Áp dụng link nguồn và anchor duy nhất từng epic. |
| structure | Giới thiệu tại từng epic lặp lợi ích; 392 từ | PRESERVE ngữ cảnh epic và BDD. | Giữ vì người đọc có thể mở thẳng một epic; không dịch nhãn BDD chỉ vì phong cách. |
| prose | 2.4: chunkMinutes và offset đọc từ settings với mặc định 5 phút | chunkMinutes mặc định 5 phút; offset chỉ áp dụng UI/export. | Tách đơn vị và thời điểm áp dụng. |
| prose | 2.5: tối đa 4 lần theo chính sách Key pool | Tổng tối đa 4 lần gửi, gồm lần đầu và xoay key. | Tránh lần đầu cộng bốn retry. |
| prose | 2.5: commit Phiên ở trạng thái partial | Commit Phiên cùng Transcript có status=partial; sessions.status=complete. | Phân biệt hai state machine. |
| prose | 1.7: danh sách code và Tls ... thuộc category network | Chỉ Tls cho CA/proxy thuộc category network. | Làm rõ phạm vi bổ nghĩa. |
| prose | 4.5/4.10: tới mốc ≤ 5 s trước khi chết | Phần audio cuối không phát được tối đa 5 s trước khi tiến trình bị kết thúc. | Viết ngưỡng đo được, bớt mơ hồ. |
| prose | 4.9: không đổi status của transcript sai | Lỗi finalize/Proxy không đổi complete/partial của Transcript. | Nêu tác nhân và hợp đồng; tách lỗi commit metadata. |
| prose | 5.1: speech đã ở Target ... không im lặng | Vẫn hiển thị ở cột Dịch; không đồng nghĩa tự bật TTS. | Tránh nhầm text với âm thanh. |

## Các sửa bổ sung trong lượt đối chiếu chính

| Vị trí | Sửa |
|---|---|
| C4 | Sửa phát biểu sai rằng PRD FR-27 đã được cập nhật; ghi rõ còn lệch nguồn. |
| 2.1 | Bộ fixture phủ cả AIFF và CAF; chia Chunk lặp tới khi payload thực sự vừa giới hạn. |
| 2.2 / C5 | Không tự thêm Files API trái NFR-8; biến kết quả S2 thành gate rõ ràng. |
| 2.4, 2.6 | Snapshot model/ngôn ngữ/chunk; offset đổi ở hiển thị, không đưa vào pipeline. |
| 2.4, 2.7 | Định nghĩa route tiến độ trước DB commit và trạng thái sau huỷ/restart. |
| 3.2 | Chuẩn hoá Tag Unicode, phân biệt picker lọc và picker gắn cho Phiên. |
| 3.4 | OQ9 cho phạm vi xoá toàn bộ; dung lượng DB không bị hứa giảm về 0. |
| 3.6 | Xác nhận xoá Template inline, giữ Memo đã sinh khi Template bị xoá. |
| 4.4 / AR-39 | Chặn trần 30 s sau jitter; phân biệt retry Auth/quota/setup/transport. |
| 4.7 | Tag LiveSetup được gắn khi tạo Phiên; rời route rồi quay lại không tạo phiên mới. |
| 4.11 | Tối đa một bản hiện hành mỗi variant; nguồn Export/Copy/Memo ở Cạnh nhau được chọn rõ. |
| 5.1 | Không lấy việc thiếu outputAudioTranscription làm bằng chứng không phát sinh audio/token dịch. |
| 5.3 | Chống thu lại TTS phải bảo vệ cả Recording và model, không tắt toàn bộ họp để pass. |
| 7.2 | Thông báo thiếu WebView2 phải từ native trước khi WebView tồn tại. |
| 7.5 | Tách submitted/approved/published; chỉ nghiệm thu ra mắt khi cài được cả hai store. |

## Kiểm tra sau sửa

- Đủ 7 epic chi tiết, 55 story với ID gốc và đủ Given/When/Then.
- Đủ FR-1–48, NFR-1–12, AR-1–53, UX-DR1–48; bảng FR/NFR trỏ tới story có thật.
- 55 khai báo phụ thuộc, không có vòng phụ thuộc; spike harness được tách khỏi thứ tự tích hợp.
- Kiểm tra anchor/link nguồn và lỗi whitespace; đối chiếu diff theo từng story để không mất AC ngoài chủ đích.
- Không chạy test ứng dụng vì chỉ thay tài liệu; các test được thêm ở đây là tiêu chí cần thực hiện khi build.
