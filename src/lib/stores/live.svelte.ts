// Process-wide Live view state. The actor owns the recording; this singleton
// owns only the current Channel subscription and the last delivered UI state.
import { Channel } from '@tauri-apps/api/core';
import {
  commands,
  type AppError,
  type Category,
  type ConnectionState,
  type LiveEvent,
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

const EMPTY_SNAPSHOT: LiveSnapshot = {
  sessionId: null,
  transcriptId: null,
  recording: 'stopped',
  connection: { type: 'stopped' },
  transcription: 'stopped',
  errorCategory: null,
  durationSec: null,
};

const UNAVAILABLE_ERROR: AppError = {
  category: 'network',
  code: 'network',
  detailRedacted: 'live command unavailable',
};

export function createLiveStore() {
  let snapshot = $state<LiveSnapshot>({ ...EMPTY_SNAPSHOT });
  let finalizedSessionId = $state<string | null>(null);
  let sources = $state<LiveSources | null>(null);
  let sourceStatus = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
  let sourceError = $state<AppError | null>(null);
  let deniedSource = $state<string | null>(null);
  let lines = $state<LiveLine[]>([]);
  let draft = $state('');
  let error = $state<Category | null>(null);
  let status = $state<'idle' | 'subscribed' | 'error'>('idle');
  let lastSeq: number | null = null;
  let generation = 0;
  let subscribers = 0;
  let pendingSubscribe: Promise<void> | null = null;

  function applyEvent(event: LiveEvent, forGeneration: number): void {
    if (forGeneration !== generation) return;
    if (event.type === 'ready') {
      const changedSession = snapshot.sessionId !== event.snapshot.sessionId;
      snapshot = event.snapshot;
      error = event.snapshot.errorCategory;
      if (changedSession) {
        finalizedSessionId = null;
        lines = [];
        draft = '';
      }
      lastSeq = event.seq;
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
    const nextChannel = new Channel<LiveEvent>();
    nextChannel.onmessage = (event) => applyEvent(event, forGeneration);
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
    locale: string,
    tagIds: string[],
  ): Promise<AppError | null> {
    try {
      const result = await commands.liveStart(source, language, locale, tagIds);
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

  function clearPermissionDenial(): void {
    deniedSource = null;
    if (error === 'permission') error = null;
  }

  function reset(): void {
    generation += 1;
    finalizedSessionId = null;
    snapshot = { ...EMPTY_SNAPSHOT };
    lines = [];
    draft = '';
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
    clearPermissionDenial,
    reset,
  };
}

export const liveStore = createLiveStore();
