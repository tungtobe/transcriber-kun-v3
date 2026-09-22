// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { Router, push } from '@keenmate/svelte-spa-router';
import { configureRouter, redirectUnknownRoute, routes } from './router';
import { i18n } from '../i18n/index.svelte';

afterEach(() => cleanup());

describe('history router runtime', () => {
  beforeEach(() => {
    i18n.applyPreference('vi');
    configureRouter();
    window.scrollTo = vi.fn();
    window.history.replaceState({}, '', '/home');
  });

  it('renders a valid deep link, preserves route params, and handles Back/Forward', async () => {
    window.history.replaceState({}, '', '/settings/gemini');
    render(Router, { props: { routes } });

    expect(await screen.findByRole('heading', { name: 'Cài đặt' })).toBeTruthy();
    expect(screen.getByText('/settings/gemini')).toBeTruthy();

    await push('/home');
    expect(await screen.findByRole('heading', { name: 'Trang chủ' })).toBeTruthy();
    window.history.back();
    await waitFor(() => expect(window.location.pathname).toBe('/settings/gemini'));
    expect(screen.getByText('/settings/gemini')).toBeTruthy();
    window.history.forward();
    await waitFor(() => expect(window.location.pathname).toBe('/home'));
    expect(screen.getByText('Kéo file vào đây')).toBeTruthy();
  });

  it('redirects an unknown route to /home instead of leaving a blank screen', async () => {
    window.history.replaceState({}, '', '/not-a-screen');
    window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
    render(Router, { props: { routes, onNotFound: redirectUnknownRoute } });

    expect(await screen.findByRole('heading', { name: 'Trang chủ' })).toBeTruthy();
    await waitFor(() => expect(window.location.pathname).toBe('/home'));
  });
});
