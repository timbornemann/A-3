import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AgentDiagrams from './AgentDiagrams.svelte';

const mermaidMocks = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn(),
}));

vi.mock('mermaid', () => ({ default: mermaidMocks }));

const sessionId = 'a'.repeat(64);
const artifactRef = 'b'.repeat(128);
const summary = {
  artifactRef,
  description: 'Belegter Ablauf',
  kind: 'flowchart' as const,
  stale: false,
  title: 'Ablauf',
  userSequence: '1',
};

describe('AgentDiagrams', () => {
  beforeEach(() => {
    mermaidMocks.initialize.mockReset();
    mermaidMocks.render.mockReset();
  });

  it('renders lazily and removes active SVG content before inserting it', async () => {
    mermaidMocks.render.mockResolvedValue({
      svg: `<svg xmlns="http://www.w3.org/2000/svg">
        <script>bad()</script><animate attributeName="x"/><image href="https://example.com/x"/>
        <style>@import url(https://example.com/theme.css); .unsafe { fill: red; }</style>
        <path onload="bad()" style="fill:url(https://example.com/x)"/>
        <path id="safe" style="fill:url(#local)"/>
      </svg>`,
    });
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        artifact: { mermaid: 'flowchart TD\n  n0["A"]\n', summary },
        kind: 'available' as const,
      },
    }));

    const { container } = render(AgentDiagrams, {
      artifactLoader,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });

    expect(await screen.findByRole('img', { name: 'Ablauf' })).toBeTruthy();
    expect(mermaidMocks.initialize).toHaveBeenCalledWith(
      expect.objectContaining({
        htmlLabels: false,
        securityLevel: 'strict',
        startOnLoad: false,
      }),
    );
    expect(container.querySelector('script, animate, image')).toBeNull();
    expect(container.querySelector('path[onload]')).toBeNull();
    expect(container.querySelector('path[style*="example.com"]')).toBeNull();
    expect(container.innerHTML).not.toContain('@import');
    expect(container.querySelector('#safe')?.getAttribute('style')).toContain('url(#local)');
  });

  it('keeps a local render failure recoverable', async () => {
    mermaidMocks.render
      .mockRejectedValueOnce(new Error('invalid'))
      .mockResolvedValueOnce({ svg: '<svg xmlns="http://www.w3.org/2000/svg"></svg>' });
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        artifact: { mermaid: 'flowchart TD\n  n0["A"]\n', summary },
        kind: 'available' as const,
      },
    }));

    render(AgentDiagrams, {
      artifactLoader,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Erneut rendern' }));
    await waitFor(() => expect(screen.getByRole('img', { name: 'Ablauf' })).toBeTruthy());
    expect(mermaidMocks.render).toHaveBeenCalledTimes(2);
  });
});
