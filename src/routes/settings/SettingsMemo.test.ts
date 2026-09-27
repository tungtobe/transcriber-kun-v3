// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import { memoTemplatesStore } from '../../lib/stores/memoTemplates.svelte';
import SettingsMemo from './SettingsMemo.svelte';

const mocks = vi.hoisted(() => ({
  memoTemplatesList: vi.fn(),
  memoTemplateCreate: vi.fn(),
  memoTemplateUpdate: vi.fn(),
  memoTemplateDelete: vi.fn(),
  memoTemplatesRestoreDefaults: vi.fn(),
}));

vi.mock('../../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      memoTemplatesList: (...args: unknown[]) => mocks.memoTemplatesList(...args),
      memoTemplateCreate: (...args: unknown[]) => mocks.memoTemplateCreate(...args),
      memoTemplateUpdate: (...args: unknown[]) => mocks.memoTemplateUpdate(...args),
      memoTemplateDelete: (...args: unknown[]) => mocks.memoTemplateDelete(...args),
      memoTemplatesRestoreDefaults: (...args: unknown[]) => mocks.memoTemplatesRestoreDefaults(...args),
    },
  };
});

function template(overrides: Partial<{
  id: string;
  name: string;
  prompt: string;
  isDefault: boolean;
  locale: string | null;
  defaultKey: string | null;
}> = {}) {
  return {
    id: 'default-1',
    name: 'Biên bản họp',
    prompt: 'Tóm tắt {transcript}',
    isDefault: true,
    locale: 'vi',
    defaultKey: 'meeting-minutes',
    ...overrides,
  };
}

function defaultRows() {
  return [
    template(),
    template({ id: 'default-2', name: 'Memo song ngữ Nhật–Việt', defaultKey: 'bilingual-ja-vi' }),
  ];
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  memoTemplatesStore.reset();
  mocks.memoTemplatesList.mockReset().mockResolvedValue({ status: 'ok', data: defaultRows() });
  mocks.memoTemplateCreate.mockReset();
  mocks.memoTemplateUpdate.mockReset();
  mocks.memoTemplateDelete.mockReset();
  mocks.memoTemplatesRestoreDefaults.mockReset();
});

