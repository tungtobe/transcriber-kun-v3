# ADR 0001 — `Channel<T>` typed qua tauri-specta rc.25

- Status: Accepted
- Ngày: 2026-09-22
- Liên quan: Architecture Spine AD-3 (đồng bộ UI bằng snapshot + delta có `seq`), Deferred "tauri-specta rc.25 với Tauri 2.11 và `Channel<T>` typed — spike Phase 0; nếu gãy, bọc Channel bằng type viết tay, AD-3 giữ nguyên."
- Spike: `src-tauri/src/ipc/spike_channel.rs` (`#[cfg(test)]`, không đăng ký production, không xuất ra `src/lib/bindings.ts`)

## Câu hỏi

`tauri-specta = 2.0.0-rc.25` (exact, theo bảng Stack) có sinh được binding TypeScript
`Channel<T>` **typed** (không phải `Channel<unknown>`/`any`) từ một command Rust nhận
`tauri::ipc::Channel<SpikeEvent>`, và runtime gửi/nhận qua Channel đó có đúng thứ tự
`seq` không? Đây là điều kiện tiên quyết để AD-3 (snapshot + delta `seq`) dùng
`Channel` sinh tự động thay vì viết tay.

## Đã kiểm

1. **API thật của rc.25 khác tài liệu `main` branch.** Docs chính thức (context7 →
   `/specta-rs/tauri-specta`) mô tả cả `Builder::export_str` lẫn
   `CommandSet::build` (TanStack Query) — cả hai **không tồn tại** trong
   `2.0.0-rc.25` (`cargo build` báo `E0599`). API thật của bản pin: `Builder::new()`,
   `.commands(collect_commands![...])`, `.events(collect_events![...])`,
   `.typ::<T>()`, `.constant(...)`, `.error_handling(...)`, `.invoke_handler(&self)`,
   `.mount_events(&self, &impl Manager<R>)`, `.export::<L>(&self, lang, path)` (chỉ
   ghi ra file, không có bản trả `String`). `specta_builder()` trong
   `src-tauri/src/ipc/mod.rs` và spike đều dùng đúng tập API này.

2. **Cần bật feature `specta` trên crate `tauri`.** `tauri::ipc::Channel<T>` chỉ
   implement `specta::Type` khi `tauri` được biên dịch với feature `specta`
   (`#[cfg(feature = "specta")] const _: () = { #[derive(specta::Type)]
   #[specta(remote = super::Channel)] struct TAURI_CHANNEL<TSend>(...); };` —
   xem `tauri-2.11.5/src/ipc/channel.rs`). Đã thêm
   `tauri = { version = "=2.11.5", features = ["specta"] }` vào
   `src-tauri/Cargo.toml` (dù `tauri-specta` cũng kéo feature này transitively,
   khai báo tường minh để không phụ thuộc vào unify feature ngẫu nhiên).

