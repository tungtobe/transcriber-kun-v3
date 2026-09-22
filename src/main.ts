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
    readonly consentAcceptedVersion?: number;
    readonly consentDeclined?: boolean;
    readonly consentPolicy?: { currentVersion: number } | null;
    bootstrap: () => void;
    load: () => Promise<void>;
  };
  configure: typeof configureRouter;
  enforce: (state: any) => Promise<unknown>;
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
  const hasConsentState = typeof dependencies.settings.consentAcceptedVersion === 'number';
  await dependencies.enforce(hasConsentState
    ? {
      onboardingCompleted: dependencies.settings.onboardingCompleted,
      consentAcceptedVersion: dependencies.settings.consentAcceptedVersion ?? 0,
      consentDeclined: dependencies.settings.consentDeclined ?? false,
      currentConsentVersion: dependencies.settings.consentPolicy?.currentVersion ?? 0,
    }
    : dependencies.settings.onboardingCompleted);
  return dependencies.mountApp(App, { target });
}

const target = document.getElementById('app');
if (!target) throw new Error('missing #app mount element');

export default await bootstrapApp(target);
