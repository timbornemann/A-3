import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import AgentAskResearch from './AgentAskResearch.svelte';

const id = (digit: string): string => digit.repeat(64);

describe('AgentAskResearch', () => {
  it('keeps the understandable trace and citations under a completed answer', async () => {
    const previewLoader = vi.fn(async () => ({
      protocolVersion: 1 as const,
      result: {
        preview: {
          highlight: { endColumn: 8, endLine: 201, startColumn: 3, startLine: 201 },
          language: 'rust' as const,
          lineCount: 1,
          pathDisplay: 'src/late.rs',
          startLine: 201,
          text: '// TODO\n',
          truncatedAfter: false,
          truncatedBefore: true,
        },
        status: 'available' as const,
      },
    }));
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          detail: {
            citedSourceCount: 1,
            sourceCount: 1,
            stale: false,
            steps: [
              {
                action: 'Aktuelle indexierte Dateien nach konkretem Text durchsuchen',
                completeness: 'limited' as const,
                occurredAtUnixMillis: '100',
                phase: 'searchingSource' as const,
                query: 'TODO',
                state: 'running' as const,
              },
              {
                action: 'Antwort und verwendete Quellen veröffentlicht',
                completeness: 'notApplicable' as const,
                occurredAtUnixMillis: '101',
                phase: 'completed' as const,
                query: null,
                state: 'completed' as const,
              },
            ],
            userSequence: '1',
          },
          status: 'available' as const,
        },
      })),
      previewLoader,
      refreshKey: '1',
      sessionId: id('1'),
      sourcesLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: {
          nextCursor: null,
          sources: [
            {
              endLine: 201,
              kind: 'symbol' as const,
              path: 'src/late.rs',
              reason: 'sourceText' as const,
              sourceRef: id('2'),
              startLine: 201,
              symbol: null,
              usedForAnswer: true,
            },
          ],
          status: 'available' as const,
        },
      })),
      userSequence: '1',
    });

    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText('Gefundene und verwendete Quellen')).toBeTruthy();
    expect(await screen.findByText(/Für Antwort verwendet/)).toBeTruthy();
    expect(screen.getByText(/feste Sicherheits- oder Ressourcengrenze/)).toBeTruthy();
    expect(screen.queryByText(/provider|token|snapshot/i)).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: /src\/late\.rs:201/ }));
    expect(await screen.findByText('// TODO')).toBeTruthy();
    expect(previewLoader).toHaveBeenCalledWith(id('1'), '1', id('2'));
  });

  it('explains traces that predate V30', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { status: 'notRecorded' as const },
      })),
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });
    await fireEvent.click(await screen.findByText('Recherche & Quellen'));
    expect(await screen.findByText(/noch nicht aufgezeichnet/)).toBeTruthy();
  });

  it('treats a not-yet-created live trace as preparation instead of old history', async () => {
    render(AgentAskResearch, {
      detailLoader: vi.fn(async () => ({
        protocolVersion: 1 as const,
        result: { status: 'notRecorded' as const },
      })),
      live: true,
      refreshKey: '1',
      sessionId: id('1'),
      userSequence: '1',
    });

    expect(await screen.findByText(/Rechercheweg wird vorbereitet/)).toBeTruthy();
    expect(screen.queryByText(/älteren Antwort/)).toBeNull();
  });
});
