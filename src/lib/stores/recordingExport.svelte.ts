import { Channel } from '@tauri-apps/api/core';
import { i18n } from '../../i18n/index.svelte';
import {
  commands,
  type AppError,
  type RecordingExportFormat,
  type RecordingExportProgress,
} from '../bindings';
import { errorHint, errorTitle } from '../errors';
import { toastStore } from './toast.svelte';

export interface RecordingExportState {
  sessionId: string;
  progress: RecordingExportProgress | null;
  cancelling: boolean;
}

function exportErrorMessage(error: AppError): string {
  if (error.category === 'storage') return i18n.t('recordingExport.error.storage');
  return `${errorTitle(error)}. ${errorHint(error)}`;
}

export interface RecordingExportChoice {
  sessionId: string;
  format: RecordingExportFormat;
}

export function createRecordingExportStore() {
  let active = $state<RecordingExportState | null>(null);
  let formatDialog = $state<RecordingExportChoice | null>(null);
  let generation = 0;

  function request(sessionId: string): void {
    if (active || formatDialog) return;
    formatDialog = { sessionId, format: 'wav' };
  }

  function chooseFormat(format: RecordingExportFormat): void {
    if (!formatDialog) return;
    formatDialog = { ...formatDialog, format };
  }

  function cancelChoice(): void {
    formatDialog = null;
  }

  async function start(sessionId: string, format: RecordingExportFormat): Promise<void> {
    if (active) return;
    formatDialog = null;
    const currentGeneration = ++generation;
    active = { sessionId, progress: null, cancelling: false };
    const channel = new Channel<RecordingExportProgress>();
    channel.onmessage = (progress) => {
      const current = active;
      if (generation !== currentGeneration || current?.sessionId !== sessionId) return;
      active = { ...current, progress };
    };

    try {
      const result = await commands.libraryRecordingExport(sessionId, format, channel);
      if (generation !== currentGeneration) return;
      if (result.status === 'error') {
        toastStore.show(exportErrorMessage(result.error), 'error');
      } else if (result.data) {
        toastStore.show(i18n.t('recordingExport.toast.saved'), 'info', 4_000);
      }
    } catch {
      if (generation === currentGeneration) {
        toastStore.show(i18n.t('recordingExport.error.generic'), 'error');
      }
    } finally {
      if (generation === currentGeneration) active = null;
    }
  }

  async function cancel(): Promise<void> {
    const current = active;
    if (!current?.progress || current.cancelling) return;
    active = { ...current, cancelling: true };
    try {
      const result = await commands.libraryRecordingExportCancel();
      if (result.status === 'error') {
        toastStore.show(exportErrorMessage(result.error), 'error');
        if (active?.sessionId === current.sessionId) {
          active = { ...active, cancelling: false };
        }
      }
    } catch {
      toastStore.show(i18n.t('recordingExport.error.generic'), 'error');
      if (active?.sessionId === current.sessionId) {
        active = { ...active, cancelling: false };
      }
    }
  }

  return {
    get active() {
      return active;
    },
    get formatDialog() {
      return formatDialog;
    },
    request,
    chooseFormat,
    cancelChoice,
    start,
    cancel,
  };
}

export const recordingExportStore = createRecordingExportStore();
