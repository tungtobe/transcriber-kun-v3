// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { installKeymap, keymap } from '../lib/keymap';
import { liveStore } from '../lib/stores/live.svelte';
import { configureRouter } from '../lib/router';
import type { LiveEvent, LiveSnapshot } from '../lib/bindings';
import Live from './Live.svelte';

const mocks = vi.hoisted(() => ({
  liveSubscribe: vi.fn(),
  liveSources: vi.fn(),
  liveStart: vi.fn(),
  liveStop: vi.fn(),
  liveSetSource: vi.fn(),
  liveOpenPermissionSettings: vi.fn(),
  keysLoad: vi.fn(),
  tagsLoad: vi.fn(),
}));

type FakeChannel<T> = { onmessage: (event: T) => void };

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    id = 1;
    onmessage: (event: unknown) => void = () => {};
  },
}));
vi.mock('../lib/bindings', () => ({
  commands: {
    liveSubscribe: (...args: unknown[]) => mocks.liveSubscribe(...args),
    liveSources: (...args: unknown[]) => mocks.liveSources(...args),
    liveStart: (...args: unknown[]) => mocks.liveStart(...args),
    liveStop: (...args: unknown[]) => mocks.liveStop(...args),
    liveSetSource: (...args: unknown[]) => mocks.liveSetSource(...args),
    liveOpenPermissionSettings: (...args: unknown[]) => mocks.liveOpenPermissionSettings(...args),
  },
}));
vi.mock('../lib/stores/keys.svelte', () => ({
  keysStore: { status: 'ready', hasUsableKey: true, load: mocks.keysLoad },
}));
vi.mock('../lib/stores/library.svelte', () => ({
  libraryStore: {
    tags: [],
    loadTags: mocks.tagsLoad,
    createTag: vi.fn(),
    deleteTagGlobally: vi.fn(),
  },
}));
vi.mock('../lib/stores/settings.svelte', () => ({
  settingsStore: { transcribeLanguage: 'auto' },
}));
vi.mock('../lib/stores/notes.svelte', () => ({
  MAX_NOTE_BODY_LENGTH: 10_000,
  notesStore: {
    load: vi.fn(),
    flush: vi.fn(() => Promise.resolve(true)),
    view: () => ({ body: '', status: 'idle', savedAtMs: null, loadError: false }),
    setBody: vi.fn(),
    retry: vi.fn(),
  },
}));

let channels: FakeChannel<LiveEvent>[] = [];
let backendSessionId: string | null = null;
let removeKeymap: (() => void) | null = null;

function snapshot(sessionId: string | null = backendSessionId, durationSec = 0): LiveSnapshot {
  return {
    sessionId,
    transcriptId: sessionId ? 'transcript-1' : null,
    recording: sessionId ? 'active' : 'stopped',
    connection: { type: sessionId ? 'connected' : 'stopped' },
    durationSec,
  };
}

afterEach(() => {
  cleanup();
  removeKeymap?.();
  removeKeymap = null;
  vi.useRealTimers();
});

beforeEach(() => {
  configureRouter();
  window.history.replaceState({}, '', '/live');
  window.dispatchEvent(new PopStateEvent('popstate', { state: {} }));
  i18n.applyPreference('en');
  liveStore.reset();
  channels = [];
  backendSessionId = null;
  mocks.liveSubscribe.mockReset().mockImplementation((channel: FakeChannel<LiveEvent>) => {
    channels.push(channel);
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    return Promise.resolve({ status: 'ok', data: null });
  });
  mocks.liveSources.mockReset().mockResolvedValue({ status: 'ok', data: {
    microphones: [{ source: 'mic:device-a', name: 'Built-in mic', isDefault: true }],
    defaultMicrophone: 'mic:device-a',
    systemAvailable: true,
    microphonePermission: 'notDetermined',
    systemPermission: 'unknown',
  } });
  mocks.liveStart.mockReset().mockImplementation(() => {
    backendSessionId = 'session-1';
    return Promise.resolve({ status: 'ok', data: backendSessionId });
  });
  mocks.liveStop.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetSource.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveOpenPermissionSettings.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.keysLoad.mockReset().mockResolvedValue(undefined);
  mocks.tagsLoad.mockReset().mockResolvedValue(undefined);
});

