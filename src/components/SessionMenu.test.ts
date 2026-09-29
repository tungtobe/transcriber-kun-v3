// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import SessionMenu from './SessionMenu.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

describe('SessionMenu', () => {
  it('the trigger has an accessible label and the menu is closed initially', () => {
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn() });
    expect(screen.getByRole('button', { name: 'Thao tác khác' })).toBeTruthy();
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('clicking the trigger opens the menu with Đổi tên and Xoá', async () => {
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    expect(screen.getByRole('menu')).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Đổi tên' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Xoá' })).toBeTruthy();
  });

  it('choosing Đổi tên calls onRename and closes the menu', async () => {
    const onRename = vi.fn();
    render(SessionMenu, { onRename, onDelete: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Đổi tên' }));

    expect(onRename).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('choosing Xoá calls onDelete and closes the menu', async () => {
    const onDelete = vi.fn();
    render(SessionMenu, { onRename: vi.fn(), onDelete });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Xoá' }));

    expect(onDelete).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('shows and invokes the optional Download Recording action', async () => {
    const onDownloadRecording = vi.fn();
    render(SessionMenu, {
      onRename: vi.fn(),
      onDelete: vi.fn(),
      onDownloadRecording,
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    const item = screen.getByRole('menuitem', { name: 'Tải Recording' });
    expect(item).toBeTruthy();
    await fireEvent.click(item);
    expect(onDownloadRecording).toHaveBeenCalledTimes(1);
  });

  it('disables recording export with the supplied active-session reason', async () => {
    render(SessionMenu, {
      onRename: vi.fn(),
      onDelete: vi.fn(),
      onDownloadRecording: vi.fn(),
      downloadDisabledReason: 'Chỉ tải sau khi phiên đã lưu xong.',
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    const item = screen.getByRole('menuitem', { name: /Tải Recording/ }) as HTMLButtonElement;
    expect(item.disabled).toBe(true);
    expect(item.textContent).toContain('Chỉ tải sau khi phiên đã lưu xong.');
  });

  it('Escape closes the menu and returns focus to the trigger', async () => {
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn() });
    const trigger = screen.getByRole('button', { name: 'Thao tác khác' });
    await fireEvent.click(trigger);

    const renameItem = screen.getByRole('menuitem', { name: 'Đổi tên' });
    await fireEvent.keyDown(renameItem, { key: 'Escape' });

    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('does not show Gắn tag when onTag is not provided', async () => {
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));
    expect(screen.queryByRole('menuitem', { name: 'Gắn tag' })).toBeNull();
  });

  it('shows Gắn tag first when onTag is provided, and choosing it calls onTag and closes the menu', async () => {
    const onTag = vi.fn();
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn(), onTag });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    const items = screen.getAllByRole('menuitem');
    expect(items.map((el) => el.textContent?.trim())).toEqual(['Gắn tag', 'Đổi tên', 'Xoá']);

    await fireEvent.click(screen.getByRole('menuitem', { name: 'Gắn tag' }));
    expect(onTag).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('ArrowDown/ArrowUp move focus between items', async () => {
    render(SessionMenu, { onRename: vi.fn(), onDelete: vi.fn() });
    await fireEvent.click(screen.getByRole('button', { name: 'Thao tác khác' }));

    const renameItem = screen.getByRole('menuitem', { name: 'Đổi tên' });
    const deleteItem = screen.getByRole('menuitem', { name: 'Xoá' });
    expect(document.activeElement).toBe(renameItem);

    await fireEvent.keyDown(renameItem, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(deleteItem);

    await fireEvent.keyDown(deleteItem, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(renameItem);

    await fireEvent.keyDown(renameItem, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(deleteItem);
  });
});
