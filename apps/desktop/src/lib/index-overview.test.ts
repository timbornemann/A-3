import { describe, expect, it, vi } from 'vitest';
import { parseIndexOverviewResponseV1, queryIndexOverview } from './index-overview';

const published = {
  protocolVersion: 1,
  result: {
    overview: {
      counts: {
        diagnosticCount: '1',
        diagnosticFileCount: '1',
        fileCount: '2',
        parsedFileCount: '1',
        symbolCount: '3',
      },
      coverageBasisPoints: 8000,
      diagnosticFiles: [
        {
          coverageBasisPoints: 8000,
          diagnosticCount: '1',
          diagnostics: [
            {
              code: 'syntaxError',
              endByte: 10,
              message: 'syntax error',
              severity: 'error',
              startByte: 8,
            },
          ],
          diagnosticsTruncated: false,
          language: 'rust',
          pathDisplay: 'src/lib.rs',
          pathDisplayTruncated: false,
        },
      ],
      diagnosticFilesTruncated: false,
      snapshotId: '1'.repeat(64),
    },
    status: 'published',
  },
} as const;

describe('index overview IPC', () => {
  it('invokes the pathless command and accepts the strict published response', async () => {
    const invoke = vi.fn(async () => published);

    await expect(queryIndexOverview(invoke)).resolves.toEqual(published);
    expect(invoke).toHaveBeenCalledWith('query_index_overview', {
      request: { protocolVersion: 1 },
    });
  });

  it('rejects unknown fields, contradictory totals, and unsafe diagnostic text', () => {
    expect(() =>
      parseIndexOverviewResponseV1({ ...published, authoritativePath: 'C:\\private' }),
    ).toThrow();
    expect(() =>
      parseIndexOverviewResponseV1({
        ...published,
        result: {
          ...published.result,
          overview: {
            ...published.result.overview,
            counts: { ...published.result.overview.counts, diagnosticFileCount: '0' },
          },
        },
      }),
    ).toThrow();
    expect(() =>
      parseIndexOverviewResponseV1({
        ...published,
        result: {
          ...published.result,
          overview: {
            ...published.result.overview,
            diagnosticFiles: [
              {
                ...published.result.overview.diagnosticFiles[0],
                diagnostics: [
                  {
                    ...published.result.overview.diagnosticFiles[0].diagnostics[0],
                    message: 'secret\nsource',
                  },
                ],
              },
            ],
          },
        },
      }),
    ).toThrow();
  });
});