describe('SettingsMemo', () => {
  it('loads and shows the two default templates with the locale badge, plus the editor for the first one', async () => {
    render(SettingsMemo);

    expect(await screen.findByText('Biên bản họp')).toBeTruthy();
    expect(screen.getByText('Memo song ngữ Nhật–Việt')).toBeTruthy();
    expect(screen.getAllByText('Mặc định · vi')).toHaveLength(2);
    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Biên bản họp');
    expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('Tóm tắt {transcript}');
  });

  it('selecting another row loads it into the editor', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    await fireEvent.click(screen.getByRole('button', { name: /Memo song ngữ Nhật–Việt/ }));

    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Memo song ngữ Nhật–Việt');
  });

  it('the transcript placeholder check goes red and Save disables when {transcript} is removed', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    const prompt = screen.getByLabelText('Prompt');
    await fireEvent.input(prompt, { target: { value: 'Chỉ có ghi chú {notes}' } });

    expect(screen.getByText('Thiếu {transcript} — bắt buộc phải có')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Lưu' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('saves an edited default template', async () => {
    mocks.memoTemplateUpdate.mockResolvedValue({
      status: 'ok',
      data: { ...defaultRows()[0], name: 'Tên đã sửa' },
    });
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Tên đã sửa' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Lưu' }));

    expect(mocks.memoTemplateUpdate).toHaveBeenCalledWith('default-1', 'Tên đã sửa', 'Tóm tắt {transcript}');
    expect(await screen.findByText('Tên đã sửa')).toBeTruthy();
  });

  it('a default template shows delete as aria-disabled and clicking it deletes nothing', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    const deleteControl = screen.getByRole('button', { name: 'Xoá mẫu' });
    expect(deleteControl.getAttribute('aria-disabled')).toBe('true');

    await fireEvent.click(deleteControl);

    expect(mocks.memoTemplateDelete).not.toHaveBeenCalled();
    expect(screen.queryByText('Xoá mẫu này? Không thể hoàn tác.')).toBeNull();
  });

  it('"+ Thêm mẫu" opens a fresh draft, and saving it selects the new row', async () => {
    const created = template({ id: 'user-1', isDefault: false, locale: null, defaultKey: null, name: 'Của tôi' });
    mocks.memoTemplateCreate.mockResolvedValue({ status: 'ok', data: created });
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    await fireEvent.click(screen.getByRole('button', { name: '+ Thêm mẫu' }));
    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Mẫu mới');

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Của tôi' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Lưu' }));

    expect(mocks.memoTemplateCreate).toHaveBeenCalledWith('Của tôi', expect.stringContaining('{transcript}'));
    expect(await screen.findByText('Của tôi')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xoá mẫu' })).toBeTruthy();
  });

  it('deleting a user template asks for an inline confirmation (no dialog) before calling the command', async () => {
    const userRow = template({ id: 'user-1', isDefault: false, locale: null, defaultKey: null, name: 'Của tôi' });
    mocks.memoTemplatesList.mockResolvedValue({ status: 'ok', data: [...defaultRows(), userRow] });
    mocks.memoTemplateDelete.mockResolvedValue({ status: 'ok', data: null });
    render(SettingsMemo);
    await screen.findByText('Của tôi');

    await fireEvent.click(screen.getByRole('button', { name: /Của tôi/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá mẫu' }));

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByText('Xoá mẫu này? Không thể hoàn tác.')).toBeTruthy();
    expect(mocks.memoTemplateDelete).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Xác nhận xoá' }));

    expect(mocks.memoTemplateDelete).toHaveBeenCalledWith('user-1');
    await screen.findByText('Biên bản họp');
    expect(screen.queryByText('Của tôi')).toBeNull();
  });

  it('switching the selected template while the editor has unsaved changes asks "Bỏ thay đổi?" first', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Đang gõ dở' } });
    await fireEvent.click(screen.getByRole('button', { name: /Memo song ngữ Nhật–Việt/ }));

    expect(screen.getByText('Bỏ thay đổi chưa lưu?')).toBeTruthy();
    // Vẫn hiện mẫu đang sửa, chưa chuyển.
    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Đang gõ dở');

    await fireEvent.click(screen.getByRole('button', { name: 'Bỏ thay đổi' }));

    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Memo song ngữ Nhật–Việt');
  });

  it('"Tiếp tục sửa" cancels the discard and keeps editing the current template', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Đang gõ dở' } });
    await fireEvent.click(screen.getByRole('button', { name: /Memo song ngữ Nhật–Việt/ }));
    await fireEvent.click(screen.getByRole('button', { name: 'Tiếp tục sửa' }));

    expect(screen.queryByText('Bỏ thay đổi chưa lưu?')).toBeNull();
    expect((screen.getByLabelText('Tên mẫu') as HTMLInputElement).value).toBe('Đang gõ dở');
  });

  it('restoring defaults asks for an inline confirmation and then resets the edited default in the still-open editor', async () => {
    // Trả nguyên bản gốc (tên chưa sửa) -- đúng những gì Rust trả về sau khi
    // ghi lại tên/prompt gốc (spec I/O Matrix "Khôi phục").
    mocks.memoTemplatesRestoreDefaults.mockResolvedValue({ status: 'ok', data: defaultRows() });
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');

    // Sửa dở tên mặc định đang mở nhưng chưa lưu.
    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Đang sửa dở' } });

    await fireEvent.click(screen.getByRole('button', { name: 'Khôi phục mẫu mặc định' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.memoTemplatesRestoreDefaults).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Xác nhận khôi phục' }));

    expect(mocks.memoTemplatesRestoreDefaults).toHaveBeenCalledWith('vi');
    // Editor (vẫn mở đúng mẫu mặc định đó) hiện lại tên gốc, bỏ bản đang sửa dở.
    const nameInput = (await screen.findByLabelText('Tên mẫu')) as HTMLInputElement;
    expect(nameInput.value).toBe('Biên bản họp');
  });

  it('reloads the list when the UI locale changes, without duplicating defaults', async () => {
    render(SettingsMemo);
    await screen.findByText('Biên bản họp');
    expect(mocks.memoTemplatesList).toHaveBeenCalledWith('vi');

    mocks.memoTemplatesList.mockResolvedValueOnce({
      status: 'ok',
      data: [template({ id: 'ja-1', name: '議事録', locale: 'ja' }), template({ id: 'ja-2', name: '日越バイリンガルメモ', locale: 'ja', defaultKey: 'bilingual-ja-vi' })],
    });
    i18n.applyPreference('ja');

    expect(await screen.findByText('議事録')).toBeTruthy();
    expect(mocks.memoTemplatesList).toHaveBeenCalledWith('ja');
    expect(screen.queryByText('Biên bản họp')).toBeNull();
  });
});
