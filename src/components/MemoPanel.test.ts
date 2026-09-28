// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '../i18n/index.svelte';
import MemoPanel from './MemoPanel.svelte';
import type { MemoEntryState } from '../lib/stores/memo.svelte';
import type { AppError, MemoTemplate, MemoView } from '../lib/bindings';

const mocks = vi.hoisted(() => ({
  memoStore: {
    view: vi.fn(),
    load: vi.fn(),
    generate: vi.fn(),
    cancel: vi.fn(),
    setVisible: vi.fn(),
  },
  memoTemplatesStore: {
    templates: [] as MemoTemplate[],
    load: vi.fn(),
  },
  keysStore: {
    status: 'ready' as 'ready' | 'idle' | 'loading' | 'error',
    hasUsableKey: true,
    load: vi.fn(),
  },
  notesStore: {
    flush: vi.fn(),
  },
  settingsStore: {
    consentStatus: 'current' as 'pending' | 'declined' | 'stale' | 'current',
  },
  commands: {
    memoExport: vi.fn(),
    openExternalUrl: vi.fn(),
  },
}));

vi.mock('../lib/stores/memo.svelte', () => ({ memoStore: mocks.memoStore }));
vi.mock('../lib/stores/memoTemplates.svelte', () => ({ memoTemplatesStore: mocks.memoTemplatesStore }));
vi.mock('../lib/stores/keys.svelte', () => ({ keysStore: mocks.keysStore }));
vi.mock('../lib/stores/notes.svelte', () => ({ notesStore: mocks.notesStore }));
vi.mock('../lib/stores/settings.svelte', () => ({ settingsStore: mocks.settingsStore }));
vi.mock('../lib/bindings', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/bindings')>();
  return { ...actual, commands: mocks.commands };
});

const SESSION_ID = 'session-1';
const TEMPLATE_ID = 'template-1';
const TRANSCRIPT_ID = 'transcript-1';

function template(): MemoTemplate {
  return {
    id: TEMPLATE_ID,
    name: 'Biên bản họp',
    prompt: '{transcript}',
    isDefault: true,
    locale: 'vi',
    defaultKey: 'meeting-minutes',
  };
}

function memoView(overrides: Partial<MemoView> = {}): MemoView {
  return {
    body: '# Memo\n\nNội dung',
    createdAt: Date.UTC(2026, 0, 15, 3, 4),
    transcriptStatus: 'complete',
    model: 'gemini-flash-lite-latest',
    templateName: 'Biên bản họp',
    usesNotes: false,
    fromPreviousTranscript: false,
    notesChanged: false,
    ...overrides,
  };
}

function entry(overrides: Partial<MemoEntryState> = {}): MemoEntryState {
  return { status: 'idle', memo: null, error: null, ...overrides };
}

const quotaError: AppError = { category: 'quota', code: 'quota', detailRedacted: 'x' };

function baseProps(overrides: Record<string, unknown> = {}) {
  return {
    sessionId: SESSION_ID,
    transcriptId: TRANSCRIPT_ID,
    sourceVariant: 'primary' as const,
    active: true,
    hasTranscript: true,
    segmentTextCount: 5,
    ...overrides,
  };
}

afterEach(() => cleanup());

beforeEach(() => {
  i18n.applyPreference('vi');
  mocks.memoStore.view.mockReset().mockReturnValue(entry());
  mocks.memoStore.load.mockReset().mockResolvedValue(undefined);
  mocks.memoStore.generate.mockReset().mockResolvedValue(undefined);
  mocks.memoStore.cancel.mockReset().mockResolvedValue(undefined);
  mocks.memoStore.setVisible.mockReset();
  mocks.memoTemplatesStore.templates = [template()];
  mocks.memoTemplatesStore.load.mockReset().mockResolvedValue(undefined);
  mocks.keysStore.status = 'ready';
  mocks.keysStore.hasUsableKey = true;
  mocks.keysStore.load.mockReset().mockResolvedValue(undefined);
  mocks.notesStore.flush.mockReset().mockResolvedValue(true);
  mocks.settingsStore.consentStatus = 'current';
  mocks.commands.memoExport.mockReset();
  mocks.commands.openExternalUrl.mockReset();
});

