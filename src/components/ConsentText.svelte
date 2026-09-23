<script lang="ts">
  // Nội dung văn bản đồng ý dùng chung — tách khỏi Onboarding (spec Code Map:
  // "About tái dùng markup/i18n consent ở Onboarding.svelte", story 1.10) để
  // Settings → Giới thiệu hiển thị lại y hệt nội dung, chỉ-đọc, mà không phải
  // chép lại i18n hay markup. Không bao giờ gọi `acceptConsent`/
  // `declineConsent` từ đây (spec Never) — component này chỉ hiển thị + mở
  // link Privacy Policy bằng `openUrl`.
  import { i18n } from '../i18n/index.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  let {
    version,
    privacyUrl,
    onPrivacyError,
  }: {
    /** "Văn bản đồng ý phiên bản N" — Rust-owned, không tự suy ra ở đây. */
    version: number;
    privacyUrl?: string | null;
    /** Gọi khi mở Privacy Policy thất bại; hiển thị lỗi là việc của caller. */
    onPrivacyError?: () => void;
  } = $props();

  async function openPrivacyPolicy(): Promise<void> {
    if (!privacyUrl) return;
    try {
      await openUrl(privacyUrl);
    } catch {
      onPrivacyError?.();
    }
  }
</script>

<div class="flow" aria-label={i18n.t('onboarding.consent.title')}>
  <div class="flow-node">{i18n.t('onboarding.consent.flowLocal')}</div>
  <div class="flow-arrow" aria-hidden="true">→</div>
  <div class="flow-node">{i18n.t('onboarding.consent.flowKey')}</div>
  <div class="flow-arrow" aria-hidden="true">→</div>
  <div class="flow-node">{i18n.t('onboarding.consent.flowGoogle')}</div>
</div>
<ul class="consent-points">
  <li>{i18n.t('onboarding.consent.bulletLocal')}</li>
  <li>{i18n.t('onboarding.consent.bulletKey')}</li>
  <li>{i18n.t('onboarding.consent.bulletGoogle')}</li>
</ul>
<button class="privacy-link" type="button" onclick={() => void openPrivacyPolicy()}>
  {i18n.t('onboarding.consent.privacy')}
</button>
<p class="consent-version">{i18n.t('onboarding.consent.version', { version })}</p>

<style>
  .flow {
    display: grid;
    grid-template-columns: 1fr auto 1fr auto 1fr;
    gap: var(--space-2);
    align-items: center;
    margin: var(--space-6) 0;
  }

  .flow-node {
    display: grid;
    min-height: 56px;
    place-items: center;
    padding: var(--space-3);
    border: 1px solid var(--color-accent-border);
    border-radius: var(--radius-lg);
    background: var(--color-accent-soft);
    text-align: center;
    font-size: var(--text-help-size);
  }

  .flow-arrow {
    color: var(--color-text-muted);
    font-size: 20px;
  }

  .consent-points {
    display: grid;
    gap: var(--space-3);
    padding-left: var(--space-5);
    color: var(--color-text-secondary);
  }

  .privacy-link {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-accent);
    text-decoration: underline;
    cursor: pointer;
  }

  .consent-version {
    margin: var(--space-4) 0 0;
    color: var(--color-text-muted);
    font-size: var(--text-help-size);
  }
</style>
