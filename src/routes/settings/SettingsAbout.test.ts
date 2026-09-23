// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SettingsAbout from './SettingsAbout.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    consentPolicy: {
      currentVersion: 1,
      privacyUrl: 'https://transkun.app/privacy',
      supportUrl: 'https://transkun.app/support',
      acceptedVersion: 0,
      declined: true,
      status: 'declined' as const,
    },
  },
  appStore: {
    version: { status: 'ok' as const, version: '0.1.0' },
  },
  openUrl: vi.fn(),
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../../lib/stores/app.svelte', () => ({ appStore: mocks.appStore }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: (...args: unknown[]) => mocks.openUrl(...args) }));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
  mocks.appStore.version = { status: 'ok', version: '0.1.0' };
  mocks.settingsStore.consentPolicy = {
    currentVersion: 1,
    privacyUrl: 'https://transkun.app/privacy',
    supportUrl: 'https://transkun.app/support',
    acceptedVersion: 0,
    declined: true,
    status: 'declined',
  };
});

describe('SettingsAbout', () => {
  // Acceptance: "Given nhóm Giới thiệu ... có version, tác giả, link hỗ trợ
  // và Privacy Policy mở trình duyệt ngoài, và nút xem lại văn bản đồng ý."
  it('shows the app version, author, support link and privacy link', () => {
    render(SettingsAbout);

    expect(screen.getByText('0.1.0')).toBeTruthy();
    expect(screen.getByText('Relipa')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Liên hệ hỗ trợ' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Đọc Chính sách quyền riêng tư' })).toBeTruthy();
  });

  it('opens the support URL via openUrl when clicked', async () => {
    render(SettingsAbout);

    await fireEvent.click(screen.getByRole('button', { name: 'Liên hệ hỗ trợ' }));

    expect(mocks.openUrl).toHaveBeenCalledWith('https://transkun.app/support');
  });

  it('opens the privacy URL via openUrl when clicked', async () => {
    render(SettingsAbout);

    await fireEvent.click(screen.getByRole('button', { name: 'Đọc Chính sách quyền riêng tư' }));

    expect(mocks.openUrl).toHaveBeenCalledWith('https://transkun.app/privacy');
  });

  // Acceptance: works fully even when Consent was declined (this test's
  // default mock state already sets `declined: true`).
  it('shows the consent review toggle and full consent text even when Consent was declined', async () => {
    render(SettingsAbout);

    const toggle = screen.getByRole('button', { name: 'Xem lại văn bản đồng ý' });
    await fireEvent.click(toggle);

    expect(await screen.findByText('Văn bản đồng ý phiên bản 1')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Ẩn văn bản đồng ý' })).toBeTruthy();
  });

  it('never calls acceptConsent/declineConsent from the review toggle', async () => {
    const acceptConsent = vi.fn();
    const declineConsent = vi.fn();
    render(SettingsAbout);

    await fireEvent.click(screen.getByRole('button', { name: 'Xem lại văn bản đồng ý' }));

    expect(acceptConsent).not.toHaveBeenCalled();
    expect(declineConsent).not.toHaveBeenCalled();
    // `consentPolicy` itself must stay untouched by opening the viewer.
    expect(mocks.settingsStore.consentPolicy.acceptedVersion).toBe(0);
    expect(mocks.settingsStore.consentPolicy.declined).toBe(true);
  });
});
