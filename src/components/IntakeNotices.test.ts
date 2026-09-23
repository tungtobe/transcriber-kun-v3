// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import { configureRouter } from '../lib/router';
import IntakeNotices from './IntakeNotices.svelte';

const mocks = vi.hoisted(() => ({
  intakeStore: {
    notices: [] as Array<{
      id: string;
      variant: 'info' | 'warning' | 'danger';
      title: string;
      message: string;
      actionLabel?: string;
      actionHref?: string;
    }>,
    dismiss: vi.fn(),
  },
}));

vi.mock('../lib/stores/intake.svelte', () => ({ intakeStore: mocks.intakeStore }));

afterEach(() => cleanup());

beforeEach(() => {
  configureRouter();
  i18n.applyPreference('vi');
  mocks.intakeStore.notices = [];
  mocks.intakeStore.dismiss.mockReset();
});

describe('IntakeNotices', () => {
  it('renders nothing when there are no notices', () => {
    const { container } = render(IntakeNotices);
    expect(container.querySelector('.intake-notices')).toBeNull();
  });

  it('renders one row per notice with title, message, and an optional action link', () => {
    mocks.intakeStore.notices = [
      { id: 'n1', variant: 'info', title: 'a.mp4', message: 'Mở lại phiên có sẵn — không tốn token' },
      {
        id: 'n2',
        variant: 'warning',
        title: 'b.mp4',
        message: 'Thêm Google API key để bật tính năng transcribe và Live.',
        actionLabel: 'Nhập key',
        actionHref: '/settings/gemini',
      },
    ];
    render(IntakeNotices);

    expect(screen.getByText('a.mp4')).toBeTruthy();
    expect(screen.getByText('Mở lại phiên có sẵn — không tốn token')).toBeTruthy();
    const action = screen.getByRole('link', { name: 'Nhập key' });
    expect(action.getAttribute('href')).toBe('/settings/gemini');
  });

  it('dismisses exactly the clicked notice', async () => {
    mocks.intakeStore.notices = [
      { id: 'n1', variant: 'danger', title: 'a.wmv', message: 'Định dạng không hỗ trợ.' },
      { id: 'n2', variant: 'danger', title: 'b.wmv', message: 'Định dạng không hỗ trợ.' },
    ];
    render(IntakeNotices);

    const dismissButtons = screen.getAllByRole('button', { name: 'Đóng thông báo' });
    expect(dismissButtons).toHaveLength(2);
    await fireEvent.click(dismissButtons[0]);

    expect(mocks.intakeStore.dismiss).toHaveBeenCalledWith('n1');
    expect(mocks.intakeStore.dismiss).toHaveBeenCalledTimes(1);
  });

  it('renders every variant with its status role', () => {
    mocks.intakeStore.notices = [
      { id: 'n1', variant: 'info', title: 'a', message: 'info msg' },
      { id: 'n2', variant: 'warning', title: 'b', message: 'warning msg' },
      { id: 'n3', variant: 'danger', title: 'c', message: 'danger msg' },
    ];
    render(IntakeNotices);

    expect(screen.getAllByRole('status')).toHaveLength(3);
  });
});
