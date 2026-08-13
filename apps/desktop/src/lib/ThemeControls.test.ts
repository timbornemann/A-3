import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import ThemeControls from './ThemeControls.svelte';

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe('ThemeControls', () => {
  it('offers named native controls and exposes the selected mode beyond color', async () => {
    render(ThemeControls);

    const dark = screen.getByRole('button', { name: 'Dunkel' });
    await fireEvent.click(dark);

    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(dark.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'System' }).getAttribute('aria-pressed')).toBe(
      'false',
    );

    await fireEvent.click(screen.getByRole('button', { name: 'System' }));
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });
});
