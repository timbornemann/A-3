import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import GlobalStatusBar from './GlobalStatusBar.svelte';

const ready = { tone: 'ready' as const, value: 'Bereit' };

describe('GlobalStatusBar', () => {
  it('opens the Index inspector from a native keyboard-operable button', async () => {
    const onIndexClick = vi.fn();
    render(GlobalStatusBar, {
      project: ready,
      index: { tone: 'pending', value: 'Quellcode lesen 3/6' },
      model: ready,
      run: ready,
      onIndexClick,
    });

    const trigger = screen.getByRole('button', { name: 'Quellcode lesen 3/6' });
    expect(trigger.getAttribute('aria-haspopup')).toBe('dialog');
    trigger.focus();
    await fireEvent.keyDown(trigger, { key: 'Enter' });
    await fireEvent.click(trigger);
    expect(onIndexClick).toHaveBeenCalled();
  });
});
