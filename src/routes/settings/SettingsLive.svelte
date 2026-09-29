<script lang="ts">
  // Settings → Live: default translation Target for new Live sessions. The
  // Live screen can still change it mid-session; this only seeds the setup.
  import SettingsRow from '../../components/SettingsRow.svelte';
  import { i18n } from '../../i18n/index.svelte';
  import { settingsStore } from '../../lib/stores/settings.svelte';
  import type { LiveTarget } from '../../lib/bindings';

  function changeTarget(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value as LiveTarget;
    void settingsStore.setLiveTarget(value);
  }
</script>

<div class="settings-fields">
  <SettingsRow
    label={i18n.t('settings.live.targetLabel')}
    help={i18n.t('settings.live.targetHelp')}
    helperText={i18n.t('settings.live.targetHelper')}
    fieldId="settings-live-target"
  >
    <select id="settings-live-target" value={settingsStore.liveTarget} onchange={changeTarget}>
      <option value="none">{i18n.t('live.target.none')}</option>
      <option value="ja">{i18n.t('live.target.ja')}</option>
      <option value="vi">{i18n.t('live.target.vi')}</option>
      <option value="en">{i18n.t('live.target.en')}</option>
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
