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
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.consentStatus = 'current';
  mocks.settingsStore.uiLanguage = 'system';
  mocks.settingsStore.theme = 'system';
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

  it('keeps the diagnostics placeholder unchanged (no regression)', () => {
    render(Settings, { props: { routeParams: { group: 'diagnostics' } } });

    expect(screen.getByText('/settings/diagnostics')).toBeTruthy();
  });

  it('keeps the about placeholder unchanged (no regression)', () => {
    render(Settings, { props: { routeParams: { group: 'about' } } });

    expect(screen.getByText('/settings/about')).toBeTruthy();
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
