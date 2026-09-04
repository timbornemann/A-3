import { describe, expect, it, vi } from 'vitest';
import {
  queryAgentAskResearchDetail,
  queryAgentAskResearchSourcePreview,
  queryAgentAskResearchSources,
  queryAgentWorkTraceProjection,
  queryAgentWorkTraceSourcesV2,
} from './agent-ask-research';

const id = (digit: string): string => digit.repeat(64);

describe('Ask research V1 client', () => {
  it('submits only session, user sequence, opaque cursor, and opaque source capability', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: { status: 'notRecorded' },
      })
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: { nextCursor: null, sources: [], status: 'available' },
      })
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: { status: 'stale' },
      });

    await queryAgentAskResearchDetail(id('1'), '7', invoke);
    await queryAgentAskResearchSources(id('1'), '7', id('2'), invoke);
    await queryAgentAskResearchSourcePreview(id('1'), '7', id('3'), invoke);

    expect(invoke.mock.calls).toEqual([
      [
        'query_agent_work_trace_detail_v2',
        { request: { protocolVersion: 1, sessionId: id('1'), userSequence: '7' } },
      ],
      [
        'query_agent_work_trace_sources',
        {
          request: {
            cursor: id('2'),
            protocolVersion: 1,
            sessionId: id('1'),
            userSequence: '7',
          },
        },
      ],
      [
        'query_agent_work_trace_source_preview',
        {
          request: {
            protocolVersion: 1,
            sessionId: id('1'),
            sourceRef: id('3'),
            userSequence: '7',
          },
        },
      ],
    ]);
  });

  it('rejects unknown fields and unbounded result pages', async () => {
    const unknown = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: {
        detail: {
          citedSourceCount: 0,
          provider: 'hidden',
          sourceCount: 0,
          stale: false,
          steps: [],
          userSequence: '1',
        },
        status: 'available',
      },
    });
    await expect(queryAgentAskResearchDetail(id('1'), '1', unknown)).rejects.toThrow();

    const tooManySources = vi.fn().mockResolvedValue({
      protocolVersion: 1,
      result: {
        nextCursor: null,
        sources: Array.from({ length: 51 }, () => ({})),
        status: 'available',
      },
    });
    await expect(
      queryAgentAskResearchSources(id('1'), '1', null, tooManySources),
    ).rejects.toThrow();
  });

  it('binds coherent source paging to the opaque projection reference', async () => {
    const source = {
      endLine: 20,
      kind: 'symbol',
      path: 'taskflow/storage/base.py',
      reason: 'sourceText',
      referenceLabel: 'S1',
      sourceRef: id('3'),
      startLine: 18,
      symbol: 'save_tasks',
      usedForAnswer: true,
    };
    const detail = {
      citedSourceCount: 1,
      depth: 'standard',
      legacy: false,
      mode: 'ask',
      sourceCount: 1,
      stale: false,
      steps: [],
      userSequence: '7',
    };
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: {
          detail,
          nextCursor: id('4'),
          projectionRef: id('2'),
          sources: [source],
          status: 'available',
        },
      })
      .mockResolvedValueOnce({
        protocolVersion: 1,
        result: { nextCursor: null, sources: [], status: 'available' },
      });

    const projection = await queryAgentWorkTraceProjection(id('1'), '7', invoke);
    const page = await queryAgentWorkTraceSourcesV2(id('1'), '7', id('2'), id('4'), invoke);

    expect(projection.result.status).toBe('available');
    expect(page.result.status).toBe('available');
    expect(invoke.mock.calls[1]).toEqual([
      'query_agent_work_trace_sources_v2',
      {
        request: {
          cursor: id('4'),
          projectionRef: id('2'),
          protocolVersion: 1,
          sessionId: id('1'),
          userSequence: '7',
        },
      },
    ]);
  });
});
