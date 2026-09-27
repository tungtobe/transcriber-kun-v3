// Store domain `notes` (story 3.5, spec Approach): tự lưu ghi chú của một
// Phiên sau debounce 800 ms, revision tăng dần do frontend cấp (spec Design
// Notes: "Revision do frontend cấp ... nên ACK/lệnh về sai thứ tự không thể
// ghi đè bản mới, kể cả khi hai lời gọi chạy song song"). Theo mẫu
// `createXStore()` (`library.svelte.ts`) — per-session buffer trong hai
// `Map` (một phần hiển thị được `$state`, một phần điều khiển thuần không
// cần reactive: debounce handle, promise đang bay).
//
// Trạng thái không bao giờ bị xoá khi một Phiên "rời màn" (`flush` gọi từ
// `NotesPanel`/`Session.svelte`/`appStore`) -- nếu flush đó thất bại, dòng
// vẫn còn "dirty" trong store để `flushAll()` (luồng đóng app) thử lại, kể
// cả khi không còn `NotesPanel` nào đang hiển thị Phiên đó.
import { untrack } from 'svelte';
import { commands, type NotesSaveOutcome } from '../bindings';

/** Giới hạn độ dài ghi chú (spec Boundaries Always: "Body giới hạn 100 000
 * ký tự") -- dùng làm `maxlength` trên textarea để không cho gõ vượt, dù
 * Rust vẫn là nguồn thật của giới hạn này. */
export const MAX_NOTE_BODY_LENGTH = 100_000;

const DEBOUNCE_MS = 800;

export type NoteStatus = 'idle' | 'dirty' | 'saving' | 'saved' | 'error';

export interface NoteViewState {
  body: string;
  status: NoteStatus;
  /** `updatedAt` của ACK gần nhất phản ánh đúng `body` hiện tại -- chỉ có
   * nghĩa khi `status === 'saved'` (spec Boundaries Always: "chỉ khi không
   * còn thay đổi chưa ACK"). */
  savedAtMs: number | null;
  /** `true` khi `notesGet` lúc mở panel lỗi -- textarea bị khoá (không cho
   * gõ đè lên một bản có thể còn tồn tại trong DB) tới khi tải lại thành
   * công. */
  loadError: boolean;
}

const EMPTY_VIEW: NoteViewState = {
  body: '',
  status: 'idle',
  savedAtMs: null,
  loadError: false,
};

/** Trạng thái điều khiển thuần (không hiển thị trực tiếp) mỗi Phiên -- tách
 * khỏi `entries` ($state) vì debounce handle/promise không cần, và không nên,
 * kích một re-render. */
interface Control {
  /** Revision cao nhất đã *gửi* trong bất kỳ lời gọi `notesSave` nào (thành
   * công, lỗi, hay đang bay) -- mỗi lần gửi dùng giá trị này + 1 (spec
   * Boundaries Always), đảm bảo không bao giờ gửi trùng/lùi revision kể cả
   * sau một lần lưu lỗi. */
  lastSentRevision: number;
  /** Revision cao nhất đã *ACK* thành công (`Saved`) -- chỉ tăng, không bao
   * giờ lùi (spec Always: "ACK về sai thứ tự không được làm lùi trạng
   * thái"). */
  ackedRevision: number;
  ackedAtMs: number | null;
  /** `body` chính xác đã nằm trong lần gửi gần nhất -- so với `entries.body`
   * hiện tại để biết có "thay đổi chưa gửi" hay không. `null` = chưa từng
   * gửi lần nào (dùng làm cờ ép gửi lại ở `retry`). */
  bodyAtLastSend: string | null;
  /** Số lời gọi `notesSave` đang bay cùng lúc -- có thể > 1 thật sự (không
   * phải chỉ tuần tự): một `flush()`/debounce mới có thể gửi trước khi lần
   * gửi trước đó ACK về, đúng kịch bản spec I/O Matrix "ACK sai thứ tự" ("gửi
   * rev 6 rồi 7; ACK 7 về trước 6"). */
  inFlight: number;
  hasError: boolean;
  debounceHandle: ReturnType<typeof setTimeout> | null;
  /** Resolve khi `inFlight` về 0 -- `flush`/`ensureSent` chờ cái này thay vì
   * chờ từng promise riêng lẻ. Tạo mới mỗi khi `inFlight` chuyển 0 → 1, resolve
   * khi chuyển ngược lại 1 → 0 (xem [markDispatchStart]/[markDispatchEnd]). */
  settled: Promise<void> | null;
  resolveSettled: (() => void) | null;
  /** Tăng mỗi lần `load()` được gọi cho Phiên này -- kết quả `notesGet` bay
   * về sau một `load()` mới hơn bị bỏ qua (spec Always tương tự
   * `Session.svelte`'s `loadToken`). */
  loadToken: number;
}

