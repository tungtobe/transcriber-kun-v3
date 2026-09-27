// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import TagPicker from './TagPicker.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

const tags = [
  { id: 't1', name: 'KH-ABC', sessionCount: 12 },
  { id: 't2', name: 'sprint-12', sessionCount: 6 },
  { id: 't3', name: 'sales', sessionCount: 3 },
];

function baseProps(overrides: Record<string, unknown> = {}) {
  return {
    mode: 'filter' as const,
    dialogLabel: 'Chọn tag để lọc',
    tags,
    selectedIds: [],
    onToggle: vi.fn().mockResolvedValue({ status: 'ok' }),
    onCreate: vi.fn(),
    onDeleteTag: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
}

describe('TagPicker', () => {
  it('shows every tag under "Tất cả" sorted as given (by session count)', () => {
    render(TagPicker, baseProps());
    const names = screen.getAllByRole('checkbox').map((el) => el.closest('label')?.textContent);
    expect(names?.[0]).toContain('KH-ABC');
    expect(names?.[1]).toContain('sprint-12');
    expect(names?.[2]).toContain('sales');
  });

  it('pins already-selected tags under "Đang lọc" in filter mode, checked', () => {
    render(TagPicker, baseProps({ selectedIds: ['t2'] }));
    expect(screen.getByText('Đang lọc · 1')).toBeTruthy();
    const checkbox = screen.getByRole('checkbox', { name: /sprint-12/ }) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });

  it('uses "Đã chọn" instead of "Đang lọc" in assign mode', () => {
    render(TagPicker, baseProps({ mode: 'assign', selectedIds: ['t1'] }));
    expect(screen.getByText('Đã chọn · 1')).toBeTruthy();
    expect(screen.queryByText(/Đang lọc/)).toBeNull();
  });

  it('shows "Gõ để tạo tag đầu tiên" when there are no tags at all', () => {
    render(TagPicker, baseProps({ tags: [] }));
    expect(screen.getByText('Gõ để tạo tag đầu tiên')).toBeTruthy();
  });

  it('clicking an unchecked tag calls onToggle with its id', async () => {
    const onToggle = vi.fn().mockResolvedValue({ status: 'ok' });
    render(TagPicker, baseProps({ onToggle }));
    await fireEvent.click(screen.getByRole('checkbox', { name: /sales/ }));
    expect(onToggle).toHaveBeenCalledWith('t3');
  });

  it('shows an inline error and does not close when onToggle fails (e.g. 20-tag limit)', async () => {
    const onToggle = vi.fn().mockResolvedValue({ status: 'error', message: 'Phiên đã có tối đa 20 tag' });
    const onClose = vi.fn();
    render(TagPicker, baseProps({ onToggle, onClose }));
    await fireEvent.click(screen.getByRole('checkbox', { name: /sales/ }));
    expect(screen.getByRole('alert').textContent).toContain('Phiên đã có tối đa 20 tag');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('typing a brand-new name shows a create action; Enter creates and selects it', async () => {
    const onCreate = vi.fn().mockResolvedValue({ status: 'ok', id: 'new', name: 'demo' });
    const onToggle = vi.fn().mockResolvedValue({ status: 'ok' });
    render(TagPicker, baseProps({ onCreate, onToggle }));

    const input = screen.getByRole('searchbox', { name: 'Tìm tag' });
    await fireEvent.input(input, { target: { value: 'demo' } });
    expect(screen.getByText('Tạo tag “demo”')).toBeTruthy();

    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCreate).toHaveBeenCalledWith('demo');
    await Promise.resolve();
    await Promise.resolve();
    expect(onToggle).toHaveBeenCalledWith('new');
  });

  it('typing a name that matches an existing tag exactly (case-insensitive) selects it instead of creating', async () => {
    const onCreate = vi.fn();
    const onToggle = vi.fn().mockResolvedValue({ status: 'ok' });
    render(TagPicker, baseProps({ onCreate, onToggle }));

    const input = screen.getByRole('searchbox', { name: 'Tìm tag' });
    await fireEvent.input(input, { target: { value: 'sales' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onCreate).not.toHaveBeenCalled();
    expect(onToggle).toHaveBeenCalledWith('t3');
  });

  it('in assign mode at 20 selected tags, disables unselected checkboxes and create, with an explanation', async () => {
    const onToggle = vi.fn().mockResolvedValue({ status: 'ok' });
    const manyTags = Array.from({ length: 21 }, (_, i) => ({ id: `t${i}`, name: `tag${i}`, sessionCount: 0 }));
    render(
      TagPicker,
      baseProps({
        mode: 'assign',
        tags: manyTags,
        selectedIds: manyTags.slice(0, 20).map((t) => t.id),
        onToggle,
      }),
    );

    expect(screen.getByText('Phiên đã có tối đa 20 tag')).toBeTruthy();
    const unselectedCheckbox = screen.getByRole('checkbox', { name: /tag20/ }) as HTMLInputElement;
    expect(unselectedCheckbox.disabled).toBe(true);
    // Vẫn gỡ được một tag đã chọn (không khoá chiều ngược lại).
    const selectedCheckbox = screen.getByRole('checkbox', { name: /tag0/ }) as HTMLInputElement;
    expect(selectedCheckbox.disabled).toBe(false);
  });

  it('Escape closes the picker', async () => {
    const onClose = vi.fn();
    render(TagPicker, baseProps({ onClose }));
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clicking outside the picker closes it', async () => {
    const onClose = vi.fn();
    render(TagPicker, baseProps({ onClose }));
    await fireEvent.pointerDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('opens "Quản lý tag" and lists every tag with a delete button', async () => {
    render(TagPicker, baseProps());
    await fireEvent.click(screen.getByRole('button', { name: 'Quản lý tag' }));

    expect(screen.getByText('Quản lý tag')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xoá tag KH-ABC' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Xoá tag sprint-12' })).toBeTruthy();
  });

  it('deleting a tag from manage opens a confirm dialog and calls onDeleteTag on confirm', async () => {
    const onDeleteTag = vi.fn().mockResolvedValue({ status: 'ok' });
    render(TagPicker, baseProps({ onDeleteTag }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quản lý tag' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá tag sales' }));

    expect(screen.getByRole('alertdialog')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá tag' }));

    expect(onDeleteTag).toHaveBeenCalledWith('t3');
  });

  it('opening the delete confirm dialog does not close the picker on a click inside it (portalled outside root)', async () => {
    const onClose = vi.fn();
    render(TagPicker, baseProps({ onClose }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quản lý tag' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Xoá tag sales' }));

    const dialog = screen.getByRole('alertdialog');
    await fireEvent.pointerDown(dialog);

    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('alertdialog')).toBeTruthy();
  });

  it('back button in manage view returns to the search/list view', async () => {
    render(TagPicker, baseProps());
    await fireEvent.click(screen.getByRole('button', { name: 'Quản lý tag' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Quay lại chọn tag' }));

    expect(screen.getByRole('searchbox', { name: 'Tìm tag' })).toBeTruthy();
  });
});
