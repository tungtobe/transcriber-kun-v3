// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import { keysStore } from '../lib/stores/keys.svelte';
import Onboarding from './Onboarding.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    uiLanguage: 'system' as 'system' | 'vi' | 'en' | 'ja',
    setUiLanguage: vi.fn(),
    consentStatus: undefined as 'pending' | 'declined' | 'stale' | 'current' | undefined,
    setOnboardingCompleted: vi.fn(),
  },
  keysList: vi.fn(),
  keysSet: vi.fn(),
  keysTest: vi.fn(),
  modelsList: vi.fn(),
}));

vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      keysList: (...args: unknown[]) => mocks.keysList(...args),
      keysSet: (...args: unknown[]) => mocks.keysSet(...args),
      keysTest: (...args: unknown[]) => mocks.keysTest(...args),
      modelsList: (...args: unknown[]) => mocks.modelsList(...args),
    },
  };
});

afterEach(() => cleanup());

beforeEach(() => {
  Object.defineProperty(navigator, 'language', { configurable: true, value: 'en-US' });
  mocks.settingsStore.uiLanguage = 'system';
  mocks.settingsStore.consentStatus = undefined;
  mocks.settingsStore.setUiLanguage.mockReset().mockImplementation(async (next) => {
    mocks.settingsStore.uiLanguage = next;
    i18n.applyPreference(next);
  });
  mocks.settingsStore.setOnboardingCompleted.mockReset().mockResolvedValue(undefined);
  mocks.keysList.mockReset().mockResolvedValue({ status: 'ok', data: [] });
  mocks.keysSet.mockReset();
  mocks.keysTest.mockReset();
  mocks.modelsList.mockReset();
  keysStore.reset();
  configureRouter();
  window.history.replaceState({}, '', '/onboarding');
  i18n.applyPreference('system');
});

describe('Onboarding language step', () => {
  it('renders a three-step, three-language accessible selector with system badge', () => {
    const { container } = render(Onboarding);

    expect(container.querySelector('ol.stepper')).toBeTruthy();
    expect(container.querySelectorAll('ol.stepper > li')).toHaveLength(3);
    expect(container.querySelectorAll('[aria-current="step"]')).toHaveLength(1);
    expect(screen.getByLabelText('Setup progress').textContent).toContain('Language');
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: /English/ })).toHaveProperty('checked', true);
    expect(screen.getByText('System language')).toBeTruthy();
    expect(screen.queryByText(/environment/i)).toBeNull();
  });

  it('uses Japanese on first launch when the system locale is ja-JP', () => {
    Object.defineProperty(navigator, 'language', { configurable: true, value: 'ja-JP' });
    mocks.settingsStore.uiLanguage = 'system';
    i18n.applyPreference('system');

    render(Onboarding);

    expect(screen.getByRole('radio', { name: /日本語/ })).toHaveProperty('checked', true);
    expect(screen.getByRole('heading', { name: 'trans-kun へようこそ' })).toBeTruthy();
    expect(screen.getByText('システム言語')).toBeTruthy();
  });

  it('switches the entire step to Japanese immediately and persists through the domain store', async () => {
    render(Onboarding);

    await fireEvent.click(screen.getByRole('radio', { name: /日本語/ }));

    expect(mocks.settingsStore.setUiLanguage).toHaveBeenCalledWith('ja');
    expect(await screen.findByRole('heading', { name: 'trans-kun へようこそ' })).toBeTruthy();
    expect(screen.getByLabelText('セットアップの進行状況').textContent).toContain('データ');
    expect(document.documentElement.lang).toBe('ja');
  });
});

