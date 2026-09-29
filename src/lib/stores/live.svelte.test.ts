// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LiveEvent } from '../bindings';

const mocks = vi.hoisted(() => ({
  liveSubscribe: vi.fn(),
  liveUnsubscribe: vi.fn(),
  librarySessionDetail: vi.fn(),
  liveSources: vi.fn(),
  liveStart: vi.fn(),
  liveStop: vi.fn(),
  liveContinueRecordingOnly: vi.fn(),
  liveSetSource: vi.fn(),
  liveSetTarget: vi.fn(),
  liveSetTts: vi.fn(),
  liveRedetect: vi.fn(),
}));

let nextChannelId = 1;
class FakeChannel<T> {
  id = nextChannelId++;
  onmessage: (event: T) => void = () => {};
}

vi.mock('@tauri-apps/api/core', () => ({ Channel: FakeChannel }));
vi.mock('../bindings', () => ({
  commands: {
    liveSubscribe: (...args: unknown[]) => mocks.liveSubscribe(...args),
    liveUnsubscribe: (...args: unknown[]) => mocks.liveUnsubscribe(...args),
    librarySessionDetail: (...args: unknown[]) => mocks.librarySessionDetail(...args),
    liveSources: (...args: unknown[]) => mocks.liveSources(...args),
    liveStart: (...args: unknown[]) => mocks.liveStart(...args),
    liveStop: (...args: unknown[]) => mocks.liveStop(...args),
    liveContinueRecordingOnly: (...args: unknown[]) => mocks.liveContinueRecordingOnly(...args),
    liveSetSource: (...args: unknown[]) => mocks.liveSetSource(...args),
    liveSetTarget: (...args: unknown[]) => mocks.liveSetTarget(...args),
    liveSetTts: (...args: unknown[]) => mocks.liveSetTts(...args),
    liveRedetect: (...args: unknown[]) => mocks.liveRedetect(...args),
  },
}));

let channels: FakeChannel<LiveEvent>[] = [];

function snapshot(sessionId: string | null = 'session-1') {
  return {
    sessionId,
    transcriptId: sessionId ? 'transcript-1' : null,
    recording: sessionId ? 'active' as const : 'stopped' as const,
    connection: { type: sessionId ? 'connecting' as const : 'stopped' as const },
    transcription: sessionId ? 'active' as const : 'stopped' as const,
    errorCategory: null,
    durationSec: 0,
    target: 'none' as const,
    tts: false,
    speaking: false,
  };
}