describe('Live setup and status UI', () => {
  it('renders three radio cards, selects mixed by default, and marks it recommended', async () => {
    const view = render(Live);
    await waitFor(() => expect(mocks.liveSources).toHaveBeenCalledWith(true));

    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(3);
    expect((screen.getByRole('radio', { name: /System \+ microphone/ }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText('Recommended')).toBeTruthy();
    expect(screen.getByText(/does not request Screen Recording/)).toBeTruthy();
    expect(view.container.querySelector('.setup-card')).toBeTruthy();
    expect(view.container.querySelector('[data-ad-slot-hidden="true"]')).toBeTruthy();

    const systemAudio = screen.getByRole('radio', { name: /System audio/ });
    await fireEvent.click(systemAudio);
    expect((systemAudio as HTMLInputElement).checked).toBe(true);
    expect(screen.queryByRole('combobox', { name: 'Microphone' })).toBeNull();
  });

  it('shows a red recording pill with a ticking timer and a separate connection icon and label', async () => {
    const view = render(Live);
    await waitFor(() => expect((screen.getByRole('button', { name: 'Start recording' }) as HTMLButtonElement).disabled).toBe(false));
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-28T12:00:00Z'));
    await fireEvent.click(screen.getByRole('button', { name: 'Start recording' }));

    const recordingPill = await screen.findByText('Recording');
    expect(recordingPill.closest('.status-pill')?.className).toContain('status-recording');
    expect(screen.getByText('00:00')).toBeTruthy();
    expect(screen.getByText('Connecting')).toBeTruthy();
    expect(view.container.querySelector('.connection-pill svg')).toBeTruthy();

    await vi.advanceTimersByTimeAsync(1_000);
    expect(screen.getByText('00:01')).toBeTruthy();
  });

  it('offers an explicit permission recheck in the denied-source banner', async () => {
    mocks.liveStart.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'permission', code: 'permission', detailRedacted: 'capture denied' },
    });
    await liveStore.start('mixed:device-a', 'auto', 'en', []);
    render(Live);
    await waitFor(() => expect(screen.getByText('System settings have denied access to this audio source.')).toBeTruthy());

    const check = screen.getByRole('button', { name: 'Check permission again' });
    await fireEvent.click(check);
    await waitFor(() => expect(mocks.liveSources).toHaveBeenCalledWith(true));
    await waitFor(() => expect(screen.queryByText('System settings have denied access to this audio source.')).toBeNull());
    expect(liveStore.deniedSource).toBeNull();
  });
});

describe('Live remount and shortcut', () => {
  it('remounts the same session snapshot without starting another session', async () => {
    const first = render(Live);
    await waitFor(() => expect((screen.getByRole('button', { name: 'Start recording' }) as HTMLButtonElement).disabled).toBe(false));
    await fireEvent.click(screen.getByRole('button', { name: 'Start recording' }));
    await screen.findByText('Recording');
    expect(mocks.liveStart).toHaveBeenCalledTimes(1);
    first.unmount();

    const second = render(Live);
    expect(await screen.findByText('Recording')).toBeTruthy();
    expect(mocks.liveStart).toHaveBeenCalledTimes(1);
    expect(mocks.liveSubscribe).toHaveBeenCalledTimes(2);
    expect(liveStore.snapshot.sessionId).toBe('session-1');
    second.unmount();
  });

  it('starts or stops once per shortcut press and ignores repeat events and dialogs', async () => {
    render(Live);
    removeKeymap = installKeymap(document);
    await waitFor(() => expect(mocks.liveSources).toHaveBeenCalledWith(true));
    await waitFor(() => expect((screen.getByRole('button', { name: 'Start recording' }) as HTMLButtonElement).disabled).toBe(false));
    await waitFor(() => expect(keymap.some((entry) => entry.id === 'live-toggle-meta')).toBe(true));
    expect(keymap.find((entry) => entry.id === 'live-toggle-meta')?.combo).toBe('Meta+Shift+L');

    const metaEvent = new KeyboardEvent('keydown', { key: 'l', metaKey: true, shiftKey: true, bubbles: true, cancelable: true });
    document.dispatchEvent(metaEvent);
    expect(metaEvent.defaultPrevented).toBe(true);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', metaKey: true, shiftKey: true, repeat: true, bubbles: true }));
    await waitFor(() => expect(mocks.liveStart).toHaveBeenCalledTimes(1));
    await screen.findByRole('button', { name: 'Stop recording' });

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', ctrlKey: true, shiftKey: true, bubbles: true }));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', ctrlKey: true, shiftKey: true, repeat: true, bubbles: true }));
    await waitFor(() => expect(mocks.liveStop).toHaveBeenCalledTimes(1));

    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.append(dialog);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', ctrlKey: true, shiftKey: true, bubbles: true }));
    expect(mocks.liveStop).toHaveBeenCalledTimes(1);
    dialog.remove();
  });
});