describe('Onboarding API key step', () => {
  beforeEach(() => {
    i18n.applyPreference('vi');
    mocks.settingsStore.consentStatus = 'current';
  });

  it('renders the key form with Continue disabled until a usable key exists', async () => {
    render(Onboarding);

    expect(screen.getByLabelText('API key')).toBeTruthy();
    await waitFor(() => expect(mocks.keysList).toHaveBeenCalled());
    const continueButton = screen.getByRole('button', { name: 'Tiếp tục' });
    expect(continueButton.getAttribute('aria-disabled')).toBe('true');
    expect(continueButton).toHaveProperty('disabled', false);
    expect(screen.getByRole('tooltip').textContent).toContain('Cần ít nhất một key hợp lệ');
    await fireEvent.click(continueButton);
    expect(mocks.settingsStore.setOnboardingCompleted).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Bỏ qua, nhập sau' })).toHaveProperty('disabled', false);
  });

  it('blocks an empty submission locally and never calls keysSet', async () => {
    render(Onboarding);

    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    expect(await screen.findByText('Định dạng không đúng')).toBeTruthy();
    expect(mocks.keysSet).not.toHaveBeenCalled();
  });

  it('reports a valid key, the model count, and enables Continue', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [{ id: 'k1', label: 'AIza••••abcd' }] });
    mocks.keysTest.mockResolvedValue({ status: 'ok', data: { keyId: 'k1', valid: true } });
    mocks.modelsList.mockResolvedValue({
      status: 'ok',
      data: [{ name: 'models/gemini-1', displayName: 'Gemini 1', supportedGenerationMethods: [] }],
    });
    render(Onboarding);

    await fireEvent.input(screen.getByLabelText('API key'), { target: { value: 'AIzaValidKey' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    expect(await screen.findByText('Key hợp lệ, 1 model khả dụng')).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Tiếp tục' }).getAttribute('aria-disabled')).toBeNull(),
    );
  });

  it('shows a category title and hint (no key/URL) when every key is rejected, and stays on this step', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [{ id: 'k1', label: 'AIza••••abcd' }] });
    mocks.keysTest.mockResolvedValue({
      status: 'error',
      error: { category: 'auth', code: 'auth', detailRedacted: 'rejected' },
    });
    render(Onboarding);

    await fireEvent.input(screen.getByLabelText('API key'), { target: { value: 'AIzaBadKey' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    expect(await screen.findByText('Key không hợp lệ')).toBeTruthy();
    expect(screen.getByText(/Kiểm tra lại key/)).toBeTruthy();
    expect(screen.queryByText('AIzaBadKey')).toBeNull();
    expect(screen.getByRole('button', { name: 'Tiếp tục' }).getAttribute('aria-disabled')).toBe('true');
  });

  it('reports a partial rejection next to the valid count without blocking Continue', async () => {
    mocks.keysSet.mockResolvedValue({
      status: 'ok',
      data: [{ id: 'k1', label: 'AIza••••abcd' }, { id: 'k2', label: 'AIza••••efgh' }],
    });
    mocks.keysTest.mockImplementation(async (id: string) =>
      id === 'k1'
        ? { status: 'ok', data: { keyId: 'k1', valid: true } }
        : { status: 'error', error: { category: 'auth', code: 'auth', detailRedacted: 'rejected' } },
    );
    mocks.modelsList.mockResolvedValue({ status: 'ok', data: [] });
    render(Onboarding);

    await fireEvent.input(screen.getByLabelText('API key'), { target: { value: 'AIzaOne,AIzaTwo' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    expect(await screen.findByText(/Key hợp lệ, 0 model khả dụng/)).toBeTruthy();
    expect(screen.getByText(/1 key bị từ chối/)).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Tiếp tục' }).getAttribute('aria-disabled')).toBeNull(),
    );
  });

  it('completes onboarding and heads to /home on Skip without requiring a key', async () => {
    render(Onboarding);

    await fireEvent.click(screen.getByRole('button', { name: 'Bỏ qua, nhập sau' }));

    await waitFor(() => expect(mocks.settingsStore.setOnboardingCompleted).toHaveBeenCalledWith(true));
    await waitFor(() => expect(window.location.pathname).toBe('/home'));
  });

  it('completes onboarding on Continue once a key is confirmed usable', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [{ id: 'k1', label: 'AIza••••abcd' }] });
    render(Onboarding);

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Tiếp tục' }).getAttribute('aria-disabled')).toBeNull(),
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục' }));

    await waitFor(() => expect(mocks.settingsStore.setOnboardingCompleted).toHaveBeenCalledWith(true));
    await waitFor(() => expect(window.location.pathname).toBe('/home'));
  });
});

describe('Onboarding with declined Consent', () => {
  it('never reaches the key step and makes no key/model request', async () => {
    mocks.settingsStore.consentStatus = 'declined';
    render(Onboarding);

    await Promise.resolve();
    expect(screen.queryByLabelText('API key')).toBeNull();
    expect(mocks.keysList).not.toHaveBeenCalled();
    expect(mocks.keysSet).not.toHaveBeenCalled();
    expect(mocks.keysTest).not.toHaveBeenCalled();
    expect(mocks.modelsList).not.toHaveBeenCalled();
  });
});
