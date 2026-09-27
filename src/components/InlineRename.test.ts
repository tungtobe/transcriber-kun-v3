// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import InlineRename from './InlineRename.svelte';

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
});

function props(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    value: 'cuộc họp',
    label: 'Tên phiên',
    onSave: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
}

describe('InlineRename', () => {
  it('renders an input preselected to the current value and focused', () => {
    render(InlineRename, props());
    const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
    expect(input.value).toBe('cuộc họp');
    expect(document.activeElement).toBe(input);
    expect(input.maxLength).toBe(200);
  });

  it('Enter with a valid trimmed title calls onSave with the trimmed value', async () => {
    const onSave = vi.fn();
    render(InlineRename, props({ onSave }));
    const input = screen.getByRole('textbox', { name: 'Tên phiên' });
    await fireEvent.input(input, { target: { value: '  Họp sprint 12  ' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onSave).toHaveBeenCalledWith('Họp sprint 12');
  });

  it('an empty or whitespace-only title is rejected inline without calling onSave', async () => {
    const onSave = vi.fn();
    render(InlineRename, props({ onSave }));
    const input = screen.getByRole('textbox', { name: 'Tên phiên' });
    await fireEvent.input(input, { target: { value: '   ' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('Tên không được để trống')).toBeTruthy();
  });

  it('a title over 200 unicode scalars is rejected inline without calling onSave', async () => {
    const onSave = vi.fn();
    render(InlineRename, props({ onSave }));
    const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
    // Bypass the `maxlength` HTML attribute (which the browser itself would
    // enforce) to prove the JS-side scalar check also rejects it — mirrors
    // Rust's own defense-in-depth validation (spec I/O Matrix "Rust cũng trả
    // Request").
    input.removeAttribute('maxlength');
    const tooLong = 'x'.repeat(201);
    await fireEvent.input(input, { target: { value: tooLong } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('Tên tối đa 200 ký tự')).toBeTruthy();
  });

  it('Escape calls onCancel without calling onSave', async () => {
    const onSave = vi.fn();
    const onCancel = vi.fn();
    render(InlineRename, props({ onSave, onCancel }));
    const input = screen.getByRole('textbox', { name: 'Tên phiên' });
    await fireEvent.keyDown(input, { key: 'Escape' });

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSave).not.toHaveBeenCalled();
  });

  it('shows a server error alongside local validation state and disables the input while saving', () => {
    render(InlineRename, props({ saving: true, serverError: 'Không đổi tên được. Thử lại sau.' }));
    const input = screen.getByRole('textbox', { name: 'Tên phiên' }) as HTMLInputElement;
    expect(input.disabled).toBe(true);
    expect(screen.getByText('Không đổi tên được. Thử lại sau.')).toBeTruthy();
  });
});
