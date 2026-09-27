// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import SessionSearch from './SessionSearch.svelte';

const mocks = vi.hoisted(() => ({
  setNameQuery: vi.fn(),
  clearNameQuery: vi.fn(),
  nameQuery: '',
}));

vi.mock('../../lib/stores/library.svelte', () => ({
  libraryStore: {
    setNameQuery: (...args: unknown[]) => mocks.setNameQuery(...args),
    clearNameQuery: (...args: unknown[]) => mocks.clearNameQuery(...args),
    get nameQuery() {
      return mocks.nameQuery;
    },
  },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.setNameQuery.mockReset();
  mocks.clearNameQuery.mockReset();
  mocks.nameQuery = '';
});

describe('SessionSearch', () => {
  it('forwards typed input to setNameQuery() on every keystroke (no Enter needed)', async () => {
    render(SessionSearch);
    const input = screen.getByRole('textbox', { name: 'Tìm theo tên' });

    await fireEvent.input(input, { target: { value: 'h' } });
    await fireEvent.input(input, { target: { value: 'họ' } });
    expect(mocks.setNameQuery).toHaveBeenNthCalledWith(1, 'h');
    expect(mocks.setNameQuery).toHaveBeenNthCalledWith(2, 'họ');
  });

  it('does not show the × clear button when the query is empty', () => {
    mocks.nameQuery = '';
    render(SessionSearch);
    expect(screen.queryByRole('button', { name: 'Xoá tìm kiếm' })).toBeNull();
  });

  it('shows the × clear button when there is a query, and clicking it clears + refocuses the input', async () => {
    mocks.nameQuery = 'họp';
    render(SessionSearch);
    const input = screen.getByRole('textbox', { name: 'Tìm theo tên' }) as HTMLInputElement;
    const clearButton = screen.getByRole('button', { name: 'Xoá tìm kiếm' });

    await fireEvent.click(clearButton);
    expect(mocks.clearNameQuery).toHaveBeenCalledTimes(1);
    expect(document.activeElement).toBe(input);
  });

  it('Escape clears the query and keeps focus when there is a query', () => {
    mocks.nameQuery = 'họp';
    render(SessionSearch);
    const input = screen.getByRole('textbox', { name: 'Tìm theo tên' }) as HTMLInputElement;

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(mocks.clearNameQuery).toHaveBeenCalledTimes(1);
  });

  it('Escape does nothing when the query is already empty', () => {
    mocks.nameQuery = '';
    render(SessionSearch);
    const input = screen.getByRole('textbox', { name: 'Tìm theo tên' });

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(mocks.clearNameQuery).not.toHaveBeenCalled();
  });

  it('exposes focusAndSelect() for the ⌘F/Ctrl+F shortcut to call', () => {
    const { component } = render(SessionSearch);
    const input = screen.getByRole('textbox', { name: 'Tìm theo tên' }) as HTMLInputElement;

    input.blur();
    expect(document.activeElement).not.toBe(input);
    component.focusAndSelect();
    expect(document.activeElement).toBe(input);
  });
});
