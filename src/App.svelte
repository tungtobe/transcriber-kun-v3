<script lang="ts">
  import { onMount } from 'svelte';
  import { Router } from '@keenmate/svelte-spa-router';
  import AppShell from './components/AppShell.svelte';
  import { installKeymap } from './lib/keymap';
  import { redirectUnknownRoute, routes } from './lib/router';
  import { appStore } from './lib/stores/app.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';

  onMount(() => {
    appStore.loadVersion();
    const removeKeymap = installKeymap();
    return () => {
      removeKeymap();
      settingsStore.destroy();
    };
  });
</script>

<AppShell>
  <Router {routes} restoreScrollState onNotFound={redirectUnknownRoute} />
</AppShell>
