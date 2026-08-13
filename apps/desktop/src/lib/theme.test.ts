import { describe, expect, it } from 'vitest';
import { applyThemePreference, THEME_PREFERENCES } from './theme';

describe('theme preference', () => {
  it('exposes only system, light, and dark without creating a second durable settings store', () => {
    expect(THEME_PREFERENCES).toEqual(['system', 'light', 'dark']);
  });

  it('applies an explicit theme and returns system choice to the CSS media query', () => {
    const root = document.createElement('html');

    applyThemePreference('dark', root);
    expect(root.dataset.theme).toBe('dark');

    applyThemePreference('light', root);
    expect(root.dataset.theme).toBe('light');

    applyThemePreference('system', root);
    expect(root.hasAttribute('data-theme')).toBe(false);
  });
});