3. **Toolchain local 1.92 không build được `specta` rc.25.** `specta-2.0.0-rc.25`
   gọi `std::fmt::from_fn` (tính năng `debug_closure_helpers`/`fmt_from_fn`,
   tracking issue rust-lang/rust#146705) — hàm này **ổn định từ Rust 1.93.0**,
   không tồn tại trên Rust 1.92 stable. Trên máy dev, `rustc 1.92.0` build
   `specta` lỗi `E0658: use of unstable library feature`. Đã nâng toolchain
   local lên `stable` mới nhất qua `rustup update stable` (lúc kiểm là
   `1.98.1`) và build lại thành công. **Hệ quả cho CI:** không ghim rustc đúng
   `1.92` như Code Map ban đầu giả định; `.github/workflows/test.yml` dùng
   `dtolnay/rust-toolchain@stable` (kênh `stable`, không ghim version) để luôn
   có Rust ≥ 1.93. `Cargo.toml` khai báo `rust-version = "1.93"` cho khớp
   ràng buộc thật do `specta` rc.25 đặt ra (MSRV của riêng Tauri 2.11.5 là
   1.77.2 nhưng không còn là giới hạn quyết định).

4. **specta-typescript cấm xuất kiểu "BigInt" theo mặc định.** Lần export đầu
   tiên của spike (với `seq: u64`, `count: u64`) lỗi:
   `Attempted to export "" but Specta forbids exporting BigInt-style types
   (usize, isize, i64, u64, i128, u128) to avoid precision loss`. Đây là hành
   vi mặc định của `specta-typescript = 0.0.12` (an toàn theo mặc định vì JS
   `number` không biểu diễn chính xác integer > 2^53, còn JS `BigInt` không
   tự nhiên qua được JSON). Sửa spike: đổi `seq`/`count` sang `u32` (đủ dùng —
   theo AD-3, `seq` là bộ đếm đơn điệu theo instance stream, `u32` cho ~4.29 tỷ
   giá trị là quá đủ cho một phiên/app chạy trên desktop). **Khuyến nghị cho
   các story dùng `seq`/counter tương tự (live, job, v.v.): dùng `u32`, không
   dùng `u64`, trừ khi cố tình bật `enable_lossless_bigints()` (qua
   `Builder::semantic_types`) kèm một tầng transform runtime có thể mang JS
   `BigInt` qua transport một cách lossless — ngoài phạm vi story này.**

5. **Runtime: dựng `Channel<SpikeEvent>` thật, không qua mock WebView.**
   `tauri::ipc::Channel::new<F: Fn(InvokeResponseBody) -> Result<()>>(f)` là
   constructor Runtime-agnostic công khai (không cần `App`/`Webview`/`R:
   Runtime`). Spike dựng một `Channel` như vậy với closure ghi vào
   `Arc<Mutex<Vec<SpikeEvent>>>`, gọi thẳng hàm `spike_stream(channel, 5)` (bỏ
   qua lớp IPC dispatch — hàm gốc vẫn gọi được trực tiếp vì `#[tauri::command]`
   chỉ thêm wrapper bên cạnh, không thay hàm gốc), rồi assert nhận đủ 5 event
   và `seq` đúng thứ tự `0,1,2,3,4`. Con đường này dùng đúng
   `TSend: IpcResponse` (`impl<T: Serialize> IpcResponse for T`) và
   `ChannelInner::on_message` — chính là phần "typed Channel" mà AD-3 phụ
   thuộc (serialize đúng kiểu + phát đúng thứ tự gửi).

   **Có chủ đích không đi hết qua `tauri::test::mock_builder()` +
   `get_ipc_response()`.** Con đường đó mô phỏng một lời gọi IPC đầy đủ (qua
   `Webview::on_message`), nhưng cơ chế truyền dữ liệu qua Channel thật sự
   (`plugin:__TAURI_CHANNEL__|fetch`, hàng đợi `ChannelDataIpcQueue`,
   `mapChannel` phía JS) chỉ có ý nghĩa khi có một runtime JS thật thực thi
   `window.__TAURI_INTERNALS__`; giả lập phần đó trong test Rust thuần sẽ không
   kiểm thêm được gì ngoài những gì Tauri core đã tự test, mà lại kéo theo độ
   phức tạp/giòn lớn cho một spike. Test trực tiếp `Channel::new` + gọi hàm đã
   kiểm đúng phần rủi ro thực sự của việc dùng `Channel<T>` typed sinh tự động
   (serialize kiểu Rust cụ thể, thứ tự phát), nên được coi là "đủ khả thi" theo
   nghĩa spec cho phép.

## Kết luận

**`Channel<T>` typed CHẠY ĐƯỢC** với `tauri-specta = 2.0.0-rc.25` +
`tauri = 2.11.5`, không cần bọc type viết tay — AD-3 giữ nguyên như đã ghi
trong Architecture Spine. Điều kiện để nó chạy đúng trong các story sau:

- Bật feature `specta` trên crate `tauri` (đã làm ở `Cargo.toml`).
- Trường nào đi qua Channel/command/event mà cần một bộ đếm lớn kiểu `seq`
  phải dùng `u32` (hoặc kiểu ≤32-bit khác), **không dùng `u64`/`usize`** trừ
  khi cố tình thiết kế đường BigInt lossless.
- Toolchain build (local + CI) phải là Rust stable ≥ 1.93 (khuyến nghị theo
  kênh `stable`, không ghim một bản cụ thể) — khác với giả định "Rust 1.92"
  ghi trong Code Map lúc lập spec.

Command `spike_stream` và builder của nó chỉ tồn tại trong
`#[cfg(test)]`, không đăng ký vào `ipc::specta_builder()`/`lib.rs`, và không
xuất hiện trong `src/lib/bindings.ts` — đúng ràng buộc Never của story 1.1.

## Hệ quả

Kết luận trên gắn chặt với các bản pin exact hiện tại (`tauri = 2.11.5`,
`tauri-specta`/`specta = 2.0.0-rc.25`, `specta-typescript = 0.0.12`): nâng bất
kỳ bản nào trong số này bắt buộc phải chạy lại các test trong
`src-tauri/src/ipc/spike_channel.rs` (`cargo test --manifest-path
src-tauri/Cargo.toml spike_channel`) và xác nhận lại ADR này trước khi coi
Channel<T> typed vẫn còn đúng như mô tả.
