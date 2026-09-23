// Domain store `diagnostics`: the only frontend owner of `diagnosticsSummary`
// / `diagnosticsClearLogs` / `diagnosticsExport`. No checkbox, no opt-in, no
// network request lives here (spec Never) — this store only reads local
// counters and triggers the Rust-owned save dialog / log clear.
import { commands, type AppError, type DiagnosticsSummary } from '../bindings';

export type DiagnosticsStatus = 'idle' | 'loading' | 'ready' | 'error';
export type DiagnosticsActionStatus = 'idle' | 'running' | 'done' | 'error';

export function createDiagnosticsStore() {
  let summary = $state<DiagnosticsSummary | null>(null);
  let status = $state<DiagnosticsStatus>('idle');
  let error = $state<AppError | null>(null);

  // "Xuất gói": `true` sau khi lưu thành công, `false` khi người dùng huỷ
  // dialog (không phải lỗi — spec Always: "Huỷ dialog → trả `false`, không
  // lỗi"). `exportError` chỉ khác `null` khi command thật sự trả `Err`.
  let exportStatus = $state<DiagnosticsActionStatus>('idle');
  let exportError = $state<AppError | null>(null);
  let lastExportSaved = $state<boolean | null>(null);

  let clearStatus = $state<DiagnosticsActionStatus>('idle');
  let clearError = $state<AppError | null>(null);

  let activeLoad: Promise<void> | undefined;

  function load(): Promise<void> {
    if (activeLoad) return activeLoad;
    status = 'loading';
    error = null;
    const request = (async () => {
      try {
        const result = await commands.diagnosticsSummary();
        if (result.status === 'ok') {
          summary = result.data;
          status = 'ready';
        } else {
          error = result.error;
          status = 'error';
        }
      } catch {
        error = null;
        status = 'error';
      }
    })().finally(() => {
      if (activeLoad === request) activeLoad = undefined;
    });
    activeLoad = request;
    return request;
  }

  async function exportBundle(): Promise<void> {
    if (exportStatus === 'running') return;
    exportStatus = 'running';
    exportError = null;
    try {
      const result = await commands.diagnosticsExport();
      if (result.status === 'ok') {
        lastExportSaved = result.data;
        exportStatus = 'done';
      } else {
        exportError = result.error;
        exportStatus = 'error';
      }
    } catch {
      exportError = { category: 'storage', code: 'storage', detailRedacted: 'export unavailable' };
      exportStatus = 'error';
    }
  }

  async function clearLogs(): Promise<void> {
    if (clearStatus === 'running') return;
    clearStatus = 'running';
    clearError = null;
    try {
      const result = await commands.diagnosticsClearLogs();
      if (result.status === 'ok') {
        clearStatus = 'done';
      } else {
        clearError = result.error;
        clearStatus = 'error';
      }
    } catch {
      clearError = { category: 'storage', code: 'storage', detailRedacted: 'clear logs unavailable' };
      clearStatus = 'error';
    }
  }

  /** Test-only seam: clears all session state between isolated test cases. */
  function reset(): void {
    summary = null;
    status = 'idle';
    error = null;
    exportStatus = 'idle';
    exportError = null;
    lastExportSaved = null;
    clearStatus = 'idle';
    clearError = null;
    activeLoad = undefined;
  }

  return {
    get summary() { return summary; },
    get status() { return status; },
    get error() { return error; },
    get exportStatus() { return exportStatus; },
    get exportError() { return exportError; },
    get lastExportSaved() { return lastExportSaved; },
    get clearStatus() { return clearStatus; },
    get clearError() { return clearError; },
    load,
    exportBundle,
    clearLogs,
    reset,
  };
}

export const diagnosticsStore = createDiagnosticsStore();
