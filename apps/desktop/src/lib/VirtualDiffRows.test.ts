import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { AgentDiffRowV1 } from './agent-inspection';
import VirtualDiffRows from './VirtualDiffRows.svelte';

function rows(count: number): AgentDiffRowV1[] {
  return Array.from({ length: count }, (_, index) => ({
    afterLine: index + 1,
    beforeLine: index + 1,
    kind: 'context' as const,
    line: {
      ending: 'lf' as const,
      text: `line-${index + 1}`,
    },
  }));
}

describe('VirtualDiffRows', () => {
  it('keeps a 128,000-row diff at a constant DOM size with absolute ARIA positions', async () => {
    const largeDiff = rows(128_000);
    const { container } = render(VirtualDiffRows, {
      props: { layout: 'unified', rows: largeDiff },
    });
    const table = screen.getByRole('table', { name: 'Unified Diff' });

    expect(table.getAttribute('aria-rowcount')).toBe('128000');
    expect(screen.getAllByRole('row').length).toBeLessThanOrEqual(20);
    expect(screen.getAllByRole('row')[0]?.getAttribute('aria-rowindex')).toBe('1');
    expect(screen.getByText('line-1')).toBeTruthy();

    table.scrollTop = 28 * 64_000;
    await fireEvent.scroll(table);

    const renderedRows = screen.getAllByRole('row');
    expect(renderedRows.length).toBeLessThanOrEqual(20);
    expect(Number(renderedRows[0]?.getAttribute('aria-rowindex'))).toBeGreaterThan(63_990);
    expect(container.textContent).not.toContain('line-1');
  });

  it('uses the same bounded window for the side-by-side presentation', () => {
    render(VirtualDiffRows, {
      props: { layout: 'sideBySide', overscan: 2, rows: rows(1_000), viewportRows: 8 },
    });

    expect(screen.getByRole('table', { name: /Side-by-side Diff/u })).toBeTruthy();
    expect(screen.getAllByRole('row')).toHaveLength(12);
    expect(screen.getAllByRole('cell')).toHaveLength(24);
  });
});
