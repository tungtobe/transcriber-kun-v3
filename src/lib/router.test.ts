// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';

describe('frontend router', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/home');
  });

  it('registers the Epic 1 routes plus story 2.4 /session/:id and builds the typed settings path', async () => {
    const { routes, routePaths } = await import('./router');

    expect(Object.keys(routes)).toEqual([
      '/onboarding',
      '/home',
      '/settings/:group',
      '/session/:id',
    ]);
    expect(routePaths.settings({ group: 'gemini' })).toBe('/settings/gemini');
  });

  it('configures clean history URLs before the router is mounted', async () => {
    const { configureRouter } = await import('./router');
    configureRouter();
    expect(window.location.pathname).toBe('/home');
  });

  it('routes incomplete onboarding to onboarding and completed onboarding to home', async () => {
    const { startupRedirectPath } = await import('./router');

    expect(startupRedirectPath(false, '/home')).toBe('/onboarding');
    expect(startupRedirectPath(false, '/settings/general')).toBe('/onboarding');
    expect(startupRedirectPath(false, '/onboarding')).toBeNull();
    expect(startupRedirectPath(true, '/onboarding')).toBe('/home');
    expect(startupRedirectPath(true, '/settings/general')).toBeNull();
  });

  it('uses the real startup replacement seam for both completion states', async () => {
    const { enforceStartupRoute } = await import('./router');
    const replaced: string[] = [];
    const replaceRoute = async (path: string) => {
      replaced.push(path);
    };

    window.history.replaceState({}, '', '/home');
    await enforceStartupRoute(false, replaceRoute);
    expect(replaced).toEqual(['/onboarding']);

    window.history.replaceState({}, '', '/onboarding');
    await enforceStartupRoute(true, replaceRoute);
    expect(replaced).toEqual(['/onboarding', '/home']);
  });

  it('puts pending and stale consent before onboarding completion', async () => {
    const { startupRedirectPath } = await import('./router');
    const state = { onboardingCompleted: true, consentAcceptedVersion: 0, consentDeclined: false, currentConsentVersion: 1 };
    expect(startupRedirectPath(state, '/home')).toBe('/onboarding');
    expect(startupRedirectPath({ ...state, consentAcceptedVersion: 0 }, '/settings/gemini')).toBe('/onboarding');
    expect(startupRedirectPath({ ...state, consentAcceptedVersion: 2 }, '/home')).toBe('/onboarding');
  });

  it('restricts a declined decision to About and the consent return path', async () => {
    const { startupRedirectPath } = await import('./router');
    const state = { onboardingCompleted: false, consentAcceptedVersion: 0, consentDeclined: true, currentConsentVersion: 1 };
    expect(startupRedirectPath(state, '/home')).toBe('/settings/about');
    expect(startupRedirectPath(state, '/settings/gemini')).toBe('/settings/about');
    expect(startupRedirectPath(state, '/settings/about')).toBeNull();
    expect(startupRedirectPath(state, '/onboarding')).toBeNull();
  });
});
