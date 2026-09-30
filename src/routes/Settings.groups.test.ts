// @vitest-environment jsdom
// Covers the Story 1.9 acceptance criteria that need `settingsStore` in a
// specific, controllable state: real content per group and the "Consent
// declined -> About only" no-regression guarantee (spec Acceptance).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import Settings from './Settings.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    uiLanguage: 'system' as 'system' | 'vi' | 'en' | 'ja',
    theme: 'system' as 'system' | 'light' | 'dark',
    consentStatus: 'current' as 'pending' | 'declined' | 'stale' | 'current',
    consentPolicy: {
      currentVersion: 1,
      privacyUrl: 'https://transkun.app/privacy',
      supportUrl: 'https://transkun.app/support',
      acceptedVersion: 1,
      declined: false,
      status: 'current' as const,
    },
    setUiLanguage: vi.fn(),
    setTheme: vi.fn(),
  },
  keysStore: {
    keys: [] as Array<{ id: string; label: string }>,
    status: 'ready' as 'idle' | 'loading' | 'ready' | 'error',
    error: null,
    checkStatus: 'idle' as 'idle' | 'checking' | 'done',
    checkResult: null,
    lastCheckedAt: null,
    hasUsableKey: false,
    modelLists: { transcribe: [], live: [], memo: [] },
    modelListStatus: { transcribe: 'idle', live: 'idle', memo: 'idle' },
    modelListError: { transcribe: null, live: null, memo: null },
    load: vi.fn(),
    checkKeys: vi.fn(),
    deleteKey: vi.fn(),
    loadModelList: vi.fn(),
  },
  diagnosticsSummary: vi.fn(),
  libraryStorageStats: vi.fn(),
  memoTemplatesList: vi.fn(),
  settingsRecommendedPreview: vi.fn(),
  settingsRecommendedApply: vi.fn(),
  settingsRecommendedCancel: vi.fn(),
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));
vi.mock('../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      diagnosticsSummary: (...args: unknown[]) => mocks.diagnosticsSummary(...args),
      libraryStorageStats: (...args: unknown[]) => mocks.libraryStorageStats(...args),
      memoTemplatesList: (...args: unknown[]) => mocks.memoTemplatesList(...args),
      settingsRecommendedPreview: (...args: unknown[]) => mocks.settingsRecommendedPreview(...args),
      settingsRecommendedApply: (...args: unknown[]) => mocks.settingsRecommendedApply(...args),
      settingsRecommendedCancel: (...args: unknown[]) => mocks.settingsRecommendedCancel(...args),
    },
  };
});

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.consentStatus = 'current';
  mocks.settingsStore.uiLanguage = 'system';
  mocks.settingsStore.theme = 'system';
  mocks.diagnosticsSummary.mockReset().mockResolvedValue({
    status: 'ok',
    data: {
      sessions: 3,
      crashes: 1,
      errorsByCategory: [
        { category: 'quota', count: 0 },
        { category: 'auth', count: 2 },
        { category: 'model', count: 0 },
        { category: 'network', count: 0 },
        { category: 'format', count: 0 },
        { category: 'permission', count: 0 },
        { category: 'storage', count: 0 },
        { category: 'blocked', count: 0 },
      ],
    },
  });
  mocks.libraryStorageStats.mockReset().mockResolvedValue({
    status: 'ok',
    data: { mediaBytes: 125_829_120, dbBytes: 40_960, sessionCount: 3 },
  });
  mocks.memoTemplatesList.mockReset().mockResolvedValue({
    status: 'ok',
    data: [
      { id: 'd1', name: 'Biên bản họp', prompt: 'Tóm tắt {transcript}', isDefault: true, locale: 'vi', defaultKey: 'meeting-minutes' },
      { id: 'd2', name: 'Memo song ngữ Nhật–Việt', prompt: 'Tóm tắt {transcript}', isDefault: true, locale: 'vi', defaultKey: 'bilingual-ja-vi' },
    ],
  });
  mocks.settingsRecommendedPreview.mockReset();
  mocks.settingsRecommendedApply.mockReset();
  mocks.settingsRecommendedCancel.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

