import { render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import App from './App.svelte';

describe('U9 accessibility contract', () => {
  it('keeps the model-free shell structurally named and natively keyboard reachable', async () => {
    const { container } = render(App, {
      props: {
        healthLoader: async () => ({
          applicationVersion: '0.1.0',
          platform: 'windows',
          protocolVersion: 1,
          status: 'ready',
        }),
        projectStatusLoader: async () => ({
          protocolVersion: 1,
          result: { status: 'noProject' },
        }),
      },
    });

    await waitFor(() =>
      expect(screen.getByRole('heading', { level: 2, name: 'Deine Projekte' })).toBeTruthy(),
    );

    expect(container.querySelectorAll('main')).toHaveLength(1);
    expect(screen.getByRole('heading', { level: 1, name: 'A^3' })).toBeTruthy();
    expect(
      screen.getByRole('link', { name: 'Zum Arbeitsbereich springen' }).getAttribute('href'),
    ).toBe('#workspace-content');
    expect(container.querySelector('.workspace-menu')).toBeNull();
    expect(container.querySelector('.sidebar-footer')).toBeNull();
    expect(container.querySelector('.app-sidebar fieldset')).toBeNull();

    assertUniqueIds(container);
    assertSequentialHeadings(container);
    assertNoPositiveTabIndex(container);
    assertNamedInteractiveElements(container);
  });
});

function assertUniqueIds(container: HTMLElement): void {
  const ids = [...container.querySelectorAll<HTMLElement>('[id]')].map((element) => element.id);
  expect(new Set(ids).size).toBe(ids.length);
}

function assertSequentialHeadings(container: HTMLElement): void {
  const levels = [...container.querySelectorAll('h1, h2, h3, h4, h5, h6')].map((heading) =>
    Number.parseInt(heading.tagName.slice(1), 10),
  );
  expect(levels[0]).toBe(1);
  for (let index = 1; index < levels.length; index += 1) {
    expect(
      levels[index]! - levels[index - 1]!,
      `heading ${index + 1} skips a level`,
    ).toBeLessThanOrEqual(1);
  }
}

function assertNoPositiveTabIndex(container: HTMLElement): void {
  for (const element of container.querySelectorAll<HTMLElement>('[tabindex]')) {
    expect(Number.parseInt(element.getAttribute('tabindex') ?? '0', 10)).toBeLessThanOrEqual(0);
  }
}

function assertNamedInteractiveElements(container: HTMLElement): void {
  const interactive = container.querySelectorAll<HTMLElement>(
    'a[href], button, input:not([type="hidden"]), select, textarea, summary',
  );
  expect(interactive.length).toBeGreaterThan(5);
  for (const element of interactive) {
    const name = accessibleName(element);
    expect(name.length, `${element.tagName.toLowerCase()} has no accessible name`).toBeGreaterThan(
      0,
    );
  }
}

function accessibleName(element: HTMLElement): string {
  const ariaLabel = element.getAttribute('aria-label')?.trim();
  if (ariaLabel !== undefined && ariaLabel !== '') return ariaLabel;
  const labelledBy = element.getAttribute('aria-labelledby');
  if (labelledBy !== null) {
    const label = labelledBy
      .split(/\s+/u)
      .map((id) => document.getElementById(id)?.textContent?.trim() ?? '')
      .join(' ')
      .trim();
    if (label !== '') return label;
  }
  if (element instanceof HTMLInputElement || element instanceof HTMLSelectElement) {
    const label = element.labels?.[0]?.textContent?.trim();
    if (label !== undefined && label !== '') return label;
  }
  return element.textContent?.trim() ?? '';
}
