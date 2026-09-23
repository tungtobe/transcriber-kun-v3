// @vitest-environment jsdom
// Covers the Story 1.9 acceptance criteria that need `settingsStore` in a
// specific, controllable state: real content per group and the "Consent
// declined -> About only" no-regression guarantee (spec Acceptance).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/svelte';
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
    expect(nav.textContent).not.toContain('Chẩn đoán');
  });
});
