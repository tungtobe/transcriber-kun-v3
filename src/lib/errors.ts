// Single place mapping the 8 stable `Category` values to user-facing i18n
// copy: a short title and a one-sentence, actionable hint. No key/URL/stack
// trace ever passes through here — only `error.<category>.title|hint`
// strings looked up by category (spec Boundaries).
import { i18n, type TranslationKey } from '../i18n/index.svelte';
import type { BannerItem } from '../components/BannerStack.svelte';
import type { AppError, Category } from './bindings';

const CATEGORY_COPY: Record<Category, { title: TranslationKey; hint: TranslationKey }> = {
  quota: { title: 'error.quota.title', hint: 'error.quota.hint' },
  auth: { title: 'error.auth.title', hint: 'error.auth.hint' },
  model: { title: 'error.model.title', hint: 'error.model.hint' },
  network: { title: 'error.network.title', hint: 'error.network.hint' },
  format: { title: 'error.format.title', hint: 'error.format.hint' },
  permission: { title: 'error.permission.title', hint: 'error.permission.hint' },
  storage: { title: 'error.storage.title', hint: 'error.storage.hint' },
  blocked: { title: 'error.blocked.title', hint: 'error.blocked.hint' },
};

/** Short category heading, e.g. "Key không hợp lệ" — never the raw code/detail. */
export function errorTitle(error: AppError): string {
  return i18n.t(CATEGORY_COPY[error.category].title);
}

/** One actionable sentence — never the raw `detailRedacted`, a key, or a URL. */
export function errorHint(error: AppError): string {
  return i18n.t(CATEGORY_COPY[error.category].hint);
}

/**
 * Reusable banner for any error that involves the model settings area (a
 * `models_list`/`model`-category failure, in this story specifically the
 * Settings → Gemini "Tải danh sách" flow). Always a warning with a shortcut
 * to Settings → Gemini and never changes the user's configured model itself
 * (spec Always: "Helper `modelErrorBanner(error)` tạo `BannerItem` warning
 * với lối tắt `/settings/gemini`, không bao giờ tự đổi model").
 *
 * `scope` disambiguates the `BannerItem.id` when more than one such banner
 * can be visible at once (e.g. the three "Tải danh sách" rows).
 */
export function modelErrorBanner(error: AppError, scope = 'model'): BannerItem {
  return {
    id: `model-error-${scope}`,
    variant: 'warning',
    title: errorTitle(error),
    message: errorHint(error),
    actionLabel: i18n.t('settings.help.modelBannerAction'),
    actionHref: '/settings/gemini',
  };
}
