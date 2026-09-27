// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../../i18n/index.svelte';
import TemplateEditor from './TemplateEditor.svelte';

const mocks = vi.hoisted(() => ({
  memoTemplateCreate: vi.fn(),
  memoTemplateUpdate: vi.fn(),
  memoTemplateDelete: vi.fn(),
}));

vi.mock('../../../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/bindings')>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      memoTemplateCreate: (...args: unknown[]) => mocks.memoTemplateCreate(...args),
      memoTemplateUpdate: (...args: unknown[]) => mocks.memoTemplateUpdate(...args),
      memoTemplateDelete: (...args: unknown[]) => mocks.memoTemplateDelete(...args),
    },
  };
});

function template() {
  return {
    id: 'user-1',
    name: 'Mẫu của tôi',
    prompt: 'Tóm tắt {transcript}',
    isDefault: false,
    locale: null as string | null,
    defaultKey: null as string | null,
  };
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.memoTemplateCreate.mockReset();
  mocks.memoTemplateUpdate.mockReset();
  mocks.memoTemplateDelete.mockReset();
});

describe('TemplateEditor', () => {
  it('an empty name blocks Save with an inline error', async () => {
    render(TemplateEditor, {
      props: {
        template: template(),
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: '   ' } });

    expect(screen.getByText('Tên mẫu không được để trống')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Lưu' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('a name over 100 scalars blocks Save with an inline error', async () => {
    render(TemplateEditor, {
      props: {
        template: template(),
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'a'.repeat(101) } });

    expect(screen.getByText('Tên mẫu tối đa 100 ký tự')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Lưu' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('a prompt over 20,000 scalars blocks Save with an inline error', async () => {
    render(TemplateEditor, {
      props: {
        template: template(),
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    await fireEvent.input(screen.getByLabelText('Prompt'), {
      target: { value: `{transcript}${'a'.repeat(20_000)}` },
    });

    expect(screen.getByText('Prompt tối đa 20 000 ký tự')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Lưu' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows the {notes} check as satisfied once {notes} is typed', async () => {
    render(TemplateEditor, {
      props: {
        template: template(),
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    expect(screen.getByText('Không có {notes} (tuỳ chọn)')).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('Prompt'), {
      target: { value: 'Tóm tắt {transcript} và {notes}' },
    });

    expect(screen.getByText('Có {notes}')).toBeTruthy();
  });

  it('reports dirty as soon as the buffer diverges from the loaded template, and clean again after a matching value', async () => {
    const onDirtyChange = vi.fn();
    render(TemplateEditor, {
      props: {
        template: template(),
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange,
      },
    });
    onDirtyChange.mockClear();

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Đã sửa' } });
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Mẫu của tôi' } });
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it('calls memoTemplateUpdate on save and reports the saved template', async () => {
    const saved = { ...template(), name: 'Tên mới' };
    mocks.memoTemplateUpdate.mockResolvedValue({ status: 'ok', data: saved });
    const onSaved = vi.fn();
    render(TemplateEditor, {
      props: { template: template(), draftSeed: null, onSaved, onDeleted: vi.fn(), onDirtyChange: vi.fn() },
    });

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Tên mới' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Lưu' }));

    expect(mocks.memoTemplateUpdate).toHaveBeenCalledWith('user-1', 'Tên mới', 'Tóm tắt {transcript}');
    expect(onSaved).toHaveBeenCalledWith(saved);
  });

  it('a draft (template null) calls memoTemplateCreate instead of update', async () => {
    const created = { ...template(), id: 'user-2', name: 'Bản nháp' };
    mocks.memoTemplateCreate.mockResolvedValue({ status: 'ok', data: created });
    const onSaved = vi.fn();
    render(TemplateEditor, {
      props: {
        template: null,
        draftSeed: { name: 'Bản nháp', prompt: '{transcript}' },
        onSaved,
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Lưu' }));

    expect(mocks.memoTemplateCreate).toHaveBeenCalledWith('Bản nháp', '{transcript}');
    expect(onSaved).toHaveBeenCalledWith(created);
  });

  it('surfaces a save error inline without calling onSaved', async () => {
    mocks.memoTemplateUpdate.mockResolvedValue({
      status: 'error',
      error: { category: 'model', code: 'request', detailRedacted: 'boom' },
    });
    const onSaved = vi.fn();
    render(TemplateEditor, {
      props: { template: template(), draftSeed: null, onSaved, onDeleted: vi.fn(), onDirtyChange: vi.fn() },
    });

    await fireEvent.input(screen.getByLabelText('Tên mẫu'), { target: { value: 'Tên mới' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Lưu' }));

    expect(await screen.findByText('Sự cố mô hình')).toBeTruthy();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('delete requires an inline confirmation before calling memoTemplateDelete, with no modal role', async () => {
    mocks.memoTemplateDelete.mockResolvedValue({ status: 'ok', data: null });
    const onDeleted = vi.fn();
    render(TemplateEditor, {
      props: { template: template(), draftSeed: null, onSaved: vi.fn(), onDeleted, onDirtyChange: vi.fn() },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá mẫu' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(mocks.memoTemplateDelete).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: 'Xác nhận xoá' }));

    expect(mocks.memoTemplateDelete).toHaveBeenCalledWith('user-1');
    expect(onDeleted).toHaveBeenCalledWith('user-1');
  });

  it('"Huỷ" on the delete confirmation cancels without calling the command', async () => {
    render(TemplateEditor, {
      props: { template: template(), draftSeed: null, onSaved: vi.fn(), onDeleted: vi.fn(), onDirtyChange: vi.fn() },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Xoá mẫu' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Huỷ' }));

    expect(screen.queryByText('Xoá mẫu này? Không thể hoàn tác.')).toBeNull();
    expect(mocks.memoTemplateDelete).not.toHaveBeenCalled();
  });

  it('a default template renders delete as aria-disabled with an explanatory hint, and never asks for delete confirmation', async () => {
    render(TemplateEditor, {
      props: {
        template: { ...template(), id: 'default-1', isDefault: true, locale: 'vi', defaultKey: 'meeting-minutes' },
        draftSeed: null,
        onSaved: vi.fn(),
        onDeleted: vi.fn(),
        onDirtyChange: vi.fn(),
      },
    });

    const deleteControl = screen.getByRole('button', { name: 'Xoá mẫu' });
    expect(deleteControl.getAttribute('aria-disabled')).toBe('true');

    await fireEvent.click(deleteControl);

    expect(mocks.memoTemplateDelete).not.toHaveBeenCalled();
    expect(screen.queryByText('Xoá mẫu này? Không thể hoàn tác.')).toBeNull();
  });
});
