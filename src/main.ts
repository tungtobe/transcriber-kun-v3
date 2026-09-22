import { mount } from 'svelte';
import App from './App.svelte';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/global.css';
import { configureRouter } from './lib/router';
import { settingsStore } from './lib/stores/settings.svelte';

const target = document.getElementById('app');
if (!target) {
  throw new Error('missing #app mount element');
}

// Apply the cached/system theme synchronously, then mount immediately so the
// shell remains visible while persisted settings load or IPC is unavailable.
settingsStore.bootstrap();
configureRouter();

const app = mount(App, { target });
void settingsStore.load();

export default app;
