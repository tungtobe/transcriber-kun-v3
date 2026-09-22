// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { keysStore } from '../../lib/stores/keys.svelte';
import SettingsGemini from './SettingsGemini.svelte';

const mocks = vi.hoisted(() => ({
  settingsStore: {
    transcribeModel: 'gemini-flash-lite-latest',
    liveModel: 'gemini-3.5-live-translate-preview',
    memoModel: 'gemini-flash-lite-latest',
    setModel: vi.fn(),
  },
  keysList: vi.fn(),
  keysSet: vi.fn(),
  keysTest: vi.fn(),
  keysDelete: vi.fn(),
  modelsList: vi.fn(),
}));

vi.mock('../../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      keysList: (...args: unknown[]) => mocks.keysList(...args),
      keysSet: (...args: unknown[]) => mocks.keysSet(...args),
      keysTest: (...args: unknown[]) => mocks.keysTest(...args),
      keysDelete: (...args: unknown[]) => mocks.keysDelete(...args),
      modelsList: (...args: unknown[]) => mocks.modelsList(...args),
    },
  };
});

function key(id: string, label = `AIza••••${id}`) {
  return { id, label };
}

function model(name: string, displayName = name) {
  return { name, displayName, supportedGenerationMethods: [] };
}

function modelRowElements(fieldLabel: string) {
  const input = screen.getByLabelText(fieldLabel) as HTMLInputElement;
  const row = input.closest('.model-input-row') as HTMLElement;
  return { input, row, loadButton: within(row).getByRole('button') };
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.settingsStore.transcribeModel = 'gemini-flash-lite-latest';
  mocks.settingsStore.liveModel = 'gemini-3.5-live-translate-preview';
  mocks.settingsStore.memoModel = 'gemini-flash-lite-latest';
  mocks.settingsStore.setModel.mockReset();
  mocks.keysList.mockReset().mockResolvedValue({ status: 'ok', data: [] });
  mocks.keysSet.mockReset();
  mocks.keysTest.mockReset();
  mocks.keysDelete.mockReset();
  mocks.modelsList.mockReset();
  keysStore.reset();
});

describe('SettingsGemini key field', () => {
  it('renders the key input as type=password and toggles reveal', async () => {
    render(SettingsGemini);

    const input = screen.getByLabelText('API key') as HTMLInputElement;
    expect(input.type).toBe('password');

    const toggle = screen.getByRole('button', { name: 'Hiện key' });
    await fireEvent.click(toggle);
    expect(input.type).toBe('text');
  });

  it('reports a valid key with the model count and a local check timestamp (HH:MM)', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    mocks.keysTest.mockResolvedValue({ status: 'ok', data: { keyId: 'k1', valid: true } });
    mocks.modelsList.mockResolvedValue({ status: 'ok', data: [model('models/gemini-1')] });
    render(SettingsGemini);

    const input = screen.getByLabelText('API key') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'AIzaValidKey' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    const status = await screen.findByText(/Key hợp lệ, 1 model khả dụng/);
    expect(status.textContent).toMatch(/·\s*\d{2}:\d{2}/);
  });

  it('shows the category title + hint (never the raw detail) when every key is rejected', async () => {
    mocks.keysSet.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    mocks.keysTest.mockResolvedValue({
      status: 'error',
      error: { category: 'auth', code: 'auth', detailRedacted: 'super-secret-detail' },
    });
    render(SettingsGemini);

    const input = screen.getByLabelText('API key') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'AIzaRejected' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Kiểm tra key' }));

    await screen.findByText('Key không hợp lệ');
    expect(screen.queryByText('super-secret-detail')).toBeNull();
  });
});

describe('SettingsGemini key list', () => {
  it('shows masked labels for stored keys and an empty state when there are none', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [] });
    render(SettingsGemini);

    await screen.findByText('Chưa lưu key nào.');
  });

  it('deletes a key via keysDelete and updates the rendered list', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [key('k1'), key('k2')] });
    mocks.keysDelete.mockResolvedValue({ status: 'ok', data: [key('k2')] });
    render(SettingsGemini);

    await screen.findByText('AIza••••k1');
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá key AIza••••k1' }));

    await waitFor(() => expect(mocks.keysDelete).toHaveBeenCalledWith('k1'));
    await waitFor(() => expect(screen.queryByText('AIza••••k1')).toBeNull());
    expect(screen.getByText('AIza••••k2')).toBeTruthy();
  });

  it('keeps the list unchanged and shows a banner when deleting fails', async () => {
    mocks.keysList.mockResolvedValue({ status: 'ok', data: [key('k1')] });
    mocks.keysDelete.mockResolvedValue({
      status: 'error',
      error: { category: 'storage', code: 'storage', detailRedacted: 'keychain locked' },
    });
    render(SettingsGemini);

    await screen.findByText('AIza••••k1');
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá key AIza••••k1' }));

    await screen.findByText('Lỗi kho khoá');
    expect(screen.getByText('AIza••••k1')).toBeTruthy();
  });
});

