<script lang="ts">
  import { onMount } from 'svelte';
  import { appStore } from './lib/stores/app.svelte';

  onMount(() => {
    appStore.loadVersion();
  });
</script>

<main>
  <h1>trans-kun</h1>
  <p>
    Version:
    {#if appStore.version.status === 'ok'}
      {appStore.version.version}
    {:else if appStore.version.status === 'loading'}
      …
    {:else}
      —
    {/if}
  </p>
</main>
