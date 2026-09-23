<script lang="ts">
  // Settings → Giới thiệu & Quyền riêng tư: version, tác giả, liên hệ hỗ trợ,
  // Privacy Policy, và xem lại văn bản đồng ý chỉ-đọc. Phải hoạt động đầy đủ
  // cả khi Consent bị từ chối (spec Always) — không gọi
  // `acceptConsent`/`declineConsent` hay đổi `consentAcceptedVersion` từ đây
  // (spec Never), và không request mạng nào (mở link chỉ mở trình duyệt
  // ngoài qua `openUrl`, không phải một request của app).
  import { openUrl } from '@tauri-apps/plugin-opener';
  import SettingsRow from '../../components/SettingsRow.svelte';
  import ConsentText from '../../components/ConsentText.svelte';
  import { i18n } from '../../i18n/index.svelte';
  import { appStore } from '../../lib/stores/app.svelte';
  import { settingsStore } from '../../lib/stores/settings.svelte';

  let consentReviewOpen = $state(false);
  let linkError = $state(false);

  const version = $derived(appStore.version.status === 'ok' ? appStore.version.version : null);
  const privacyUrl = $derived(settingsStore.consentPolicy?.privacyUrl ?? null);
  const supportUrl = $derived(settingsStore.consentPolicy?.supportUrl ?? null);
  const consentVersion = $derived(settingsStore.consentPolicy?.currentVersion ?? 1);

  async function open(url: string | null): Promise<void> {
    if (!url) return;
    linkError = false;
    try {
      await openUrl(url);
    } catch {
      linkError = true;
    }
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.about.versionLabel')}
    help={i18n.t('settings.about.versionHelp')}
    helperText={i18n.t('settings.about.versionHelper')}
  >
    <p class="value mono">{version ?? '—'}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.about.authorLabel')}
    help={i18n.t('settings.about.authorHelp')}
    helperText={i18n.t('settings.about.authorHelper')}
  >
    <p class="value">{i18n.t('settings.about.authorValue')}</p>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.about.supportLabel')}
    help={i18n.t('settings.about.supportHelp')}
    helperText={i18n.t('settings.about.supportHelper')}
  >
    <button type="button" class="link-button" onclick={() => void open(supportUrl)}>
      {i18n.t('settings.about.supportLinkText')}
    </button>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.about.privacyLabel')}
    help={i18n.t('settings.about.privacyHelp')}
    helperText={i18n.t('settings.about.privacyHelper')}
  >
    <button type="button" class="link-button" onclick={() => void open(privacyUrl)}>
      {i18n.t('settings.about.privacyLinkText')}
    </button>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.about.consentLabel')}
    help={i18n.t('settings.about.consentHelp')}
    helperText={i18n.t('settings.about.consentHelper')}
  >
    <button type="button" aria-expanded={consentReviewOpen} onclick={() => (consentReviewOpen = !consentReviewOpen)}>
      {consentReviewOpen ? i18n.t('settings.about.consentHide') : i18n.t('settings.about.consentShow')}
    </button>
    {#if consentReviewOpen}
      <div class="consent-review" role="region" aria-label={i18n.t('settings.about.consentLabel')}>
        <ConsentText version={consentVersion} {privacyUrl} onPrivacyError={() => (linkError = true)} />
      </div>
    {/if}
  </SettingsRow>

  {#if linkError}
    <p class="link-error" role="alert">{i18n.t('error.network.hint')}</p>
  {/if}
</div>

<style>
  .settings-fields {
    display: grid;
  }

  .value {
    margin: 0;
    padding: 0 var(--space-3);
    min-height: 36px;
    display: flex;
    align-items: center;
    width: fit-content;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    font-size: var(--text-option-size);
  }

  button {
    min-height: 36px;
    width: fit-content;
    padding: 0 var(--space-4);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }

  .link-button {
    color: var(--color-accent);
  }

  .consent-review {
    margin-top: var(--space-4);
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface-sunken);
  }

  .link-error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-help-size);
  }
</style>