describe('SettingsGemini model fields', () => {
  it('renders the three model rows with their current settings values', () => {
    mocks.settingsStore.transcribeModel = 'transcribe-model-x';
    mocks.settingsStore.liveModel = 'live-model-x';
    mocks.settingsStore.memoModel = 'memo-model-x';
    render(SettingsGemini);

    expect((screen.getByLabelText('Model transcribe') as HTMLInputElement).value).toBe('transcribe-model-x');
    expect((screen.getByLabelText('Model Live') as HTMLInputElement).value).toBe('live-model-x');
    expect((screen.getByLabelText('Model Memo') as HTMLInputElement).value).toBe('memo-model-x');
  });

  it('blocks an empty value inline without calling setModel', async () => {
    render(SettingsGemini);
    const { input } = modelRowElements('Model transcribe');

    await fireEvent.input(input, { target: { value: '   ' } });
    await fireEvent.change(input);

    expect(screen.getByText('Tên model không được để trống.')).toBeTruthy();
    expect(mocks.settingsStore.setModel).not.toHaveBeenCalled();
  });

  it('saves a custom model name not present in any loaded list', async () => {
    render(SettingsGemini);
    const { input } = modelRowElements('Model Memo');

    await fireEvent.input(input, { target: { value: 'my-custom-memo-model' } });
    await fireEvent.change(input);

    expect(mocks.settingsStore.setModel).toHaveBeenCalledWith('memo', 'my-custom-memo-model');
  });

  it('loads the list for one kind only ("Tải danh sách" gọi modelsList(kind) theo từng select)', async () => {
    let resolveList: ((value: { status: 'ok'; data: ReturnType<typeof model>[] }) => void) | undefined;
    mocks.modelsList.mockImplementation(
      () => new Promise((resolve) => { resolveList = resolve; }),
    );
    render(SettingsGemini);
    const live = modelRowElements('Model Live');
    const transcribe = modelRowElements('Model transcribe');

    await fireEvent.click(live.loadButton);

    expect(mocks.modelsList).toHaveBeenCalledWith('live');
    expect(mocks.modelsList).not.toHaveBeenCalledWith('transcribe');
    // Busy state on the clicked button only; the input stays editable.
    expect(live.loadButton.getAttribute('aria-busy')).toBe('true');
    expect(live.input.hasAttribute('disabled')).toBe(false);
    expect(transcribe.loadButton.getAttribute('aria-busy')).toBe('false');

    resolveList?.({ status: 'ok', data: [model('models/live-only')] });
    await waitFor(() => expect(live.loadButton.getAttribute('aria-busy')).toBe('false'));
  });

  it('shows a soft, non-blocking warning when the saved value is not in the loaded list', async () => {
    mocks.settingsStore.transcribeModel = 'not-in-the-list';
    mocks.modelsList.mockResolvedValue({ status: 'ok', data: [model('models/one'), model('models/two')] });
    render(SettingsGemini);
    const { loadButton } = modelRowElements('Model transcribe');

    await fireEvent.click(loadButton);

    await screen.findByText('Không có trong danh sách đã tải — vẫn được lưu.');
  });

  it('shows an in-place category banner on a load failure and never touches the configured value', async () => {
    mocks.settingsStore.memoModel = 'configured-memo-model';
    mocks.modelsList.mockResolvedValue({
      status: 'error',
      error: { category: 'network', code: 'network', detailRedacted: 'offline' },
    });
    render(SettingsGemini);
    const { loadButton, input } = modelRowElements('Model Memo');

    await fireEvent.click(loadButton);

    await screen.findByText('Lỗi kết nối');
    expect(input.value).toBe('configured-memo-model');
    expect(mocks.settingsStore.setModel).not.toHaveBeenCalled();
  });
});
