// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  diagnosticsSummary: vi.fn(),
  diagnosticsClearLogs: vi.fn(),
  diagnosticsExport: vi.fn(),
}));

vi.mock('../bindings', () => ({
  commands: {
    diagnosticsSummary: (...args: unknown[]) => mocks.diagnosticsSummary(...args),
    diagnosticsClearLogs: (...args: unknown[]) => mocks.diagnosticsClearLogs(...args),
    diagnosticsExport: (...args: unknown[]) => mocks.diagnosticsExport(...args),
  },
}));

function summary(overrides: Partial<{ sessions: number; crashes: number }> = {}) {
  return {
    sessions: overrides.sessions ?? 0,
    crashes: overrides.crashes ?? 0,
    errorsByCategory: [
      { category: 'quota', count: 0 },
      { category: 'auth', count: 0 },
      { category: 'model', count: 0 },
      { category: 'network', count: 0 },
      { category: 'format', count: 0 },
      { category: 'permission', count: 0 },
      { category: 'storage', count: 0 },
      { category: 'blocked', count: 0 },
    ],
  };
}

describe('diagnosticsStore', () => {
  beforeEach(() => {
    vi.resetModules();
    mocks.diagnosticsSummary.mockReset();
    mocks.diagnosticsClearLogs.mockReset();
    mocks.diagnosticsExport.mockReset();
  });

  it('loads the summary and exposes all eight categories', async () => {
    mocks.diagnosticsSummary.mockResolvedValue({ status: 'ok', data: summary({ sessions: 2, crashes: 1 }) });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.load();

    expect(store.summary?.sessions).toBe(2);
    expect(store.summary?.crashes).toBe(1);
    expect(store.summary?.errorsByCategory).toHaveLength(8);
    expect(store.status).toBe('ready');
  });

  it('surfaces a storage error from the summary command without throwing', async () => {
    mocks.diagnosticsSummary.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'disk full' },
    });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.load();

    expect(store.status).toBe('error');
    expect(store.error?.category).toBe('storage');
  });

  it('exportBundle reports true when the user saves the file', async () => {
    mocks.diagnosticsExport.mockResolvedValue({ status: 'ok', data: true });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.exportBundle();

    expect(store.exportStatus).toBe('done');
    expect(store.lastExportSaved).toBe(true);
    expect(store.exportError).toBeNull();
  });

  it('exportBundle reports false (not an error) when the dialog is cancelled', async () => {
    mocks.diagnosticsExport.mockResolvedValue({ status: 'ok', data: false });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.exportBundle();

    expect(store.exportStatus).toBe('done');
    expect(store.lastExportSaved).toBe(false);
    expect(store.exportError).toBeNull();
  });

  it('exportBundle surfaces a storage error banner when writing the file fails', async () => {
    mocks.diagnosticsExport.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'write failed' },
    });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.exportBundle();

    expect(store.exportStatus).toBe('error');
    expect(store.exportError?.category).toBe('storage');
  });

  it('clearLogs succeeds independently from export state', async () => {
    mocks.diagnosticsClearLogs.mockResolvedValue({ status: 'ok', data: null });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.clearLogs();

    expect(store.clearStatus).toBe('done');
    expect(store.clearError).toBeNull();
    expect(mocks.diagnosticsExport).not.toHaveBeenCalled();
  });

  it('clearLogs surfaces a storage error on IO failure', async () => {
    mocks.diagnosticsClearLogs.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'io error' },
    });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();

    await store.clearLogs();

    expect(store.clearStatus).toBe('error');
    expect(store.clearError?.category).toBe('storage');
  });

  it('reset clears every field back to idle', async () => {
    mocks.diagnosticsSummary.mockResolvedValue({ status: 'ok', data: summary() });
    const { createDiagnosticsStore } = await import('./diagnostics.svelte');
    const store = createDiagnosticsStore();
    await store.load();

    store.reset();

    expect(store.summary).toBeNull();
    expect(store.status).toBe('idle');
  });
});
