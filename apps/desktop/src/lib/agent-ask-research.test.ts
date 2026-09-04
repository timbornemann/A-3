import { describe, expect, it, vi } from 'vitest';
import {
  queryAgentAskResearchDetail,
  queryAgentAskResearchSourcePreview,
  queryAgentAskResearchSources,
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
});
