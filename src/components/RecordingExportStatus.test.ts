// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import RecordingExportStatus from './RecordingExportStatus.svelte';
import { recordingExportStore } from '../lib/stores/recordingExport.svelte';
import { toastStore } from '../lib/stores/toast.svelte';

const mocks = vi.hoisted(() => ({
  exportRecording: vi.fn(),
  cancelRecordingExport: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class<T> {
    onmessage: (event: T) => void = () => {};
  },
}));

vi.mock('../lib/bindings', () => ({
  commands: {
    libraryRecordingExport: (...args: unknown[]) => mocks.exportRecording(...args),
    libraryRecordingExportCancel: (...args: unknown[]) => mocks.cancelRecordingExport(...args),
  },
}));

afterEach(() => {
  cleanup();
  toastStore.reset();
});

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.exportRecording.mockReset().mockResolvedValue({ status: 'ok', data: false });
  mocks.cancelRecordingExport.mockReset().mockResolvedValue({ status: 'ok', data: null });
});

describe('RecordingExportStatus', () => {
  it('lets the user choose a format and sends it with the native save request', async () => {
    render(RecordingExportStatus);
    recordingExportStore.request('session-1');

    const flacChoice = await screen.findByRole('radio', { name: 'FLAC' }) as HTMLInputElement;
    expect((screen.getByRole('radio', { name: 'WAV' }) as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(flacChoice);
    expect(flacChoice.checked).toBe(true);

    await fireEvent.click(await screen.findByRole('button', { name: 'Chọn nơi lưu' }));
    await waitFor(() => expect(mocks.exportRecording).toHaveBeenCalledTimes(1));
    expect(mocks.exportRecording.mock.calls[0]?.[0]).toBe('session-1');
    expect(mocks.exportRecording.mock.calls[0]?.[1]).toBe('flac');
  });

  it('shows progress and lets the user cancel an active export', async () => {
    let resolveExport = (_result: { status: 'ok'; data: boolean }) => {};
    mocks.exportRecording.mockImplementation(() => new Promise((resolve) => {
      resolveExport = resolve;
    }));

    render(RecordingExportStatus);
    recordingExportStore.request('session-2');
    await fireEvent.click(await screen.findByRole('button', { name: 'Chọn nơi lưu' }));

    await waitFor(() => expect(mocks.exportRecording).toHaveBeenCalledTimes(1));
    const channel = mocks.exportRecording.mock.calls[0]?.[2] as {
      onmessage: (progress: { processedSeconds: number; totalSeconds: number }) => void;
    };
    channel.onmessage({ processedSeconds: 2, totalSeconds: 10 });

    const progress = await screen.findByRole('progressbar');
    await waitFor(() => expect(progress.getAttribute('aria-valuenow')).toBe('20'));
    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ tải' }));

    expect(mocks.cancelRecordingExport).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Đang huỷ…' })).toBeTruthy();

    resolveExport({ status: 'ok', data: false });
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });
});