describe('MemoPanel disabled Sinh button', () => {
  it('disabled with a consent reason when consent is not current', async () => {
    mocks.settingsStore.consentStatus = 'pending';
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(screen.getByRole('tooltip').textContent).toContain('Cần đồng ý dùng Gemini trước');

    await fireEvent.click(screen.getByRole('button', { name: /Sinh/ }));
    expect(mocks.memoStore.generate).not.toHaveBeenCalled();
  });

  it('disabled with a key reason when there is no usable key', async () => {
    mocks.keysStore.hasUsableKey = false;
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(screen.getByRole('tooltip').textContent).toContain('Chưa có API key dùng được');

    await fireEvent.click(screen.getByRole('button', { name: /Sinh/ }));
    expect(mocks.memoStore.generate).not.toHaveBeenCalled();
  });

  it('disabled with a no-transcript reason when the session has no primary transcript', async () => {
    render(MemoPanel, baseProps({ hasTranscript: false }));
    await Promise.resolve();

    expect(screen.getByRole('tooltip').textContent).toContain('Phiên chưa có transcript');

    await fireEvent.click(screen.getByRole('button', { name: /Sinh/ }));
    expect(mocks.memoStore.generate).not.toHaveBeenCalled();
  });

  it('disabled with an only-gaps reason when the transcript has zero text segments', async () => {
    render(MemoPanel, baseProps({ segmentTextCount: 0 }));
    await Promise.resolve();

    expect(screen.getByRole('tooltip').textContent).toContain('Transcript không có đoạn text nào');

    await fireEvent.click(screen.getByRole('button', { name: /Sinh/ }));
    expect(mocks.memoStore.generate).not.toHaveBeenCalled();
  });

  it('is enabled (no tooltip) when every gate passes', async () => {
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(screen.queryByRole('tooltip')).toBeNull();
    expect(screen.getByRole('button', { name: 'Sinh' })).toBeTruthy();
  });
});

describe('MemoPanel cached memo', () => {
  it('shows the cached memo on open without calling memoStore.generate, with a source line', async () => {
    mocks.memoStore.view.mockReturnValue(entry({ status: 'ready', memo: memoView() }));
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(mocks.memoStore.load).toHaveBeenCalledWith(SESSION_ID, TEMPLATE_ID, TRANSCRIPT_ID);
    expect(mocks.memoStore.generate).not.toHaveBeenCalled();
    expect(screen.getByText(/bản gốc/)).toBeTruthy();
  });

  it('shows the "Memo sinh từ bản trước" label when fromPreviousTranscript is true', async () => {
    mocks.memoStore.view.mockReturnValue(
      entry({ status: 'ready', memo: memoView({ fromPreviousTranscript: true }) }),
    );
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(screen.getByText('Memo sinh từ bản trước')).toBeTruthy();
    expect(screen.getByText(/bản transcript trước/)).toBeTruthy();
  });

  it('does not show the stale label when fromPreviousTranscript is false', async () => {
    mocks.memoStore.view.mockReturnValue(entry({ status: 'ready', memo: memoView() }));
    render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(screen.queryByText('Memo sinh từ bản trước')).toBeNull();
  });
});

describe('MemoPanel XSS sanitization', () => {
  it('does not render a script tag or an inline event handler from the memo body', async () => {
    mocks.memoStore.view.mockReturnValue(
      entry({
        status: 'ready',
        memo: memoView({
          body: 'hello <script>alert(1)</script> <img src="x" onerror="alert(2)"> world',
        }),
      }),
    );
    const { container } = render(MemoPanel, baseProps());
    await Promise.resolve();

    expect(container.querySelector('script')).toBeNull();
    expect(container.innerHTML).not.toContain('onerror');
    expect(container.innerHTML).not.toContain('alert(1)');
    expect(container.innerHTML).not.toContain('alert(2)');
  });
});

describe('MemoPanel inline categorized error', () => {
  it('keeps the old memo visible, shows the quota error, and Thử lại retries generate', async () => {
    mocks.memoStore.view.mockReturnValue(
      entry({ status: 'ready', memo: memoView({ body: '# Bản cũ' }), error: quotaError }),
    );
    render(MemoPanel, baseProps());
    await Promise.resolve();

    const alert = screen.getByRole('alert');
    expect(alert.textContent).toContain('Hết hạn mức');
    expect(screen.getByText('Bản cũ')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Thử lại' }));
    await Promise.resolve();

    expect(mocks.notesStore.flush).toHaveBeenCalledWith(SESSION_ID);
    expect(mocks.memoStore.generate).toHaveBeenCalledWith(SESSION_ID, TEMPLATE_ID, 'vi', TRANSCRIPT_ID);
  });
});

describe('MemoPanel link interception', () => {
  it('clicking an http(s) link in the body calls openExternalUrl instead of navigating', async () => {
    mocks.memoStore.view.mockReturnValue(
      entry({
        status: 'ready',
        memo: memoView({ body: '[site](https://example.com/path)' }),
      }),
    );
    const { container } = render(MemoPanel, baseProps());
    await Promise.resolve();

    const anchor = container.querySelector('a') as HTMLAnchorElement;
    expect(anchor).toBeTruthy();
    expect(anchor.getAttribute('href')).toBe('https://example.com/path');

    const event = await fireEvent.click(anchor);

    expect(mocks.commands.openExternalUrl).toHaveBeenCalledWith('https://example.com/path');
    // `fireEvent` returns `false` when `preventDefault()` was called -- proof
    // the click never falls through to a real navigation.
    expect(event).toBe(false);
  });

  it('clicking a javascript: link never calls openExternalUrl', async () => {
    mocks.memoStore.view.mockReturnValue(
      entry({
        status: 'ready',
        memo: memoView({ body: '<a href="javascript:alert(1)">bad</a>' }),
      }),
    );
    const { container } = render(MemoPanel, baseProps());
    await Promise.resolve();

    const anchor = container.querySelector('a');
    if (anchor) {
      await fireEvent.click(anchor);
    }

    expect(mocks.commands.openExternalUrl).not.toHaveBeenCalled();
  });
});
