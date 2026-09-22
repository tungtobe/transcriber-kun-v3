// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  mount: vi.fn(() => ({})),
  configure: vi.fn(),
  enforce: vi.fn().mockResolvedValue(undefined),
  settings: {
    onboardingCompleted: false,
    bootstrap: vi.fn(),
    load: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('svelte', () => ({ mount: mocks.mount }));
vi.mock('./App.svelte', () => ({ default: {} }));
vi.mock('./lib/router', () => ({
  configureRouter: mocks.configure,
  enforceStartupRoute: mocks.enforce,
}));
vi.mock('./lib/stores/settings.svelte', () => ({ settingsStore: mocks.settings }));

describe('entrypoint bootstrap ordering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.innerHTML = '<div id="app"></div>';
    mocks.settings.onboardingCompleted = false;
    mocks.settings.load.mockResolvedValue(undefined);
    mocks.enforce.mockResolvedValue(undefined);
  });

  it.each([
    [false, '/onboarding'],
    [true, '/home'],
  ] as const)('loads settings and enforces the %s startup destination before mounting', async (completed, expected) => {
    const { bootstrapApp } = await import('./main');
    const order: string[] = [];
    const settings = {
      onboardingCompleted: completed,
      bootstrap: vi.fn(() => { order.push('bootstrap'); }),
      load: vi.fn(async () => { order.push('load'); }),
    };
    const configure = vi.fn(() => order.push('configure'));
    const destinations: string[] = [];
    const enforce = vi.fn(async (value: boolean) => {
      order.push('enforce');
      const destination = value ? '/home' : '/onboarding';
      destinations.push(destination);
      expect(value).toBe(completed);
    });
    const mountApp = vi.fn(() => {
      order.push('mount');
      return {};
    });

    await bootstrapApp(document.getElementById('app') ?? document.body, {
      settings,
      configure,
      enforce,
      mountApp,
    });

    expect(order).toEqual(['bootstrap', 'configure', 'load', 'enforce', 'mount']);
    expect(enforce).toHaveBeenCalledWith(completed);
    expect(destinations).toEqual([expected]);
  });
});
