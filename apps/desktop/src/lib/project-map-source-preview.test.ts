import { describe, expect, it, vi } from 'vitest';
import {
  parseProjectMapSourcePreviewResponseV1,
  queryProjectMapSourcePreview,
} from './project-map-source-preview';

const id = (digit: string): string => digit.repeat(64);
const query = {
  cardId: id('1'),
  currentIndexRunId: id('2'),
  currentSnapshotId: id('3'),
  evidenceId: id('4'),
  moduleId: id('5'),
  sourceIndexRunId: id('2'),
  sourceSnapshotId: id('3'),
};

function response() {
  return {
    protocolVersion: 1,
    result: {
      preview: {
        highlight: { endColumn: 12, endLine: 10, startColumn: 0, startLine: 10 },
        language: 'rust',
        lineCount: 2,
        pathDisplay: 'src/lib.rs',
        startLine: 9,
        text: 'context\nfn main() {}\n',
        truncatedAfter: true,
        truncatedBefore: true,
      },
      status: 'available',
    },
  };
}

describe('Project Map source preview V1', () => {
  it('submits only the complete Evidence selection', async () => {
    const invoke = vi.fn().mockResolvedValue(response());
    await queryProjectMapSourcePreview(query, invoke);
    expect(invoke).toHaveBeenCalledWith('query_project_map_source_preview', {
      request: { protocolVersion: 1, selection: { ...query, kind: 'moduleCard' } },
    });
  });

  it('submits current index Evidence without a path or caller-controlled range', async () => {
    const invoke = vi.fn().mockResolvedValue(response());
    await queryProjectMapSourcePreview(
      {
        evidence: {
          evidenceId: id('4'),
          kind: 'symbol',
          moduleId: id('5'),
          symbolId: id('6'),
        },
        kind: 'index',
      },
      invoke,
    );
    expect(invoke).toHaveBeenCalledWith('query_project_map_source_preview', {
      request: {
        protocolVersion: 1,
        selection: {
          evidence: {
            evidenceId: id('4'),
            kind: 'symbol',
            moduleId: id('5'),
            symbolId: id('6'),
          },
          kind: 'index',
        },
      },
    });
  });

  it('rejects unknown fields, oversized pages, and highlights outside the page', () => {
    const unknown = response();
    Object.assign(unknown.result.preview, { html: '<b>source</b>' });
    expect(() => parseProjectMapSourcePreviewResponseV1(unknown)).toThrow();

    const oversized = response();
    oversized.result.preview.lineCount = 65;
    oversized.result.preview.text = 'line\n'.repeat(65);
    expect(() => parseProjectMapSourcePreviewResponseV1(oversized)).toThrow();

    const outside = response();
    outside.result.preview.highlight.startLine = 8;
    expect(() => parseProjectMapSourcePreviewResponseV1(outside)).toThrow();
  });

  it('keeps stale Evidence metadata-only', () => {
    expect(
      parseProjectMapSourcePreviewResponseV1({
        protocolVersion: 1,
        result: { status: 'staleEvidence' },
      }).result,
    ).toEqual({ status: 'staleEvidence' });
  });
});
