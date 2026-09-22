<script lang="ts">
  // Settings → Chung: language and theme. Both reuse the existing
  // `settingsStore` optimistic setters (spec Always: "Đổi ngôn ngữ/theme áp
  // dụng tức thì và lưu bền qua setter optimistic của `settingsStore`") — a
  // save failure surfaces through AppShell's existing `settingsStore.error`
  // banner (rollback + banner storage), so this screen needs no error UI of
  // its own.
  import SettingsRow from '../../components/SettingsRow.svelte';
  import { i18n } from '../../i18n/index.svelte';
  import { settingsStore } from '../../lib/stores/settings.svelte';
  import type { Theme, UiLanguage } from '../../lib/bindings';

  function changeLanguage(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value as UiLanguage;
    void settingsStore.setUiLanguage(value);
  }

  function changeTheme(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value as Theme;
    void settingsStore.setTheme(value);
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.general.languageLabel')}
    help={i18n.t('settings.general.languageHelp')}
    helperText={i18n.t('settings.general.languageHelper')}
    fieldId="settings-general-language"
  >
    <select
      id="settings-general-language"
      value={settingsStore.uiLanguage}
      onchange={changeLanguage}
    >
      <option value="system">{i18n.t('settings.general.languageSystem')}</option>
      <option value="vi">{i18n.t('settings.general.languageVi')}</option>
      <option value="en">{i18n.t('settings.general.languageEn')}</option>
      <option value="ja">{i18n.t('settings.general.languageJa')}</option>
    </select>
  </SettingsRow>

  <SettingsRow
    label={i18n.t('settings.general.themeLabel')}
    help={i18n.t('settings.general.themeHelp')}
    helperText={i18n.t('settings.general.themeHelper')}
    fieldId="settings-general-theme"
  >
    <select id="settings-general-theme" value={settingsStore.theme} onchange={changeTheme}>
      <option value="system">{i18n.t('settings.general.themeSystem')}</option>
      <option value="light">{i18n.t('settings.general.themeLight')}</option>
      <option value="dark">{i18n.t('settings.general.themeDark')}</option>
    </select>
  </SettingsRow>
</div>

<style>
  .settings-fields {
    display: grid;
  }

  .settings-fields select {
    min-height: 36px;
    max-width: 280px;
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
  }
</style>
