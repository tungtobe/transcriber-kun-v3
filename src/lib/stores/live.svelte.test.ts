// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LiveEvent } from '../bindings';

const mocks = vi.hoisted(() => ({
  liveSubscribe: vi.fn(),
  liveSources: vi.fn(),
  liveStart: vi.fn(),
  liveStop: vi.fn(),
  liveSetSource: vi.fn(),
}));

class FakeChannel<T> {
  id = 1;
  onmessage: (event: T) => void = () => {};
}

vi.mock('@tauri-apps/api/core', () => ({ Channel: FakeChannel }));
vi.mock('../bindings', () => ({
  commands: {
    liveSubscribe: (...args: unknown[]) => mocks.liveSubscribe(...args),
    liveSources: (...args: unknown[]) => mocks.liveSources(...args),
    liveStart: (...args: unknown[]) => mocks.liveStart(...args),
    liveStop: (...args: unknown[]) => mocks.liveStop(...args),
    liveSetSource: (...args: unknown[]) => mocks.liveSetSource(...args),
  },
}));

let channels: FakeChannel<LiveEvent>[] = [];

function snapshot(sessionId: string | null = 'session-1') {
  return {
    sessionId,
    transcriptId: sessionId ? 'transcript-1' : null,
    recording: sessionId ? 'active' as const : 'stopped' as const,
    connection: { type: sessionId ? 'connecting' as const : 'stopped' as const },
    durationSec: 0,
  };
}

beforeEach(() => {
  vi.resetModules();
  channels = [];
  mocks.liveSubscribe.mockReset().mockImplementation((channel: FakeChannel<LiveEvent>) => {
    channels.push(channel);
    return Promise.resolve({ status: 'ok', data: null });
  });
  mocks.liveSources.mockReset().mockResolvedValue({ status: 'ok', data: {
    microphones: [], defaultMicrophone: null, systemAvailable: true,
    microphonePermission: 'notDetermined', systemPermission: 'unknown',
  } });
  mocks.liveStart.mockReset().mockResolvedValue({ status: 'ok', data: 'session-1' });
  mocks.liveStop.mockReset().mockResolvedValue({ status: 'ok', data: null });
  mocks.liveSetSource.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

describe('liveStore', () => {
  it('sends setup choices as one typed start and applies recording, connection, segment and gap events in order', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    await store.subscribe();
    const channel = channels[0];
    channel.onmessage({ type: 'ready', seq: 0, snapshot: snapshot() });

    const tagIds = ['tag-a', 'tag-b'];
    expect(await store.start('mixed:mic-1', 'vi', 'vi', tagIds)).toBeNull();
    expect(mocks.liveStart).toHaveBeenCalledTimes(1);
    expect(mocks.liveStart).toHaveBeenCalledWith('mixed:mic-1', 'vi', 'vi', tagIds);

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

  it('keeps permission denial scoped to the source that failed until the user rechecks it', async () => {
    const { createLiveStore } = await import('./live.svelte');
    const store = createLiveStore();
    mocks.liveStart.mockResolvedValueOnce({
      status: 'error',
      error: { category: 'permission', code: 'permission', detailRedacted: 'capture denied' },
    });

    const error = await store.start('mic:device-a', 'auto', 'en', []);
    expect(error?.category).toBe('permission');
    expect(store.deniedSource).toBe('mic:device-a');
    store.clearPermissionDenial();
    expect(store.deniedSource).toBeNull();
  });
});
