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
