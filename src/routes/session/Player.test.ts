// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../../i18n/index.svelte';
import Player from './Player.svelte';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));

vi.mock('../../lib/stores/settings.svelte', () => ({
  settingsStore: { timestampOffsetSec: 0 },
}));

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  HTMLMediaElement.prototype.play = vi.fn().mockResolvedValue(undefined);
  HTMLMediaElement.prototype.pause = vi.fn();
});

describe('Player', () => {
  it('shows "no audio" with a relink button when proxyPath is null, and calls onRelinkRequest', async () => {
    const onRelinkRequest = vi.fn();
    render(Player, {
      proxyPath: null,
      onRelinkRequest,
      relinking: false,
      relinkMessage: null,
    });

    expect(screen.getByText('Không có audio · Chọn lại file nguồn')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Chọn lại file nguồn' }));
    expect(onRelinkRequest).toHaveBeenCalledTimes(1);
    expect(document.querySelector('audio')).toBeNull();
  });

  it('falls back to the same "no audio" UI when the <audio> element fires an error event', async () => {
    render(Player, {
      proxyPath: '/data/media/s1/proxy.flac',
      onRelinkRequest: vi.fn(),
      relinking: false,
      relinkMessage: null,
    });

    expect(screen.queryByText('Không có audio · Chọn lại file nguồn')).toBeNull();
    const audio = document.querySelector('audio')!;
    await fireEvent.error(audio);

    expect(screen.getByText('Không có audio · Chọn lại file nguồn')).toBeTruthy();
  });

  it('never autoplays — <audio preload="metadata"> with no autoplay attribute', () => {
    render(Player, {
      proxyPath: '/data/media/s1/proxy.flac',
      onRelinkRequest: vi.fn(),
      relinking: false,
      relinkMessage: null,
    });

    const audio = document.querySelector('audio')!;
    expect(audio.getAttribute('preload')).toBe('metadata');
    expect(audio.hasAttribute('autoplay')).toBe(false);
    expect(audio.getAttribute('src')).toBe('asset://localhost//data/media/s1/proxy.flac');
  });

  it('toggling play calls audio.play(), and the play event flips the button label to Pause', async () => {
    render(Player, {
      proxyPath: '/data/media/s1/proxy.flac',
      onRelinkRequest: vi.fn(),
      relinking: false,
      relinkMessage: null,
    });

    const toggle = screen.getByRole('button', { name: 'Phát' });
    await fireEvent.click(toggle);
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalled();

    const audio = document.querySelector('audio')!;
    await fireEvent.play(audio);
    expect(screen.getByRole('button', { name: 'Tạm dừng' })).toBeTruthy();

    await fireEvent.pause(audio);
    expect(screen.getByRole('button', { name: 'Phát' })).toBeTruthy();
  });

  it('the seek slider has aria-valuetext as a formatted timestamp and ArrowRight/ArrowLeft seek ±5s', async () => {
    render(Player, {
      proxyPath: '/data/media/s1/proxy.flac',
      onRelinkRequest: vi.fn(),
      relinking: false,
      relinkMessage: null,
    });

    const audio = document.querySelector('audio')! as HTMLAudioElement;
    audio.currentTime = 10;
    await fireEvent.timeUpdate(audio);

    const slider = screen.getByRole('slider', { name: 'Tua' });
    expect(slider.getAttribute('aria-valuetext')).toBe('00:10');

    await fireEvent.keyDown(slider, { key: 'ArrowRight' });
    expect(audio.currentTime).toBe(15);

    await fireEvent.keyDown(slider, { key: 'ArrowLeft' });
    await fireEvent.keyDown(slider, { key: 'ArrowLeft' });
    await fireEvent.keyDown(slider, { key: 'ArrowLeft' });
    // Clamped at 0, never negative.
    expect(audio.currentTime).toBe(0);
  });

  it('changing volume and playback speed writes straight to the <audio> element', async () => {
    render(Player, {
      proxyPath: '/data/media/s1/proxy.flac',
      onRelinkRequest: vi.fn(),
      relinking: false,
      relinkMessage: null,
    });

    const audio = document.querySelector('audio')! as HTMLAudioElement;
    const volume = screen.getByLabelText('Âm lượng') as HTMLInputElement;
    await fireEvent.input(volume, { target: { value: '0.4' } });
    expect(audio.volume).toBeCloseTo(0.4);

    const speed = screen.getByLabelText('Tốc độ phát') as HTMLSelectElement;
    await fireEvent.change(speed, { target: { value: '1.5' } });
    expect(audio.playbackRate).toBe(1.5);
  });

  it('shows the relink error message and disables the button while relinking', () => {
    render(Player, {
      proxyPath: null,
      onRelinkRequest: vi.fn(),
      relinking: true,
      relinkMessage: 'File này không khớp với bản ghi gốc.',
    });

    expect((screen.getByRole('button', { name: /Đang chọn/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole('alert')).toHaveProperty('textContent', 'File này không khớp với bản ghi gốc.');
  });
});
