// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { installKeymap, keymap } from '../lib/keymap';
import { liveStore } from '../lib/stores/live.svelte';
import { configureRouter } from '../lib/router';
import type { LiveEvent, LiveSnapshot } from '../lib/bindings';
import Live from './Live.svelte';

const mocks = vi.hoisted(() => ({
  liveSubscribe: vi.fn(),
  liveUnsubscribe: vi.fn(),
  librarySessionDetail: vi.fn(),
  liveSources: vi.fn(),
  liveStart: vi.fn(),
  liveStop: vi.fn(),
  liveContinueRecordingOnly: vi.fn(),
  liveSetSource: vi.fn(),
  liveOpenPermissionSettings: vi.fn(),
  keysLoad: vi.fn(),
  tagsLoad: vi.fn(),
  notesFlush: vi.fn((_sessionId: string) => Promise.resolve(true)),
  notesRetry: vi.fn((_sessionId: string) => Promise.resolve(true)),
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
    liveUnsubscribe: (...args: unknown[]) => mocks.liveUnsubscribe(...args),
    librarySessionDetail: (...args: unknown[]) => mocks.librarySessionDetail(...args),
    liveSources: (...args: unknown[]) => mocks.liveSources(...args),
    liveStart: (...args: unknown[]) => mocks.liveStart(...args),
    liveStop: (...args: unknown[]) => mocks.liveStop(...args),
    liveContinueRecordingOnly: (...args: unknown[]) => mocks.liveContinueRecordingOnly(...args),
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
    flush: (sessionId: string) => mocks.notesFlush(sessionId),
    view: () => ({ body: '', status: 'idle', savedAtMs: null, loadError: false }),
    setBody: vi.fn(),
    retry: (sessionId: string) => mocks.notesRetry(sessionId),
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
    transcription: sessionId ? 'active' : 'stopped',
    errorCategory: null,
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
  mocks.liveUnsubscribe.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.librarySessionDetail.mockReset().mockResolvedValue({ status: 'ok', data: { transcript: null } });
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
  mocks.liveStop.mockReset().mockResolvedValue({ status: 'ok', data: 'session-1' });
  mocks.liveContinueRecordingOnly.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetSource.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveOpenPermissionSettings.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.keysLoad.mockReset().mockResolvedValue(undefined);
  mocks.tagsLoad.mockReset().mockResolvedValue(undefined);
  mocks.notesFlush.mockReset().mockResolvedValue(true);
  mocks.notesRetry.mockReset().mockResolvedValue(true);
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

describe('Live failure handling', () => {
  it('flushes notes before stopping and opens the returned session ID', async () => {
    backendSessionId = 'session-1';
    render(Live);
    const stop = await screen.findByRole('button', { name: 'Stop recording' });

    await fireEvent.click(stop);

    await waitFor(() => expect(window.location.pathname).toBe('/session/session-1'));
    expect(mocks.notesFlush).toHaveBeenCalledWith('session-1');
    expect(mocks.notesFlush.mock.invocationCallOrder[0]).toBeLessThan(mocks.liveStop.mock.invocationCallOrder[0]);
    expect(mocks.liveStop).toHaveBeenCalledTimes(1);
  });

  it('shows the saving overlay while recording and Proxy finalization are pending', async () => {
    backendSessionId = 'session-1';
    let finishStop!: (value: { status: 'ok'; data: string }) => void;
    mocks.liveStop.mockImplementationOnce(() => new Promise((resolve) => { finishStop = resolve; }));
    render(Live);

    await fireEvent.click(await screen.findByRole('button', { name: 'Stop recording' }));
    expect(await screen.findByText('Saving session…')).toBeTruthy();
    expect(screen.getByText('Finalizing the recording and creating the Proxy')).toBeTruthy();

    finishStop({ status: 'ok', data: 'session-1' });
    await waitFor(() => expect(window.location.pathname).toBe('/session/session-1'));
  });

  it('keeps the Live screen and skips stop when notes fail to flush, then allows retry', async () => {
    backendSessionId = 'session-1';
    mocks.notesFlush.mockResolvedValue(false);
    render(Live);
    await fireEvent.click(await screen.findByRole('button', { name: 'Stop recording' }));

    expect(await screen.findByText('Notes could not be saved. Retry before opening the session.')).toBeTruthy();
    expect(mocks.liveStop).not.toHaveBeenCalled();
    expect(window.location.pathname).toBe('/live');

    await fireEvent.click(screen.getByRole('button', { name: 'Retry save and continue' }));
    await waitFor(() => expect(window.location.pathname).toBe('/session/session-1'));
    expect(mocks.notesRetry).toHaveBeenCalledWith('session-1');
    expect(mocks.liveStop).toHaveBeenCalledTimes(1);
  });

  it('flushes notes and opens a session finalized by an automatic device stop', async () => {
    backendSessionId = 'session-1';
    render(Live);
    await waitFor(() => expect(channels).toHaveLength(1));

    channels[0].onmessage({
      type: 'final', seq: 1, sessionId: 'session-1', transcriptId: 'transcript-1', durationSec: 1,
    });

    await waitFor(() => expect(window.location.pathname).toBe('/session/session-1'));
    expect(mocks.notesFlush).toHaveBeenCalledWith('session-1');
    expect(mocks.liveStop).not.toHaveBeenCalled();
  });

  it('shows reconnect elapsed time while keeping the recording timer active', async () => {
    backendSessionId = 'session-1';
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-28T12:00:20Z'));
    render(Live);
    await waitFor(() => expect(channels).toHaveLength(1));
    channels[0].onmessage({
      type: 'connection', seq: 1,
      state: { type: 'reconnecting', sinceMs: Date.parse('2026-09-28T11:00:20Z') },
    });

    await waitFor(() => expect(screen.getByText('Reconnecting · 60:00')).toBeTruthy());
    expect(screen.getByText(/Connection lost\. Reconnecting automatically \(60:00\); recording continues\./)).toBeTruthy();
    expect(screen.getByText('Recording').closest('.status-pill')?.className).toContain('status-recording');
  });

  it('keeps WAV recording after setup rejection and restores the recording-only choice', async () => {
    backendSessionId = 'session-1';
    render(Live);
    await waitFor(() => expect(channels).toHaveLength(1));
    channels[0].onmessage({ type: 'transcription', seq: 1, state: 'setupRejected' });

    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Live setup failed five times'));
    const continueButton = screen.getByRole('button', { name: 'Continue recording only' });
    const stopButton = screen.getByRole('button', { name: /^Stop$/ });
    expect(continueButton).toBeTruthy();
    expect(stopButton).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open Gemini Settings' }).getAttribute('href')).toBe('/settings/gemini');
    await fireEvent.click(continueButton);

    await waitFor(() => expect(mocks.liveContinueRecordingOnly).toHaveBeenCalledTimes(1));
    expect(screen.getByText('Recording audio only')).toBeTruthy();
    expect(screen.getByText('Transcript stopped')).toBeTruthy();
    expect(screen.getByText('Live transcription has stopped.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Stop recording' })).toBeTruthy();
    expect(mocks.liveStop).not.toHaveBeenCalled();
  });

  it('renders model, quota and auth copy with a Settings action without exposing error details', async () => {
    backendSessionId = 'session-1';
    render(Live);
    await waitFor(() => expect(channels).toHaveLength(1));
    await act(() => {
      channels[0].onmessage({
        type: 'error', seq: 1,
        error: { category: 'quota', code: 'quota', detailRedacted: 'secret-key-and-url' },
      });
    });

    await waitFor(() => expect(liveStore.error).toBe('quota'));
    await waitFor(() => expect(screen.getByText('The Gemini key has reached its usage limit.')).toBeTruthy());
    expect(screen.getByRole('link', { name: 'Open Gemini Settings' }).getAttribute('href')).toBe('/settings/gemini');
    expect(document.body.textContent).not.toContain('secret-key-and-url');
    await act(() => {
      channels[0].onmessage({
        type: 'error', seq: 2,
        error: { category: 'auth', code: 'auth', detailRedacted: 'secret-key-and-url' },
      });
    });
    expect(screen.getByText('Gemini authentication failed. Check the key in Settings.')).toBeTruthy();
    await act(() => {
      channels[0].onmessage({
        type: 'error', seq: 3,
        error: { category: 'model', code: 'model', detailRedacted: 'secret-key-and-url' },
      });
    });
    expect(screen.getByText('The selected Live model is unavailable.')).toBeTruthy();
    expect(document.body.textContent).not.toContain('secret-key-and-url');
  });

  it('shows a disconnected gap with both sample-clock endpoints inline', async () => {
    backendSessionId = 'session-1';
    render(Live);
    await waitFor(() => expect(channels).toHaveLength(1));
    channels[0].onmessage({
      type: 'gap', seq: 1, startSec: 12, endSec: 18, reason: 'disconnected',
    });
    await waitFor(() => expect(screen.getByText('Connection lost 00:12–00:18')).toBeTruthy());
    expect(screen.getByText('Connection lost 00:12–00:18').closest('.gap-line')).toBeTruthy();
  });
});
