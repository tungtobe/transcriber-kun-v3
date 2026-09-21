- source_spec: `_bmad-output/implementation-artifacts/spec-1-1-dung-repo-greenfield-ci-hai-os-va-binding-ipc-typed.md`
  summary: Thêm harness test render component (jsdom/happy-dom + @testing-library/svelte) và test App.svelte hiển thị "…"/version/"—" theo trạng thái store.
  evidence: Review pass 1 — chỉ store `app` có test; đổi nhánh `{#if}` trong `src/App.svelte` không làm test nào fail. Hợp lý nhất khi làm story 1.3 (app shell).
