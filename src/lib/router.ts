import {
  defineRoutes,
  replace,
  setBasePath,
  setHashRoutingEnabled,
} from '@keenmate/svelte-spa-router';
import Home from '../routes/Home.svelte';
import Onboarding from '../routes/Onboarding.svelte';
import Session from '../routes/Session.svelte';
import Settings from '../routes/Settings.svelte';

/** Route definitions cover the screens delivered by Epic 1 plus, as of story
 * 2.4, `/session/:id` — a Job's live progress or a saved Phiên's summary
 * (spec I/O Matrix "`/session/:id`"). */
const routeDefinition = defineRoutes({
  onboarding: { path: '/onboarding', component: Onboarding },
  home: { path: '/home', component: Home },
  settings: { path: '/settings/:group', component: Settings },
  session: { path: '/session/:id', component: Session },
});

export const routes = routeDefinition.routes;
export const navigation = routeDefinition.nav;
export const routePaths = routeDefinition.paths;

/**
 * Tauri serves one WebView document, so clean history URLs are preferable to
 * hash fragments. This must run before the Router component is mounted.
 */
export function configureRouter(): void {
  setHashRoutingEnabled(false);
  setBasePath('/');
}
/** Unknown deep links are safe: replace them with the stable home route. */
export function redirectUnknownRoute(): void {
  void replace('/home');
}

export type StartupRouteState = {
  onboardingCompleted: boolean;
  consentAcceptedVersion: number;
  consentDeclined: boolean;
  currentConsentVersion: number;
};

function normalizeStartupState(
  state: StartupRouteState | boolean,
): StartupRouteState {
  if (typeof state === 'boolean') {
    return {
      onboardingCompleted: state,
      consentAcceptedVersion: state ? 1 : 0,
      consentDeclined: false,
      currentConsentVersion: 1,
    };
  }
  return state;
}

export function startupRedirectPath(
  state: StartupRouteState | boolean,
  currentPath = typeof window === 'undefined' ? '/home' : window.location.pathname,
): '/onboarding' | '/home' | '/settings/about' | null {
  const { onboardingCompleted, consentAcceptedVersion, consentDeclined, currentConsentVersion } = normalizeStartupState(state);
  const consentCurrent = !consentDeclined
    && currentConsentVersion > 0
    && consentAcceptedVersion === currentConsentVersion;
  if (consentDeclined) {
    if (currentPath !== '/onboarding' && currentPath !== '/settings/about') return '/settings/about';
    return null;
  }
  if (!consentCurrent && currentPath !== '/onboarding') return '/onboarding';
  if (consentCurrent && onboardingCompleted && currentPath === '/onboarding') return '/home';
  return null;
}

export async function enforceStartupRoute(
  state: StartupRouteState | boolean,
  replaceRoute: (path: '/onboarding' | '/home' | '/settings/about') => Promise<unknown> = replace,
): Promise<void> {
  const target = startupRedirectPath(state);
  if (target) await replaceRoute(target);
}
