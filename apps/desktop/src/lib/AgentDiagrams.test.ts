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
  it('keeps the SVG when only historical freshness changes', async () => {
    mermaidMocks.render.mockResolvedValue({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>Stabil</text></svg>',
    });
    let stale = false;
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        kind: 'available' as const,
        artifact: { mermaid: 'flowchart TD\n n0["A"]', summary: { ...summary, stale } },
      },
    }));
    const view = render(AgentDiagrams, {
      artifactLoader,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });
    await screen.findByRole('img', { name: 'Ablauf' });
    const svg = view.container.querySelector('svg');
    stale = true;
    await view.rerender({ summaries: [{ ...summary, stale }] });
    await waitFor(() => expect(artifactLoader).toHaveBeenCalledTimes(2));
    expect(view.container.querySelector('svg')).toBe(svg);
    expect(mermaidMocks.render).toHaveBeenCalledOnce();
  });
  it('uses different Mermaid DOM identities for separate conversation turns', async () => {
    mermaidMocks.render.mockImplementation(async (renderId: string) => {
      // Mermaid removes an existing node with this ID before preparing its canvas.
      document.getElementById(renderId)?.remove();
      return {
        svg: `<svg xmlns="http://www.w3.org/2000/svg" id="${renderId}"><text>Bleibt sichtbar</text></svg>`,
      };
    });
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        kind: 'available' as const,
        artifact: { mermaid: 'flowchart TD\n  n0["A"]\n', summary },
      },
    }));
    const first = render(AgentDiagrams, {
      artifactLoader,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });
    await waitFor(() =>
      expect(first.container.querySelector('.diagram-canvas svg')).not.toBeNull(),
    );
    const svg = first.container.querySelector('svg');
    const second = render(AgentDiagrams, {
      artifactLoader,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '3',
    });
    await waitFor(() =>
      expect(second.container.querySelector('.diagram-canvas svg')).not.toBeNull(),
    );
    expect(first.container.querySelector('svg')).toBe(svg);
    expect(new Set(mermaidMocks.render.mock.calls.map(([renderId]) => renderId)).size).toBe(2);
  });
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

  it('offers a new evidence-bound generation after a Mermaid syntax failure', async () => {
    mermaidMocks.render.mockRejectedValue(new Error('Parse error on line 3'));
    const onregenerate = vi.fn(async () => {});
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        artifact: { mermaid: 'flowchart TD\n  n0["A"]\n', summary },
        kind: 'available' as const,
      },
    }));

    render(AgentDiagrams, {
      artifactLoader,
      onregenerate,
      refreshKey: '1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByRole('button', { name: 'Diagramm neu erzeugen' }));
    expect(onregenerate).toHaveBeenCalledWith(summary);
  });

  it('keeps a completed diagram mounted during unrelated conversation polling', async () => {
    mermaidMocks.render.mockResolvedValue({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>Stabil</text></svg>',
    });
    const artifactLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        artifact: { mermaid: 'flowchart TD\n  n0["A"]\n', summary },
        kind: 'available' as const,
      },
    }));
    const view = render(AgentDiagrams, {
      artifactLoader,
      refreshKey: 'conversation-poll-1',
      sessionId,
      summaries: [summary],
      userSequence: '1',
    });

    await screen.findByRole('img', { name: 'Ablauf' });
    const mountedSection = view.container.querySelector('.diagram-section');
    const mountedCanvas = view.container.querySelector('.diagram-canvas');

    await view.rerender({
      artifactLoader,
      refreshKey: 'conversation-poll-2',
      sessionId,
      summaries: [{ ...summary }],
      userSequence: '1',
    });
    await Promise.resolve();

    expect(view.container.querySelector('.diagram-section')).toBe(mountedSection);
    expect(view.container.querySelector('.diagram-canvas')).toBe(mountedCanvas);
    expect(artifactLoader).toHaveBeenCalledTimes(1);
    expect(mermaidMocks.render).toHaveBeenCalledTimes(1);
  });
});
