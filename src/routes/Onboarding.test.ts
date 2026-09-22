// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render } from '@testing-library/svelte';
import Onboarding from './Onboarding.svelte';

afterEach(() => cleanup());

describe('Onboarding', () => {
  it('marks only the current setup step for assistive technology', () => {
    const { container } = render(Onboarding);
    const currentSteps = container.querySelectorAll('[aria-current="step"]');

    expect(currentSteps).toHaveLength(1);
    expect(currentSteps[0]?.textContent).toContain('Ngôn ngữ');
  });
});
