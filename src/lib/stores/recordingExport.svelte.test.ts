// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { i18n } from '../../i18n/index.svelte';
import { createRecordingExportStore } from './recordingExport.svelte';
import { toastStore } from './toast.svelte';

const mocks = vi.hoisted(() => ({
  exportRecording: vi.fn(),
  cancelRecordingExport: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class<T> {
    onmessage: (event: T) => void = () => {};
  },
}));

vi.mock('../bindings', () => ({
  commands: {
    libraryRecordingExport: (...args: unknown[]) => mocks.exportRecording(...args),
    libraryRecordingExportCancel: (...args: unknown[]) => mocks.cancelRecordingExport(...args),
  },
}));

afterEach(() => toastStore.reset());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.exportRecording.mockReset();
  mocks.cancelRecordingExport.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

describe('recordingExportStore', () => {
  it('lets the user choose WAV or FLAC before opening the save dialog', () => {
    const store = createRecordingExportStore();
    store.request('session-1');
    expect(store.formatDialog).toEqual({ sessionId: 'session-1', format: 'wav' });

    store.chooseFormat('flac');
    expect(store.formatDialog?.format).toBe('flac');
    store.cancelChoice();
    expect(store.formatDialog).toBeNull();
  });

  it('sends Cancel to Rust while a copy or encode is active', async () => {
    let capturedChannel: { onmessage: (event: { processedSeconds: number; totalSeconds: number }) => void } | null = null;
    let resolveExport: ((result: { status: 'ok'; data: boolean }) => void) | null = null;
    mocks.exportRecording.mockImplementation((_sessionId, _format, channel) => {
      capturedChannel = channel as typeof capturedChannel;
      return new Promise((resolve) => {
        resolveExport = resolve;
      });
    });
    const store = createRecordingExportStore();
    const pending = store.start('session-1', 'flac');

    capturedChannel!.onmessage({ processedSeconds: 2, totalSeconds: 10 });
    expect(store.active?.progress?.processedSeconds).toBe(2);
    await store.cancel();

    expect(mocks.cancelRecordingExport).toHaveBeenCalledTimes(1);
    expect(store.active?.cancelling).toBe(true);
    resolveExport!({ status: 'ok', data: false });
    await pending;
    expect(store.active).toBeNull();
  });
});
