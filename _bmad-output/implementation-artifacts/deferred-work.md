- source_spec: `_bmad-output/implementation-artifacts/spec-1-1-dung-repo-greenfield-ci-hai-os-va-binding-ipc-typed.md`
  summary: Thêm harness test render component (jsdom/happy-dom + @testing-library/svelte) và test App.svelte hiển thị "…"/version/"—" theo trạng thái store.
  evidence: Review pass 1 — chỉ store `app` có test; đổi nhánh `{#if}` trong `src/App.svelte` không làm test nào fail. Hợp lý nhất khi làm story 1.3 (app shell).
- source_spec: `_bmad-output/implementation-artifacts/spec-2-3-luu-phien-va-transcript-ben-vung.md`
  summary: Mở rộng `library::store::reconcile` để giữ file role `recording.*` (và Phiên live `recording|finalizing`) trước khi Epic 4 ghi Recording vào `media/<sid>/`.
  evidence: Reconcile 2.3 xoá mọi entry trong thư mục Phiên trừ `proxy.<ext>` đang được DB tham chiếu; chưa có cột/role Recording trong schema.
- source_spec: `_bmad-output/implementation-artifacts/spec-6-5-remote-publishing-and-smoke.md`
  summary: Chốt GitHub Pages origin, Privacy Policy URL và người giữ signing key; cấu hình build, xuất bản nội dung đã ký và xác minh URL thật; thêm test API key để chạy smoke mạng.
  evidence: Người dùng chọn GitHub Pages nhưng sẽ cung cấp URL sau; key custodian và `GEMINI_SMOKE_API_KEY` chưa được cung cấp. Công cụ, workflow và runbook đã triển khai, nhưng các bước production này không thể kiểm chứng khi thiếu giá trị thật.