beforeEach(() => {
  vi.resetModules();
  channels = [];
  nextChannelId = 1;
  mocks.liveUnsubscribe.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.librarySessionDetail.mockReset().mockResolvedValue({
    status: 'ok',
    data: { transcript: null },
  });
  mocks.liveSubscribe.mockReset().mockImplementation((channel: FakeChannel<LiveEvent>) => {
    channels.push(channel);
    return Promise.resolve({ status: 'ok', data: null });
  });
  mocks.liveSources.mockReset().mockResolvedValue({ status: 'ok', data: {
    microphones: [], defaultMicrophone: null, systemAvailable: true,
    microphonePermission: 'notDetermined', systemPermission: 'unknown',
  } });
  mocks.liveStart.mockReset().mockResolvedValue({ status: 'ok', data: 'session-1' });
  mocks.liveStop.mockReset().mockResolvedValue({ status: 'ok', data: 'session-1' });
  mocks.liveContinueRecordingOnly.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetSource.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetTarget.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetTts.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveRedetect.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

describe('liveStore', () => {
  it('sends setup choices as one typed start and applies recording, connection, segment and gap events in order', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });

    const tagIds = ['tag-a', 'tag-b'];
    expect(await store.start('mixed:mic-1', 'vi', 'vi', 'vi', tagIds)).toBeNull();
    expect(mocks.liveStart).toHaveBeenCalledTimes(1);
    expect(mocks.liveStart).toHaveBeenCalledWith('mixed:mic-1', 'vi', 'vi', 'vi', tagIds);

    channel.onmessage({ type: 'connection', seq: 1, state: { type: 'connected' } });
    channel.onmessage({ type: 'delta', seq: 2, text: 'Xin chào.' });
    channel.onmessage({
      type: 'segment', seq: 3,
      segment: { startSec: 0, endSec: 1.2, text: 'Xin chào.' },
    });
    channel.onmessage({
      type: 'gap', seq: 4, startSec: 1.2, endSec: 2, reason: 'disconnected',
    });

    expect(store.snapshot.connection).toEqual({ type: 'connected' });
    expect(store.lines).toEqual([
      { seq: 3, kind: 'segment', segment: { startSec: 0, endSec: 1.2, text: 'Xin chào.' } },
      { seq: 4, kind: 'gap', startSec: 1.2, endSec: 2, reason: 'disconnected' },
    ]);
    expect(store.draft).toBe('');
  });

  it('retains the same session on route remount and resubscribes after a missing sequence', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const first = channels[0];
    first.onmessage({ type: 'ready', seq: 5, snapshot: snapshot() });
    first.onmessage({
      type: 'segment', seq: 6,
      segment: { startSec: 0, endSec: 1, text: 'Saved in memory' },
    });
    store.unsubscribe();

    await store.subscribe();
    const second = channels[1];
    second.onmessage({ type: 'ready', seq: 6, snapshot: snapshot() });
    expect(store.snapshot.sessionId).toBe('session-1');
    expect(store.lines).toHaveLength(1);

    second.onmessage({ type: 'connection', seq: 8, state: { type: 'reconnecting', sinceMs: 100 } });
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.liveSubscribe).toHaveBeenCalledTimes(3);
  });

  it('returns the stopped session ID and records automatic final events', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });

    expect(await store.stop()).toEqual({ sessionId: 'session-1', error: null });
    channel.onmessage({
      type: 'final', seq: 1, sessionId: 'session-1', transcriptId: 'transcript-1', durationSec: 2,
    });
    expect(store.finalizedSessionId).toBe('session-1');
    expect(store.snapshot.recording).toBe('stopped');
  });

  it('clears a finished session so the next Live visit starts fresh, even when the backend resends it', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    channel.onmessage({ type: 'segment', seq: 1, segment: { startSec: 0, endSec: 1, text: 'cũ' } });
    channel.onmessage({
      type: 'final', seq: 2, sessionId: 'session-1', transcriptId: 'transcript-1', durationSec: 2,
    });

    store.clearFinishedSession();
    expect(store.snapshot.sessionId).toBeNull();
    expect(store.lines).toHaveLength(0);
    expect(store.finalizedSessionId).toBeNull();

    // The backend keeps the stopped snapshot and replays it on resubscribe.
    channel.onmessage({
      type: 'ready', seq: 3, snapshot: { ...snapshot(), recording: 'stopped' as const },
    });
    expect(store.snapshot.sessionId).toBeNull();

    // A genuinely new session is accepted again.
    channel.onmessage({ type: 'ready', seq: 4, snapshot: snapshot('session-2') });
    expect(store.snapshot.sessionId).toBe('session-2');
  });

  it('does not clear a running session', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    channels[0].onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    store.clearFinishedSession();
    expect(store.snapshot.sessionId).toBe('session-1');
  });

  it('keeps permission denial scoped to the source that failed until the user rechecks it', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    mocks.liveStart.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'permission', code: 'permission', detailRedacted: 'capture denied' },
    });

    const error = await store.start('mic:device-a', 'auto', 'none', 'en', []);
    expect(error?.category).toBe('permission');
    expect(store.deniedSource).toBe('mic:device-a');
    store.clearPermissionDenial();
    expect(store.deniedSource).toBeNull();
  });

  it('stores only category codes and restores the actor-owned recording-only choice on remount', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const first = channels[0];
    first.onmessage({
      type: 'ready', seq: 0,
      snapshot: { ...snapshot(), transcription: 'setupRejected' },
    });
    first.onmessage({
      type: 'error', seq: 1,
      error: { category: 'quota', code: 'quota', detailRedacted: 'private diagnostic detail' },
    });
    expect(store.error).toBe('quota');
    expect(JSON.stringify(store.error)).not.toContain('private diagnostic detail');

    store.unsubscribe();
    await store.subscribe();
    channels[1].onmessage({
      type: 'ready', seq: 1,
      snapshot: { ...snapshot(), transcription: 'recordingOnly', errorCategory: 'quota' },
    });
    expect(store.snapshot.transcription).toBe('recordingOnly');
    expect(store.error).toBe('quota');
  });

  it('unregisters its Channel when the last subscriber leaves and when it resubscribes', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const first = channels[0];
    first.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    expect(mocks.liveUnsubscribe).not.toHaveBeenCalled();

    store.unsubscribe();
    expect(mocks.liveUnsubscribe).toHaveBeenCalledTimes(1);
    expect(mocks.liveUnsubscribe).toHaveBeenLastCalledWith(first);

    await store.subscribe();
    const second = channels[1];
    second.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    // A seq gap replaces the Channel: the stale one is dropped first.
    second.onmessage({ type: 'connection', seq: 5, state: { type: 'connected' } });
    await Promise.resolve();
    await Promise.resolve();
    expect(mocks.liveUnsubscribe).toHaveBeenLastCalledWith(second);
    expect(channels).toHaveLength(3);
  });

  it('reloads persisted lines after a sequence gap and keeps only newer unflushed ones', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const first = channels[0];
    first.onmessage({ type: 'ready', seq: 5, snapshot: snapshot() });
    first.onmessage({
      type: 'segment', seq: 6,
      segment: { startSec: 0, endSec: 1, text: 'streamed one' },
    });
    first.onmessage({
      type: 'segment', seq: 7,
      segment: { startSec: 5, endSec: 6, text: 'not flushed yet' },
    });
    mocks.librarySessionDetail.mockResolvedValue({
      status: 'ok',
      data: {
        transcript: {
          segments: [
            { idx: 0, startSec: 0, endSec: 1, kind: 'text', gapReason: null, text: 'persisted one' },
            { idx: 1, startSec: 1, endSec: 2, kind: 'gap', gapReason: 'disconnected', text: '' },
            { idx: 2, startSec: 2, endSec: 3, kind: 'text', gapReason: null, text: 'persisted two' },
          ],
        },
      },
    });

    // Events 8..9 were lost: the store resubscribes.
    first.onmessage({ type: 'connection', seq: 10, state: { type: 'connected' } });
    await vi.waitFor(() => expect(channels).toHaveLength(2));
    channels[1].onmessage({ type: 'ready', seq: 10, snapshot: snapshot() });
    await vi.waitFor(() => expect(mocks.librarySessionDetail).toHaveBeenCalledWith('session-1'));
    await vi.waitFor(() => expect(store.lines.length).toBe(4));

    const texts = store.lines.map((line) => (line.kind === 'segment' ? line.segment.text : `gap:${line.reason}`));
    expect(texts).toEqual([
      'persisted one',
      'gap:disconnected',
      'persisted two',
      'not flushed yet',
    ]);
  });

  it('keeps the streamed lines when the persisted transcript cannot be loaded', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    mocks.librarySessionDetail.mockRejectedValue(new Error('offline'));
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    channel.onmessage({
      type: 'segment', seq: 1,
      segment: { startSec: 0, endSec: 1, text: 'kept' },
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(store.lines).toHaveLength(1);
  });

  it('clears lines, draft and the finalized session when a new session starts', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });
    channel.onmessage({
      type: 'segment', seq: 1,
      segment: { startSec: 0, endSec: 1, text: 'old session' },
    });
    channel.onmessage({ type: 'delta', seq: 2, text: 'half a sentence' });
    channel.onmessage({
      type: 'final', seq: 3, sessionId: 'session-1', transcriptId: 'transcript-1', durationSec: 1,
    });
    expect(store.lines).toHaveLength(1);
    expect(store.draft).toBe('half a sentence');
    expect(store.finalizedSessionId).toBe('session-1');

    expect(await store.start('system', 'auto', 'none', 'en', [])).toBeNull();

    expect(store.lines).toEqual([]);
    expect(store.draft).toBe('');
    expect(store.finalizedSessionId).toBeNull();
  });

  it('tracks the speaker toggle and speaking flag from events and forwards setTts', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: { ...snapshot(), target: 'vi' as const } });
    channel.onmessage({ type: 'tts', seq: 1, enabled: true });
    channel.onmessage({ type: 'speaking', seq: 2, speaking: true });
    expect(store.snapshot.tts).toBe(true);
    expect(store.snapshot.speaking).toBe(true);
    channel.onmessage({ type: 'tts', seq: 3, enabled: false });
    expect(store.snapshot.speaking).toBe(false);
    expect(await store.setTts(true)).toBeNull();
    expect(mocks.liveSetTts).toHaveBeenCalledWith(true);
  });

  it('keeps translated text in memory only, resets it per session and follows the confirmed Target', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: { ...snapshot(), target: 'vi' as const } });

    channel.onmessage({ type: 'deltaTranslated', seq: 1, text: 'Xin ' });
    channel.onmessage({ type: 'deltaTranslated', seq: 2, text: 'chào.' });
    expect(store.translatedDraft).toBe('Xin chào.');
    channel.onmessage({
      type: 'segmentTranslated', seq: 3,
      segment: { startSec: 0, endSec: 1, text: 'Xin chào.' },
    });
    expect(store.translatedDraft).toBe('');
    expect(store.translatedLines.map((line) => line.segment.text)).toEqual(['Xin chào.']);
    expect(store.lines).toEqual([]);

    channel.onmessage({ type: 'target', seq: 4, target: 'en' });
    expect(store.snapshot.target).toBe('en');

    // A different session starts with a clean translation.
    channel.onmessage({ type: 'ready', seq: 9, snapshot: snapshot('session-2') });
    expect(store.translatedLines).toEqual([]);
  });

  it('redetect returns null on success and the typed error on failure', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    expect(await store.redetect()).toBeNull();
    mocks.liveRedetect.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'network', code: 'network', detailRedacted: 'x' },
    });
    expect((await store.redetect())?.category).toBe('network');
  });

  it('setTarget forwards the choice and surfaces a model failure as a banner category', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    expect(await store.setTarget('en')).toBeNull();
    expect(mocks.liveSetTarget).toHaveBeenCalledWith('en');

    mocks.liveSetTarget.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'model', code: 'model', detailRedacted: 'x' },
    });
    const error = await store.setTarget('ja');
    expect(error?.category).toBe('model');
    expect(store.error).toBe('model');

    mocks.liveSetTarget.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'network', code: 'network', detailRedacted: 'x' },
    });
    expect((await store.setTarget('vi'))?.category).toBe('network');
  });
});
