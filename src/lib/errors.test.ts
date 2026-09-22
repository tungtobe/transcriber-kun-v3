// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';
import { i18n } from '../i18n/index.svelte';
import { errorHint, errorTitle, modelErrorBanner } from './errors';
import type { AppError } from './bindings';

describe('modelErrorBanner', () => {
  beforeEach(() => i18n.applyPreference('vi'));

  it('is a warning with a shortcut to Settings → Gemini and never mentions the model itself', () => {
    const error: AppError = { category: 'model', code: 'model', detailRedacted: 'model rejected' };

    const banner = modelErrorBanner(error);

    expect(banner.variant).toBe('warning');
    expect(banner.title).toBe(errorTitle(error));
    expect(banner.message).toBe(errorHint(error));
    expect(banner.actionHref).toBe('/settings/gemini');
    expect(banner.actionLabel).toBeTruthy();
  });

  it('renders the right category copy for a non-model error too (e.g. a modelsList network failure)', () => {
    const error: AppError = { category: 'network', code: 'network', detailRedacted: 'offline' };

    const banner = modelErrorBanner(error);

    expect(banner.variant).toBe('warning');
    expect(banner.title).toBe(i18n.t('error.network.title'));
    expect(banner.message).toBe(i18n.t('error.network.hint'));
  });

  it('scopes the banner id so several can coexist without colliding in a keyed #each', () => {
    const error: AppError = { category: 'auth', code: 'auth', detailRedacted: 'rejected' };

    const transcribe = modelErrorBanner(error, 'transcribe');
    const live = modelErrorBanner(error, 'live');

    expect(transcribe.id).not.toBe(live.id);
  });

  it('defaults to a stable id when no scope is given', () => {
    const error: AppError = { category: 'auth', code: 'auth', detailRedacted: 'rejected' };

    expect(modelErrorBanner(error).id).toBe(modelErrorBanner(error).id);
  });
});