describe('Settings group routing', () => {
  it('renders real SettingsGeneral fields for the general group', () => {
    render(Settings, { props: { routeParams: { group: 'general' } } });

    expect(screen.getByLabelText('Ngôn ngữ')).toBeTruthy();
    expect(screen.getByLabelText('Giao diện')).toBeTruthy();
  });

  it('renders real SettingsGemini fields for the gemini group', () => {
    render(Settings, { props: { routeParams: { group: 'gemini' } } });

    expect(screen.getByLabelText('API key')).toBeTruthy();
    expect(mocks.keysStore.load).toHaveBeenCalled();
  });

  it('renders real SettingsDiagnostics counters for the diagnostics group', async () => {
    render(Settings, { props: { routeParams: { group: 'diagnostics' } } });

    expect(screen.getByText('Số phiên')).toBeTruthy();
    expect(screen.getByText('Số lần thoát không sạch')).toBeTruthy();
    expect(await screen.findByText('3')).toBeTruthy();
    expect(screen.getByText('1')).toBeTruthy();
    expect(screen.getByText('Key không hợp lệ')).toBeTruthy();
    expect(screen.getByText('2')).toBeTruthy();
    expect(screen.queryByRole('checkbox')).toBeNull();
  });

  it('renders real SettingsStorage numbers for the storage group', async () => {
    render(Settings, { props: { routeParams: { group: 'storage' } } });

    expect(screen.getByText('Media')).toBeTruthy();
    expect(await screen.findByText('120 MB')).toBeTruthy();
    expect(screen.getByText('40 KB')).toBeTruthy();
    expect(screen.getByText('3')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Mở thư mục' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xoá toàn bộ dữ liệu…' })).toBeTruthy();
    expect(screen.queryByLabelText(/cache/i)).toBeNull();
  });

  it('renders real SettingsMemo master-detail for the memo group', async () => {
    render(Settings, { props: { routeParams: { group: 'memo' } } });

    expect(await screen.findByText('Biên bản họp')).toBeTruthy();
    expect(screen.getByText('Memo song ngữ Nhật–Việt')).toBeTruthy();
    expect(screen.getByRole('button', { name: '+ Thêm mẫu' })).toBeTruthy();
    expect(screen.getByLabelText('Tên mẫu')).toBeTruthy();
    expect(screen.getByLabelText('Prompt')).toBeTruthy();
  });

  it('does not fetch recommendations on mount and fetches only after a click', async () => {
    mocks.settingsRecommendedPreview.mockResolvedValue({
      status: 'error',
      error: { category: 'network', code: 'network', detailRedacted: 'https://private.example/secret' },
    });
    render(Settings, { props: { routeParams: { group: 'recommended' } } });

    expect(mocks.settingsRecommendedPreview).not.toHaveBeenCalled();
    expect(await screen.findByRole('heading', { name: 'Cấu hình đề xuất', level: 2 })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Tải cấu hình đề xuất' }));

    expect(mocks.settingsRecommendedPreview).toHaveBeenCalledTimes(1);
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Lỗi kết nối');
    expect(alert.textContent).not.toContain('private.example');
  });

  it('applies only after the displayed diff is clicked and can cancel a preview', async () => {
    mocks.settingsRecommendedPreview.mockResolvedValue({
      status: 'ok',
      data: {
        token: 'preview-token',
        changes: [{ field: 'chunkMinutes', currentValue: '5', proposedValue: '8' }],
        templateChanges: [],
        templateConflicts: [],
      },
    });
    mocks.settingsRecommendedApply.mockResolvedValue({
      status: 'ok',
      data: { stale: false, appliedCount: 1, preview: null },
    });
    render(Settings, { props: { routeParams: { group: 'recommended' } } });

    await fireEvent.click(screen.getByRole('button', { name: 'Tải cấu hình đề xuất' }));
    expect(await screen.findByText('5')).toBeTruthy();
    expect(screen.getByText('8')).toBeTruthy();
    expect(mocks.settingsRecommendedApply).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Áp dụng thay đổi' }));
    expect(mocks.settingsRecommendedApply).toHaveBeenCalledWith('preview-token');
    expect(await screen.findByText('Đã áp dụng 1 thay đổi.')).toBeTruthy();

    mocks.settingsRecommendedPreview.mockResolvedValueOnce({
      status: 'ok',
      data: {
        token: 'cancel-token',
        changes: [{ field: 'chunkMinutes', currentValue: '8', proposedValue: '9' }],
        templateChanges: [],
        templateConflicts: [],
      },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Tải cấu hình đề xuất' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ bản xem trước' }));
    expect(mocks.settingsRecommendedCancel).toHaveBeenCalledWith('cancel-token');
    expect(await screen.findByText('Đã huỷ bản xem trước. Không có thay đổi nào được lưu.')).toBeTruthy();
  });

  it('renders real SettingsAbout content for the about group', () => {
    render(Settings, { props: { routeParams: { group: 'about' } } });

    expect(screen.getByText('Relipa')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Liên hệ hỗ trợ' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Đọc Chính sách quyền riêng tư' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xem lại văn bản đồng ý' })).toBeTruthy();
  });

  it('shows only the About group when Consent has been declined', () => {
    mocks.settingsStore.consentStatus = 'declined';
    render(Settings, { props: { routeParams: { group: 'about' } } });

    const nav = screen.getByRole('navigation', { name: 'Nhóm cài đặt' });
    expect(nav.textContent).toContain('Giới thiệu & Quyền riêng tư');
    expect(nav.textContent).not.toContain('Chung');
    expect(nav.textContent).not.toContain('Gemini');
    expect(nav.textContent).not.toContain('Memo');
    expect(nav.textContent).not.toContain('Lưu trữ');
    expect(nav.textContent).not.toContain('Chẩn đoán');
  });
});
