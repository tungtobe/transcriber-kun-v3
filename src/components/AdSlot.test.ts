// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import AdSlot from './AdSlot.svelte';
import { i18n } from '../i18n/index.svelte';

const mocks = vi.hoisted(() => ({
  commands: {
    adsNext: vi.fn(),
    adsImpression: vi.fn(),
    adsClick: vi.fn(),
    adsReport: vi.fn(),
  },
}));

vi.mock('../lib/bindings', () => ({ commands: mocks.commands }));

const creative = {
  token: 'ad-token-1',
  sponsoredLabel: 'Tài trợ',
  sponsor: 'Transcriber Kun',
  title: 'Tập trung vào cuộc trò chuyện',
  body: 'Chép lời và sắp xếp cuộc họp.',
  imageDataUrl: 'data:image/png;base64,ZmFrZQ==',
  width: 300,
  height: 100,
  whyThisAd: 'Quảng cáo được chọn theo ngôn ngữ giao diện. Không dùng dữ liệu cá nhân.',
};

class MockIntersectionObserver {
  static instances: MockIntersectionObserver[] = [];
  private target: Element | null = null;

  constructor(private readonly callback: IntersectionObserverCallback) {
    MockIntersectionObserver.instances.push(this);
  }

  observe(target: Element): void {
    this.target = target;
  }

  disconnect = vi.fn();

  intersect(ratio = 1): void {
    if (!this.target) throw new Error('No element is being observed');
    this.callback([{
      target: this.target,
      isIntersecting: ratio > 0,
      intersectionRatio: ratio,
    } as IntersectionObserverEntry], this as unknown as IntersectionObserver);
  }
}

beforeEach(() => {
  i18n.applyPreference('vi');
  MockIntersectionObserver.instances = [];
  mocks.commands.adsNext.mockReset().mockResolvedValue({ status: 'ok', data: creative });
  mocks.commands.adsImpression.mockReset().mockResolvedValue({ status: 'ok', data: 'recorded' });
  mocks.commands.adsClick.mockReset().mockResolvedValue({ status: 'ok', data: 'opened' });
  mocks.commands.adsReport.mockReset().mockResolvedValue({ status: 'ok', data: 'opened' });
  vi.stubGlobal('IntersectionObserver', MockIntersectionObserver);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 236,
    bottom: 220,
    width: 236,
    height: 220,
    toJSON: () => ({}),
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('AdSlot', () => {
  it('loads a safe creative, keeps the fixed footprint, and counts only after visible once', async () => {
    render(AdSlot, { props: { eligible: true } });

    const loading = screen.getByRole('status', { name: 'Thông tin được tài trợ' });
    expect(screen.getByText('Đang tải thông tin được tài trợ…')).toBeTruthy();
    expect(loading.classList.contains('ad-loading')).toBe(true);
    expect(loading.classList.contains('ad-slot')).toBe(true);
    expect(mocks.commands.adsNext).toHaveBeenCalledWith('vi');

    expect(await screen.findByText('Tập trung vào cuộc trò chuyện')).toBeTruthy();
    expect(document.querySelector('.ad-slot')?.classList.contains('ad-loading')).toBe(false);
    expect(MockIntersectionObserver.instances).toHaveLength(1);

    const observer = MockIntersectionObserver.instances[0];
    observer.intersect(0.4);
    expect(mocks.commands.adsImpression).not.toHaveBeenCalled();
    observer.intersect(0.75);
    await waitFor(() => expect(mocks.commands.adsImpression).toHaveBeenCalledOnce());
    expect(mocks.commands.adsImpression).toHaveBeenCalledWith('ad-token-1');
    observer.intersect(1);
    expect(mocks.commands.adsImpression).toHaveBeenCalledOnce();
  });

  it('removes the slot and its footprint when ads are disabled or suppressed', async () => {
    mocks.commands.adsNext.mockResolvedValue({ status: 'ok', data: null });
    render(AdSlot, { props: { eligible: true } });

    await waitFor(() => expect(document.querySelector('.ad-slot')).toBeNull());
    expect(screen.queryByText('Tài trợ')).toBeNull();
    expect(mocks.commands.adsImpression).not.toHaveBeenCalled();
  });

  it('uses the embedded safe fallback with a stable image placeholder when offline', async () => {
    mocks.commands.adsNext.mockResolvedValue({ status: 'ok', data: {
      ...creative,
      imageDataUrl: null,
      title: 'Tập trung vào cuộc trò chuyện',
    } });
    render(AdSlot, { props: { eligible: true } });

    expect(await screen.findByText('Transcriber Kun', { selector: '.image-fallback' })).toBeTruthy();
    expect(document.querySelector('.image-frame')?.getAttribute('style')).toContain('300 / 100');
    expect(screen.getByText(creative.whyThisAd).hasAttribute('hidden')).toBe(true);
    expect(screen.queryByRole('button', { name: /Mở Tập trung vào cuộc trò chuyện từ Transcriber Kun/ })).toBeNull();
    expect(screen.getByRole('button', { name: 'Báo cáo quảng cáo' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Vì sao tôi thấy quảng cáo này?' })).toBeTruthy();
  });

  it('routes click and report through typed IPC and exposes the localized explanation', async () => {
    render(AdSlot, { props: { eligible: true } });

    const creativeButton = await screen.findByRole('button', { name: /Mở Tập trung vào cuộc trò chuyện từ Transcriber Kun/ });
    expect(creativeButton.getAttribute('aria-describedby')).toBe('ad-creative-description');
    await fireEvent.click(creativeButton);
    await fireEvent.click(screen.getByRole('button', { name: 'Báo cáo quảng cáo' }));
    expect(mocks.commands.adsClick).toHaveBeenCalledWith('ad-token-1');
    expect(mocks.commands.adsReport).toHaveBeenCalledWith('ad-token-1');
    expect(await screen.findByText('Đã mở trang báo cáo trong trình duyệt.')).toBeTruthy();

    const whyButton = screen.getByRole('button', { name: 'Vì sao tôi thấy quảng cáo này?' });
    whyButton.focus();
    expect(document.activeElement).toBe(whyButton);
    await fireEvent.click(whyButton);
    expect(whyButton.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText(creative.whyThisAd)).toBeTruthy();
  });

  it.each([
    ['en', 'Sponsored message', 'Why this ad?', 'Report ad'],
    ['ja', 'スポンサー広告', 'この広告が表示される理由', '広告を報告'],
  ] as const)('localizes the slot controls in %s', async (locale, label, why, report) => {
    i18n.applyPreference(locale);
    render(AdSlot, { props: { eligible: true } });

    expect(await screen.findByRole('region', { name: label })).toBeTruthy();
    expect(screen.getByRole('button', { name: why })).toBeTruthy();
    expect(screen.getByRole('button', { name: report })).toBeTruthy();
    expect(mocks.commands.adsNext).toHaveBeenCalledWith(locale);
  });

  it('does not request an ad when the slot is ineligible', async () => {
    render(AdSlot, { props: { eligible: false } });
    await Promise.resolve();
    expect(mocks.commands.adsNext).not.toHaveBeenCalled();
    expect(document.querySelector('.ad-slot')).toBeNull();
  });
});