function newControl(): Control {
  return {
    lastSentRevision: 0,
    ackedRevision: 0,
    ackedAtMs: null,
    bodyAtLastSend: null,
    inFlight: 0,
    hasError: false,
    debounceHandle: null,
    settled: null,
    resolveSettled: null,
    loadToken: 0,
  };
}

function markDispatchStart(control: Control): void {
  if (control.inFlight === 0) {
    control.settled = new Promise<void>((resolve) => {
      control.resolveSettled = resolve;
    });
  }
  control.inFlight += 1;
}

function markDispatchEnd(control: Control): void {
  control.inFlight -= 1;
  if (control.inFlight === 0) {
    control.resolveSettled?.();
    control.resolveSettled = null;
  }
}

export function createNotesStore() {
  let entries = $state<Map<string, NoteViewState>>(new Map());
  const controls = new Map<string, Control>();

  function getEntry(sessionId: string): NoteViewState {
    return entries.get(sessionId) ?? EMPTY_VIEW;
  }

  // `untrack` quanh lần đọc `entries` cũ: `load()` gọi `setEntry` đồng bộ
  // ngay trong `$effect` mount của `NotesPanel` (trước bất kỳ `await` thật
  // nào nếu `commands.notesGet` từ chối/ném lỗi đồng bộ) -- không `untrack`,
  // Svelte quy lần đọc này cho chính effect đó, rồi thấy effect "vừa đọc vừa
  // ghi" `entries` trong cùng một lượt chạy và báo
  // `effect_update_depth_exceeded` (tự re-run effect vô hạn). `entries` vẫn
  // là nguồn thật duy nhất -- `untrack` chỉ tắt việc *theo dõi* lần đọc này
  // làm dependency, không đổi giá trị đọc được.
  function setEntry(sessionId: string, next: NoteViewState): void {
    const map = untrack(() => new Map(entries));
    map.set(sessionId, next);
    entries = map;
  }

  function hasUnsaved(sessionId: string, control: Control): boolean {
    return (
      control.hasError ||
      control.debounceHandle !== null ||
      getEntry(sessionId).body !== (control.bodyAtLastSend ?? getEntry(sessionId).body)
    );
  }

  function getControl(sessionId: string): Control {
    let control = controls.get(sessionId);
    if (!control) {
      control = newControl();
      controls.set(sessionId, control);
    }
    return control;
  }

  /** Tính lại `status`/`savedAtMs` hiển thị từ `Control` + `body` hiện tại --
   * gọi lại sau mỗi thay đổi (gõ, bắt đầu gửi, ACK, lỗi) thay vì rải logic
   * này ở nhiều chỗ. */
  function sync(sessionId: string): void {
    const control = getControl(sessionId);
    const current = getEntry(sessionId);
    let status: NoteStatus;
    let savedAtMs = current.savedAtMs;
    if (control.inFlight > 0) {
      status = 'saving';
    } else if (control.hasError) {
      status = 'error';
    } else if (
      control.ackedRevision === control.lastSentRevision &&
      current.body === control.bodyAtLastSend
    ) {
      status = control.ackedRevision === 0 ? 'idle' : 'saved';
      savedAtMs = control.ackedAtMs;
    } else {
      status = 'dirty';
    }
    setEntry(sessionId, { ...current, status, savedAtMs });
  }

  /** Gửi đúng một lần `notesSave` với `body` hiện tại, xử lý outcome (spec
   * Boundaries Always) rồi đồng bộ trạng thái hiển thị. Có thể chạy song song
   * với một `dispatchOnce` khác cho cùng Phiên (revision khác nhau, xem
   * `Control::inFlight`) -- không chờ lần gửi trước xong mới gửi lần này. */
  async function dispatchOnce(sessionId: string): Promise<void> {
    const control = getControl(sessionId);
    const bodySent = getEntry(sessionId).body;
    const revision = control.lastSentRevision + 1;
    control.lastSentRevision = revision;
    control.bodyAtLastSend = bodySent;
    markDispatchStart(control);
    sync(sessionId);
    try {
      const result = await commands.notesSave(sessionId, bodySent, revision);
      if (result.status !== 'ok') {
        control.hasError = true;
      } else {
        await applyOutcome(sessionId, bodySent, result.data);
      }
    } catch {
      control.hasError = true;
    } finally {
      markDispatchEnd(control);
      sync(sessionId);
    }
  }

  async function applyOutcome(
    sessionId: string,
    bodySent: string,
    outcome: NotesSaveOutcome,
  ): Promise<void> {
    const control = getControl(sessionId);
    if (outcome.kind === 'saved') {
      if (outcome.revision > control.ackedRevision) {
        control.ackedRevision = outcome.revision;
        control.ackedAtMs = outcome.updatedAt ?? Date.now();
      }
      control.hasError = false;
      sync(sessionId);
      return;
    }
    if (outcome.kind === 'notFound') {
      // Phiên đã bị xoá đồng thời -- không còn gì để lưu vào, hiện như một
      // lỗi lưu bình thường (buffer vẫn giữ nguyên, spec Never: "không mất
      // ký tự nào tự ý").
      control.hasError = true;
      sync(sessionId);
      return;
    }
    // `Stale` (spec Design Notes: "nếu nhận `Stale` (vd hai cửa sổ), store
    // nạp lại bản trong DB chỉ khi buffer không có thay đổi chưa lưu; ngược
    // lại gửi lại với revision = stale + 1").
    const stillMatches = getEntry(sessionId).body === bodySent;
    if (stillMatches) {
      await reloadFromServer(sessionId);
      return;
    }
    control.lastSentRevision = Math.max(control.lastSentRevision, outcome.revision);
    await dispatchOnce(sessionId);
  }

  async function reloadFromServer(sessionId: string): Promise<void> {
    const control = getControl(sessionId);
    try {
      const result = await commands.notesGet(sessionId);
      if (result.status === 'ok' && result.data) {
        control.ackedRevision = result.data.revision;
        control.lastSentRevision = Math.max(control.lastSentRevision, result.data.revision);
        control.ackedAtMs = result.data.updatedAt ?? Date.now();
        control.bodyAtLastSend = result.data.body;
        control.hasError = false;
        setEntry(sessionId, {
          body: result.data.body,
          status: 'saved',
          savedAtMs: control.ackedAtMs,
          loadError: false,
        });
        return;
      }
    } catch {
      // rơi xuống nhánh lỗi bên dưới.
    }
    control.hasError = true;
    sync(sessionId);
  }

  /** Đảm bảo `body` hiện tại đã (hoặc đang) được gửi, rồi trả một promise
   * settle khi mọi lần gửi đang bay cho Phiên này (kể cả lần gửi vừa bắt đầu
   * ở đây) đã xong -- nguồn dùng chung cho debounce hết giờ và `flush()`.
   * Gửi ngay (không chờ một lần gửi trước đó xong) khi `body` khác lần gửi
   * gần nhất -- spec Design Notes: "hai lời gọi chạy song song" (`flush()`
   * trong lúc một lần gửi trước còn đang bay phải gửi nội dung mới nhất
   * ngay, không xếp hàng chờ). */
  function ensureSent(sessionId: string): Promise<void> {
    const control = getControl(sessionId);
    const body = getEntry(sessionId).body;
    if (body !== control.bodyAtLastSend) {
      void dispatchOnce(sessionId);
    }
    return control.settled ?? Promise.resolve();
  }

  function clearDebounce(control: Control): void {
    if (control.debounceHandle !== null) {
      clearTimeout(control.debounceHandle);
      control.debounceHandle = null;
    }
  }

  /** Tải ghi chú đã lưu của một Phiên -- gọi lúc `NotesPanel` mount hoặc đổi
   * `sessionId` (spec I/O Matrix "Bền qua đóng đột ngột": mở lại phải thấy
   * đúng bản đã ACK). Bỏ qua nếu Phiên này còn một thao tác gửi đang bay từ
   * một lần mount trước (tránh đè lên state chưa ổn định của nó) -- rất hiếm,
   * chỉ xảy ra khi mở lại đúng Phiên đó gần như ngay lập tức. */
  async function load(sessionId: string): Promise<void> {
    const existing = controls.get(sessionId);
    if (existing && existing.inFlight > 0) return;
    // Buffer còn thay đổi chưa ACK (lưu lỗi / debounce đang chờ) thì không
    // nạp đè từ DB -- spec Always: "lỗi giữ buffer, không mất chữ" (vd rời
    // màn khi flush lỗi rồi quay lại).
    if (existing && hasUnsaved(sessionId, existing)) return;
    const control = getControl(sessionId);
    const token = ++control.loadToken;
    setEntry(sessionId, { ...EMPTY_VIEW });
    try {
      const result = await commands.notesGet(sessionId);
      if (token !== control.loadToken) return;
      if (result.status !== 'ok') {
        setEntry(sessionId, { ...EMPTY_VIEW, loadError: true });
        return;
      }
      const data = result.data;
      const typedDuringLoad = hasUnsaved(sessionId, control);
      control.lastSentRevision = data?.revision ?? 0;
      control.ackedRevision = data?.revision ?? 0;
      control.ackedAtMs = data?.updatedAt ?? null;
      control.bodyAtLastSend = data?.body ?? '';
      control.hasError = false;
      // Người dùng đã gõ trong lúc chờ `notesGet`: giữ chữ đang gõ, chỉ nhận
      // revision từ DB để lần lưu kế tiếp (debounce đã hẹn) vượt revision đó.
      if (typedDuringLoad) {
        const typed = getEntry(sessionId);
        setEntry(sessionId, { ...typed, loadError: false });
        return;
      }
      setEntry(sessionId, {
        body: data?.body ?? '',
        status: data ? 'saved' : 'idle',
        savedAtMs: control.ackedAtMs,
        loadError: false,
      });
    } catch {
      if (token !== control.loadToken) return;
      setEntry(sessionId, { ...EMPTY_VIEW, loadError: true });
    }
  }

  /** Gõ vào textarea -- đặt lại debounce 800 ms (spec Boundaries Always:
   * "mỗi lần gõ đặt lại debounce"). Bỏ qua khi đang ở trạng thái lỗi tải
   * (textarea bị khoá ở component, hàm này không tự kiểm tra lại). */
  function setBody(sessionId: string, body: string): void {
    const current = getEntry(sessionId);
    if (current.body === body) return;
    setEntry(sessionId, { ...current, body });
    const control = getControl(sessionId);
    sync(sessionId);
    clearDebounce(control);
    control.debounceHandle = setTimeout(() => {
      control.debounceHandle = null;
      void ensureSent(sessionId);
    }, DEBOUNCE_MS);
  }

  /** Nút "Thử lại" sau một lần lưu lỗi -- ép gửi lại dù `body` chưa đổi kể từ
   * lần gửi thất bại (spec I/O Matrix "Lưu lỗi": "Thử lại lưu được"). */
  async function retry(sessionId: string): Promise<boolean> {
    const control = getControl(sessionId);
    clearDebounce(control);
    control.bodyAtLastSend = null;
    await ensureSent(sessionId);
    return getEntry(sessionId).status !== 'error';
  }

  /** Huỷ debounce, gửi ngay nội dung mới nhất nếu có, và chờ ACK/lỗi ổn định
   * -- dùng bởi `NotesPanel` (unmount/đổi tab/đổi Phiên) và `appStore`
   * (đóng app). Trả `true` khi kết thúc ở trạng thái không lỗi. */
  async function flush(sessionId: string): Promise<boolean> {
    const control = controls.get(sessionId);
    if (!control) return true;
    clearDebounce(control);
    await ensureSent(sessionId);
    return getEntry(sessionId).status !== 'error';
  }

  /** Flush mọi Phiên đang được theo dõi (kể cả một Phiên đã rời màn từ lâu
   * mà flush lúc đó lỗi) -- dùng bởi luồng đóng app (spec Code Map: "nghe
   * `closeRequested`: flush ghi chú trước"). Trả `true` chỉ khi tất cả đều
   * kết thúc không lỗi. */
  async function flushAll(): Promise<boolean> {
    const sessionIds = [...controls.keys()];
    const results = await Promise.all(sessionIds.map((id) => flush(id)));
    return results.every(Boolean);
  }

  function view(sessionId: string): NoteViewState {
    return getEntry(sessionId);
  }

  /** Test-only seam: xoá sạch state, huỷ mọi debounce đang chờ. */
  function reset(): void {
    for (const control of controls.values()) {
      clearDebounce(control);
    }
    controls.clear();
    entries = new Map();
  }

  return {
    view,
    load,
    setBody,
    retry,
    flush,
    flushAll,
    reset,
  };
}

export const notesStore = createNotesStore();
