import {
  defineRoutes,
  replace,
  setBasePath,
  setHashRoutingEnabled,
} from '@keenmate/svelte-spa-router';
import Home from '../routes/Home.svelte';
import Onboarding from '../routes/Onboarding.svelte';
import Settings from '../routes/Settings.svelte';

/** Route definitions intentionally cover only screens delivered by Epic 1. */
const routeDefinition = defineRoutes({
  onboarding: { path: '/onboarding', component: Onboarding },
  home: { path: '/home', component: Home },
  settings: { path: '/settings/:group', component: Settings },
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

/**
 * Startup access owned by Story 1.4. Story 1.5 inserts versioned Consent
 * precedence before this completion decision; no Consent state is invented here.
 */
export function startupRedirectPath(
  onboardingCompleted: boolean,
  currentPath = typeof window === 'undefined' ? '/home' : window.location.pathname,
): '/onboarding' | '/home' | null {
  if (!onboardingCompleted && currentPath !== '/onboarding') return '/onboarding';
  if (onboardingCompleted && currentPath === '/onboarding') return '/home';
  return null;
}

export async function enforceStartupRoute(
  onboardingCompleted: boolean,
  replaceRoute: (path: '/onboarding' | '/home') => Promise<unknown> = replace,
): Promise<void> {
  const target = startupRedirectPath(onboardingCompleted);
  if (target) await replaceRoute(target);
}
