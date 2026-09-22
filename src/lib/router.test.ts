// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest';

describe('frontend router', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/home');
  });

  it('registers only the Epic 1 routes and builds the typed settings path', async () => {
    const { routes, routePaths } = await import('./router');

    expect(Object.keys(routes)).toEqual(['/onboarding', '/home', '/settings/:group']);
    expect(routePaths.settings({ group: 'gemini' })).toBe('/settings/gemini');
  });

  it('configures clean history URLs before the router is mounted', async () => {
    const { configureRouter } = await import('./router');
    configureRouter();
    expect(window.location.pathname).toBe('/home');
  });
});
