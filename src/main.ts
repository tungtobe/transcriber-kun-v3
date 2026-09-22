import { mount } from 'svelte';
import App from './App.svelte';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/global.css';
import { configureRouter, enforceStartupRoute } from './lib/router';
import { settingsStore } from './lib/stores/settings.svelte';

type BootstrapDependencies = {
  settings: {
    readonly onboardingCompleted: boolean;
    bootstrap: () => void;
    load: () => Promise<void>;
  };
  configure: typeof configureRouter;
  enforce: typeof enforceStartupRoute;
  mountApp: (component: typeof App, options: { target: HTMLElement }) => unknown;
};

/**
 * Load the durable settings snapshot and enforce the startup guard before the
 * first component is mounted. This ordering prevents a first-launch Home
 * paint before an incomplete onboarding state is redirected.
 */
export async function bootstrapApp(
  target: HTMLElement,
  dependencies: BootstrapDependencies = {
    settings: settingsStore,
    configure: configureRouter,
    enforce: enforceStartupRoute,
    mountApp: mount,
  },
) {
  dependencies.settings.bootstrap();
  dependencies.configure();
  await dependencies.settings.load();
  await dependencies.enforce(dependencies.settings.onboardingCompleted);
  return dependencies.mountApp(App, { target });
}

const target = document.getElementById('app');
if (!target) throw new Error('missing #app mount element');

export default await bootstrapApp(target);
