// Store domain `app`: bọc `commands.appVersion()` (binding sinh tự động) và giữ
// trạng thái loading/ok/error để UI không bao giờ hiện màn trắng (AD-13).
// `error` mang đúng kiểu `AppError` (category/code/detailRedacted) sinh từ
// story 1.2 khi có; `null` khi lỗi không phải một `AppError` có cấu trúc
// (invoke ném exception thẳng, ví dụ IPC chưa sẵn sàng).
import { commands, events, type AppError } from '../bindings';

export type AppVersionState =
  | { status: 'loading' }
  | { status: 'ok'; version: string }
  | { status: 'error'; error: AppError | null };

function createAppStore() {
  let version = $state<AppVersionState>({ status: 'loading' });

  async function loadVersion() {
    version = { status: 'loading' };
    try {
      const result = await commands.appVersion();
      if (result.status === 'ok') {
        version = { status: 'ok', version: result.data };
      } else {
        version = { status: 'error', error: result.error };
      }
    } catch {
      version = { status: 'error', error: null };
    }
  }

  /**
   * Story 3.5 (trước đó story 2.4): `CloseRequested` giờ phát mỗi lần cửa sổ
   * chính bị yêu cầu đóng, kèm `busy` (spec Code Map: "luôn emit (thêm cờ
   * `busy`) ... frontend quyết định") — trước đây Rust chỉ emit khi registry
   * bận và tự thoát thẳng ở nhánh rảnh, nên không có chỗ nào chắc chắn chạy
   * để flush ghi chú trước khi đóng. `CloseConfirm` gọi hàm này một lần lúc
   * mount; hàm unlisten trả về phải được gọi lúc unmount.
   */
  function listenForCloseRequested(callback: (busy: boolean) => void): Promise<() => void> {
    return events.closeRequested.listen((event) => callback(event.payload.busy));
  }

  /** Thật sự thoát: huỷ mọi Job hiện có rồi thoát (spec Design Notes: "đồng ý
   * → huỷ sạch rồi thoát" — không đổi hành vi Rust ở story 3.5). `CloseConfirm`
   * gọi hàm này sau khi flush ghi chú xong, dù rảnh (flush ok, tự thoát,
   * không dialog thừa) hay bận (người dùng đã bấm "Huỷ và thoát") hay flush
   * lỗi mà người dùng chọn "Vẫn thoát". */
  async function confirmClose(): Promise<void> {
    await commands.appCloseConfirm();
  }

  return {
    get version() {
      return version;
    },
    loadVersion,
    listenForCloseRequested,
    confirmClose,
  };
}

export const appStore = createAppStore();
