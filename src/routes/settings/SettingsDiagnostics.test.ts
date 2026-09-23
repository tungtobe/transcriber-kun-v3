// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { diagnosticsStore } from '../../lib/stores/diagnostics.svelte';
import SettingsDiagnostics from './SettingsDiagnostics.svelte';

const mocks = vi.hoisted(() => ({
  diagnosticsSummary: vi.fn(),
  diagnosticsExport: vi.fn(),
  diagnosticsClearLogs: vi.fn(),
}));

vi.mock('../../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      diagnosticsSummary: (...args: unknown[]) => mocks.diagnosticsSummary(...args),
      diagnosticsExport: (...args: unknown[]) => mocks.diagnosticsExport(...args),
      diagnosticsClearLogs: (...args: unknown[]) => mocks.diagnosticsClearLogs(...args),
    },
  };
});

function summary(overrides: Partial<{ sessions: number; crashes: number; authCount: number }> = {}) {
  return {
    sessions: overrides.sessions ?? 0,
    crashes: overrides.crashes ?? 0,
    errorsByCategory: [
      { category: 'quota', count: 0 },
      { category: 'auth', count: overrides.authCount ?? 0 },
      { category: 'model', count: 0 },
      { category: 'network', count: 0 },
      { category: 'format', count: 0 },
      { category: 'permission', count: 0 },
      { category: 'storage', count: 0 },
      { category: 'blocked', count: 0 },
    ],
  };
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  diagnosticsStore.reset();
  mocks.diagnosticsSummary.mockReset().mockResolvedValue({ status: 'ok', data: summary() });
  mocks.diagnosticsExport.mockReset();
  mocks.diagnosticsClearLogs.mockReset();
});

describe('SettingsDiagnostics', () => {
  it('shows sessions, crashes, and all eight error categories with zero counts', async () => {
    render(SettingsDiagnostics);

    // The counter values fall back to `0` synchronously (before the async
    // load resolves), so wait on a category label instead — those only
    // render once the real summary has loaded.
    expect(await screen.findByText('Hết hạn mức')).toBeTruthy();
    expect(screen.getAllByText('0').length).toBeGreaterThan(0);
    expect(screen.getByText('Số phiên')).toBeTruthy();
    expect(screen.getByText('Số lần thoát không sạch')).toBeTruthy();
    for (const label of [
      'Hết hạn mức',
      'Key không hợp lệ',
      'Sự cố mô hình',
      'Lỗi kết nối',
      'Định dạng không đúng',
      'Thiếu quyền',
      'Lỗi kho khoá',
      'Đang bị chặn',
    ]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
  });

  it('renders non-zero counts once loaded', async () => {
    mocks.diagnosticsSummary.mockResolvedValue({
      status: 'ok',
      data: summary({ sessions: 5, crashes: 2, authCount: 3 }),
    });
    render(SettingsDiagnostics);

    expect(await screen.findByText('5')).toBeTruthy();
    expect(screen.getByText('2')).toBeTruthy();
    expect(screen.getByText('3')).toBeTruthy();
  });

  it('never renders a checkbox (no opt-in/statistics toggle)', async () => {
    render(SettingsDiagnostics);
    await waitFor(() => expect(mocks.diagnosticsSummary).toHaveBeenCalled());

    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('exports the bundle and shows a success status', async () => {
    mocks.diagnosticsExport.mockResolvedValue({ status: 'ok', data: true });
    render(SettingsDiagnostics);

    await fireEvent.click(screen.getByRole('button', { name: 'Xuất gói nhật ký' }));

    expect(await screen.findByText('Đã lưu gói chẩn đoán.')).toBeTruthy();
  });

  it('shows a cancelled status (not an error) when the dialog is dismissed', async () => {
    mocks.diagnosticsExport.mockResolvedValue({ status: 'ok', data: false });
    render(SettingsDiagnostics);

    await fireEvent.click(screen.getByRole('button', { name: 'Xuất gói nhật ký' }));

    expect(await screen.findByText('Đã huỷ, chưa lưu file nào.')).toBeTruthy();
  });

  it('shows a storage banner when export fails', async () => {
    mocks.diagnosticsExport.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'disk full' },
    });
    render(SettingsDiagnostics);

    await fireEvent.click(screen.getByRole('button', { name: 'Xuất gói nhật ký' }));

    const banner = await screen.findByText('Không thể lưu hoặc đọc key. Thử lại hoặc khởi động lại ứng dụng.');
    expect(within(banner.closest('.banner') as HTMLElement).getByText('Lỗi kho khoá')).toBeTruthy();
  });

  it('clears logs and shows a success status', async () => {
    mocks.diagnosticsClearLogs.mockResolvedValue({ status: 'ok', data: null });
    render(SettingsDiagnostics);

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá nhật ký' }));

    expect(await screen.findByText('Đã xoá nhật ký cũ.')).toBeTruthy();
  });
});
