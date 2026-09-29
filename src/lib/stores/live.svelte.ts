// Process-wide Live view state. The actor owns the recording; this singleton
// owns only the current Channel subscription and the last delivered UI state.
import { Channel } from '@tauri-apps/api/core';
import {
  commands,
  type AppError,
  type Category,
  type ConnectionState,
  type LiveEvent,
  type LiveTarget,
  type LiveSegment,
  type LiveSnapshot,
  type LiveSources,
  type RecordingState,
  type TranscriptionState,
  type TranscribeLanguage,
} from '../bindings';

export type LiveLine =
  | { seq: number; kind: 'segment'; segment: LiveSegment }
  | { seq: number; kind: 'gap'; startSec: number | null; endSec: number | null; reason: string };

/** A translated sentence. Translations live only in memory (never persisted). */
export type TranslatedLine = { seq: number; segment: LiveSegment };

const EMPTY_SNAPSHOT: LiveSnapshot = {
  sessionId: null,
  transcriptId: null,
  recording: 'stopped',
  connection: { type: 'stopped' },
  transcription: 'stopped',
  errorCategory: null,
  durationSec: null,
  target: 'none',
};

const UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'live command unavailable',
};

export function createLiveStore() {
  let snapshot = $state<LiveSnapshot>({ ...EMPTY_SNAPSHOT });
  // Phiên đã kết thúc mà người dùng rời đi: backend vẫn giữ snapshot 'stopped'
  // của nó và gửi lại trong `ready` khi đăng ký lại, nên phải bỏ qua.
  let dismissedSessionId: string | null = null;
  let finalizedSessionId = $state<string | null>(null);
  let sources = $state<LiveSources | null>(null);
  let sourceStatus = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
  let sourceError = $state<AppError | null>(null);
  let deniedSource = $state<string | null>(null);
  let lines = $state<LiveLine[]>([]);
  let draft = $state('');
  let translatedLines = $state<TranslatedLine[]>([]);
  let translatedDraft = $state('');
  let error = $state<Category | null>(null);
  let status = $state<'idle' | 'subscribed' | 'error'>('idle');
  let lastSeq: number | null = null;
  let generation = 0;
  let subscribers = 0;
  let pendingSubscribe: Promise<void> | null = null;
  let channel: Channel<LiveEvent> | null = null;

  // Tells the actor to forget a Channel this store no longer reads. A stale
  // registration would otherwise keep receiving (and failing on) every event.
  function dropChannel(): void {
    const stale = channel;
    channel = null;
    if (!stale) return;
    try {
      void Promise.resolve(commands.liveUnsubscribe(stale)).catch(() => {});
    } catch {
      // Best effort: the actor also drops a Channel whose send fails.
    }
  }

  // After a sequence gap or a remount the streamed lines may be incomplete;
  // the persisted transcript is the source of truth for everything already
  // flushed. Lines newer than the last persisted one (not flushed yet) stay.
  async function reloadPersistedLines(sessionId: string, forGeneration: number): Promise<void> {
    try {
      const result = await commands.librarySessionDetail(sessionId);
      if (forGeneration !== generation || snapshot.sessionId !== sessionId) return;
      if (result.status !== 'ok' || !result.data) return;
      const segments = result.data.transcript?.segments ?? [];
      const persisted: LiveLine[] = [];
      segments.forEach((segment, index) => {
        const seq = -(index + 1);
        if (segment.kind === 'gap') {
          persisted.push({
            seq,
            kind: 'gap',
            startSec: segment.startSec,
            endSec: segment.endSec,
            reason: segment.gapReason ?? 'disconnected',
          });
        } else if (segment.text.trim() !== '') {
          persisted.push({
            seq,
            kind: 'segment',
            segment: {
              startSec: segment.startSec ?? 0,
              endSec: segment.endSec ?? segment.startSec ?? 0,
              text: segment.text,
            },
          });
        }
      });
      const lastPersistedEnd = persisted.reduce((latest, line) => {
        const end = line.kind === 'segment' ? line.segment.endSec : line.endSec;
        return Math.max(latest, end ?? 0);
      }, 0);
      const unflushed = lines.filter((line) => {
        if (line.seq < 0) return false;
        const start = line.kind === 'segment' ? line.segment.startSec : line.startSec;
        return start !== null && start >= lastPersistedEnd - 0.01;
      });
      lines = [...persisted, ...unflushed];
    } catch {
      // Keep the streamed lines; the next Ready will retry.
    }
  }

  function applyEvent(event: LiveEvent, forGeneration: number): void {
    if (forGeneration !== generation) return;
    if (event.type === 'ready') {
      const incoming = event.snapshot.sessionId === dismissedSessionId
        && event.snapshot.recording === 'stopped'
        ? { ...EMPTY_SNAPSHOT }
        : event.snapshot;
      if (event.snapshot.sessionId && event.snapshot.sessionId !== dismissedSessionId) {
        dismissedSessionId = null;
      }
      const changedSession = snapshot.sessionId !== incoming.sessionId;
      snapshot = incoming;
      error = incoming.errorCategory;
      if (changedSession) {
        finalizedSessionId = null;
        lines = [];
        draft = '';
        translatedLines = [];
        translatedDraft = '';
      }
      lastSeq = event.seq;
      if (incoming.sessionId) {
        void reloadPersistedLines(incoming.sessionId, forGeneration);
      }
      return;
    }
    if (lastSeq !== null && event.seq !== lastSeq + 1) {
      void doSubscribe();
      return;
    }
    lastSeq = event.seq;
    switch (event.type) {
      case 'delta':
        draft += event.text;
        break;
      case 'turn':
        draft = '';
        break;
      case 'segment':
        lines = [...lines, { seq: event.seq, kind: 'segment', segment: event.segment }];
        draft = '';
        break;
      case 'deltaTranslated':
        translatedDraft += event.text;
        break;
      case 'segmentTranslated':
        translatedLines = [...translatedLines, { seq: event.seq, segment: event.segment }];
        translatedDraft = '';
        break;
      case 'target':
        snapshot = { ...snapshot, target: event.target };
        translatedDraft = '';
        break;
      case 'gap':
        lines = [...lines, {
          seq: event.seq,
          kind: 'gap',
          startSec: event.startSec,
          endSec: event.endSec,
          reason: event.reason,
        }];
        break;
      case 'recording':
        snapshot = { ...snapshot, recording: event.state as RecordingState };
        break;
      case 'connection':
        snapshot = { ...snapshot, connection: event.state as ConnectionState };
        break;
      case 'transcription':
        snapshot = { ...snapshot, transcription: event.state as TranscriptionState };
        break;
      case 'error':
        error = event.error.category;
        snapshot = { ...snapshot, errorCategory: event.error.category };
        break;
      case 'final':
        finalizedSessionId = event.sessionId;
        snapshot = {
          ...snapshot,
          sessionId: event.sessionId,
          transcriptId: event.transcriptId,
          durationSec: event.durationSec,
          recording: 'stopped',
          connection: { type: 'stopped' },
          transcription: 'stopped',
        };
        break;
      case 'done':
        if (snapshot.recording === 'active') {
          snapshot = { ...snapshot, recording: 'stopped', transcription: 'stopped' };
        }
        break;
      case 'log':
        // Actor log messages are intentionally not retained; logs must stay
        // free of transcript text and credentials.
        break;
    }
  }

  async function doSubscribe(): Promise<void> {
    generation += 1;
    const forGeneration = generation;
    lastSeq = null;
    dropChannel();
    const nextChannel = new Channel<LiveEvent>();
    nextChannel.onmessage = (event) => applyEvent(event, forGeneration);
    channel = nextChannel;
    try {
      const result = await commands.liveSubscribe(nextChannel);
      if (forGeneration !== generation) return;
      if (result.status === 'ok') {
        status = 'subscribed';
      } else {
        status = 'error';
        error = result.error.category;
      }
    } catch {
      if (forGeneration !== generation) return;
      status = 'error';
      error = UNAVAILABLE_ERROR.category;
    }
  }

  function subscribe(): Promise<void> {
    subscribers += 1;
    if (subscribers === 1 || (status === 'error' && !pendingSubscribe)) {
      const request = doSubscribe().finally(() => {
        if (pendingSubscribe === request) pendingSubscribe = null;
      });
      pendingSubscribe = request;
      return request;
    }
    return pendingSubscribe ?? Promise.resolve();
  }

  function unsubscribe(): void {
    if (subscribers === 0) return;
    subscribers -= 1;
    if (subscribers > 0) return;
    generation += 1;
    status = 'idle';
    pendingSubscribe = null;
    dropChannel();
    // The process actor keeps the session. A new Channel on remount receives
    // an atomic snapshot; the singleton transcript lines remain available.
  }

  async function loadSources(refresh = false): Promise<void> {
    sourceStatus = 'loading';
    sourceError = null;
    try {
      const result = await commands.liveSources(refresh);
      if (result.status === 'ok') {
        sources = result.data;
        sourceStatus = 'ready';
      } else {
        sourceError = result.error;
        sourceStatus = 'error';
      }
    } catch {
      sourceError = UNAVAILABLE_ERROR;
      sourceStatus = 'error';
    }
  }

  async function start(
    source: string,
    language: TranscribeLanguage,
    target: LiveTarget,
    locale: string,
    tagIds: string[],
  ): Promise<AppError | null> {
    // A new session must not show the previous one's transcript.
    finalizedSessionId = null;
    lines = [];
    draft = '';
    translatedLines = [];
    translatedDraft = '';
    try {
      const result = await commands.liveStart(source, language, target, locale, tagIds);
      if (result.status === 'ok') {
        finalizedSessionId = null;
        snapshot = {
          sessionId: result.data,
          transcriptId: null,
          recording: 'active',
          connection: { type: 'connecting' },
          transcription: 'active',
          errorCategory: null,
          durationSec: 0,
          target,
        };
        error = null;
        return null;
      }
      error = result.error.category;
      if (result.error.category === 'permission') deniedSource = source;
      return result.error;
    } catch {
      error = UNAVAILABLE_ERROR.category;
      return UNAVAILABLE_ERROR;
    }
  }

  async function stop(): Promise<{ sessionId: string | null; error: AppError | null }> {
    try {
      const result = await commands.liveStop();
      if (result.status === 'ok') return { sessionId: result.data, error: null };
      error = result.error.category;
      return { sessionId: null, error: result.error };
    } catch {
      error = UNAVAILABLE_ERROR.category;
      return { sessionId: null, error: UNAVAILABLE_ERROR };
    }
  }

  async function continueRecordingOnly(): Promise<AppError | null> {
    try {
      const result = await commands.liveContinueRecordingOnly();
      if (result.status === 'ok') {
        snapshot = { ...snapshot, transcription: 'recordingOnly' };
        return null;
      }
      error = result.error.category;
      return result.error;
    } catch {
      error = UNAVAILABLE_ERROR.category;
      return UNAVAILABLE_ERROR;
    }
  }

  async function setSource(source: string): Promise<AppError | null> {
    try {
      const result = await commands.liveSetSource(source);
      if (result.status === 'ok') return null;
      error = result.error.category;
      if (result.error.category === 'permission') deniedSource = source;
      return result.error;
    } catch {
      error = UNAVAILABLE_ERROR.category;
      return UNAVAILABLE_ERROR;
    }
  }

  /** Changes the translation Target of the running session. A failure keeps
   * the previous Target (the caller restores its dropdown from
   * `snapshot.target`); model-side failures also raise the in-place banner. */
  async function setTarget(target: LiveTarget): Promise<AppError | null> {
    try {
      const result = await commands.liveSetTarget(target);
      if (result.status === 'ok') return null;
      if (['model', 'quota', 'auth'].includes(result.error.category)) {
        error = result.error.category;
      }
      return result.error;
    } catch {
      return UNAVAILABLE_ERROR;
    }
  }

  function clearPermissionDenial(): void {
    deniedSource = null;
    if (error === 'permission') error = null;
  }

  /** Phiên vừa kết thúc và đã lưu: dọn transcript/snapshot của nó để lần vào
   * màn Live kế tiếp là màn khởi tạo phiên mới. Không đụng tới channel hay
   * bộ đếm subscriber (AppShell vẫn theo dõi trạng thái Live). Phiên đang
   * chạy hoặc bị lỗi ghi âm được giữ nguyên. */
  function clearFinishedSession(): void {
    if (snapshot.sessionId === null || snapshot.recording !== 'stopped') return;
    dismissedSessionId = snapshot.sessionId;
    finalizedSessionId = null;
    snapshot = { ...EMPTY_SNAPSHOT };
    lines = [];
    draft = '';
    translatedLines = [];
    translatedDraft = '';
    error = null;
  }

  function reset(): void {
    dismissedSessionId = null;
    generation += 1;
    dropChannel();
    finalizedSessionId = null;
    snapshot = { ...EMPTY_SNAPSHOT };
    lines = [];
    draft = '';
    translatedLines = [];
    translatedDraft = '';
    error = null;
    deniedSource = null;
    status = 'idle';
    lastSeq = null;
    subscribers = 0;
    pendingSubscribe = null;
    sources = null;
    sourceStatus = 'idle';
    sourceError = null;
  }

  return {
    get snapshot() { return snapshot; },
    get finalizedSessionId() { return finalizedSessionId; },
    get lines() { return lines; },
    get draft() { return draft; },
    get translatedLines() { return translatedLines; },
    get translatedDraft() { return translatedDraft; },
    get error() { return error; },
    get status() { return status; },
    get sources() { return sources; },
    get sourceStatus() { return sourceStatus; },
    get sourceError() { return sourceError; },
    get deniedSource() { return deniedSource; },
    subscribe,
    unsubscribe,
    loadSources,
    start,
    stop,
    continueRecordingOnly,
    setSource,
    setTarget,
    clearPermissionDenial,
    clearFinishedSession,
    reset,
  };
}

export const liveStore = createLiveStore();
