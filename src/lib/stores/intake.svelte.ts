// Store domain `intake` (story 2.8): the single place every way a file
// enters the app — dialog (`transcribe_pick_files`) or drag-drop — funnels
// through before calling `jobsStore.start(path)` (spec Approach: "mọi đường
// vào dồn về một hàng xử lý tuần tự phía UI gọi `transcribe_start` từng
// file"). Overlapping calls (a dialog pick racing a drop, or two drops) are
// chained so a batch always finishes — including navigation — before the
// next one starts (spec Tasks: "`submit(paths)` tuần tự (chuỗi hoá các lần
// gọi chồng)").
import { push } from '@keenmate/svelte-spa-router';
import { errorHint, errorTitle } from '../errors';
import { i18n } from '../../i18n/index.svelte';
import { commands, type AppError, type TranscribeStartOutcome } from '../bindings';
import { jobsStore } from './jobs.svelte';

export type IntakeNoticeVariant = 'info' | 'warning' | 'danger';

/** One dismissible row in the notices list (spec Always: "danh sách thông
 * báo có thể đóng"). `title` is always the file's basename (spec Always:
 * "Tên file hiển thị là basename") — never a full path. */
export type IntakeNotice = {
  id: string;
  variant: IntakeNoticeVariant;
  title: string;
  message: string;
  actionLabel?: string;
  actionHref?: string;
};

/** Strips any directory prefix (`/` or `\`) so a notice never leaks a full
 * path (spec Always: "Tên file hiển thị là basename"). */
export function basename(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const segments = normalized.split('/').filter((segment) => segment.length > 0);
  return segments.length > 0 ? segments[segments.length - 1] : path;
}

let noticeSeq = 0;
function nextNoticeId(): string {
  noticeSeq += 1;
  return `intake-notice-${noticeSeq}`;
}

function outcomeNotice(name: string, outcome: TranscribeStartOutcome): IntakeNotice {
  if (outcome.kind === 'existing') {
    return {
      id: nextNoticeId(),
      variant: 'info',
      title: name,
      message: i18n.t('intake.notice.existingSession'),
    };
  }
  if (outcome.kind === 'existingJob') {
    return {
      id: nextNoticeId(),
      variant: 'info',
      title: name,
      message: i18n.t('intake.notice.existingJob'),
    };
  }
  return {
    id: nextNoticeId(),
    variant: 'info',
    title: name,
    message: i18n.t('intake.notice.queued'),
  };
}

/** `error.code` distinguishes "sai định dạng" from "không có audio" even
 * though both share `category: 'format'` (spec Code Map: `error.format.hint`
 * nói về API key nên câu nhận file cần khoá i18n riêng, không dùng chung
 * `errorTitle`/`errorHint`). */
function errorNotice(name: string, error: AppError): IntakeNotice {
  if (error.code === 'format') {
    return {
      id: nextNoticeId(),
      variant: 'danger',
      title: name,
      message: i18n.t('intake.error.wrongFormat'),
    };
  }
  if (error.code === 'noaudio') {
    return {
      id: nextNoticeId(),
      variant: 'danger',
      title: name,
      message: i18n.t('intake.error.noAudio'),
    };
  }
  if (error.category === 'auth') {
    return {
      id: nextNoticeId(),
      variant: 'warning',
      title: name,
      message: i18n.t('home.banner.keyMissingMessage'),
      actionLabel: i18n.t('home.banner.keyMissingAction'),
      actionHref: '/settings/gemini',
    };
  }
  return {
    id: nextNoticeId(),
    variant: 'danger',
    title: name,
    message: `${errorTitle(error)} — ${errorHint(error)}`,
  };
}

function successHref(outcome: TranscribeStartOutcome): string {
  return `/session/${outcome.sessionId}`;
}

export function createIntakeStore() {
  let notices = $state<IntakeNotice[]>([]);
  let picking = $state(false);
  let processing = $state(false);
  // Chuỗi hoá mọi batch — xem doc module ở trên.
  let chain: Promise<void> = Promise.resolve();

  function dismiss(id: string): void {
    notices = notices.filter((notice) => notice.id !== id);
  }

  async function submitOne(path: string): Promise<string | null> {
    const name = basename(path);
    let result: TranscribeStartOutcome | { error: AppError };
    try {
      result = await jobsStore.start(path);
    } catch {
      notices = [
        ...notices,
        errorNotice(name, {
          category: 'network',
          code: 'network',
          detailRedacted: 'transcribe start unavailable',
        }),
      ];
      return null;
    }
    if ('error' in result) {
      notices = [...notices, errorNotice(name, result.error)];
      return null;
    }
    notices = [...notices, outcomeNotice(name, result)];
    return successHref(result);
  }

  async function processBatch(paths: string[]): Promise<void> {
    processing = true;
    let target: string | null = null;
    try {
      // Tuần tự theo thứ tự nhận (spec Always: "Nhiều file ... xử lý tuần
      // tự theo thứ tự nhận") — file lỗi không chặn file sau.
      for (const path of paths) {
        const href = await submitOne(path);
        if (href && !target) target = href;
      }
    } finally {
      processing = false;
    }
    // Kết quả thành công đầu tiên theo thứ tự (spec Always); không có thì ở
    // lại màn hiện tại.
    if (target) {
      await push(target);
    }
  }

  /** Xếp một batch vào chuỗi xử lý — không bao giờ chạy chồng lên batch
   * trước (spec Tasks). */
  function submit(paths: string[]): Promise<void> {
    if (paths.length === 0) return Promise.resolve();
    const next = chain.then(() => processBatch(paths));
    // Một batch lỗi không được làm kẹt chuỗi cho batch sau.
    chain = next.catch(() => undefined);
    return next;
  }

  /** Mở dialog chọn nhiều file rồi dồn kết quả qua `submit` (spec Tasks:
   * "`pick()`"). Huỷ dialog (`Ok([])`) không làm gì. */
  async function pick(): Promise<void> {
    if (picking) return;
    picking = true;
    try {
      const result = await commands.transcribePickFiles();
      if (result.status === 'ok' && result.data.length > 0) {
        await submit(result.data);
      }
    } catch {
      // Dialog không mở được — im lặng, giống huỷ dialog (không có gì để
      // báo lỗi vì chưa có file nào được chọn).
    } finally {
      picking = false;
    }
  }

  /** Test-only seam: resets every field without touching a live chain. */
  function reset(): void {
    notices = [];
    picking = false;
    processing = false;
    chain = Promise.resolve();
  }

  return {
    get notices() {
      return notices;
    },
    get picking() {
      return picking;
    },
    get processing() {
      return processing;
    },
    pick,
    submit,
    dismiss,
    reset,
  };
}

export const intakeStore = createIntakeStore();
