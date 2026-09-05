import { describe, expect, it, vi } from 'vitest';
import { parseFlowResponse, queryFunctionFlows } from './function-flow';
import { flow, selection } from './function-flow.test-fixtures';
describe('function flow V1 boundary', () => {
  it('decodes exact occurrence paths', () => {
    expect(
      parseFlowResponse({ protocolVersion: 1, result: { status: 'flow', flow } }).result,
    ).toEqual({ status: 'flow', flow });
  });
  it.each([
    { protocolVersion: 1, result: { status: 'flow', flow: { ...flow, stepTotal: 0 } } },
    { protocolVersion: 1, result: { status: 'flow', flow: { ...flow, breadcrumbs: [] } } },
    {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: { ...flow, steps: [{ ...flow.steps[0], processMode: 'spawn' }] },
      },
    },
    { protocolVersion: 2, result: { status: 'noProject' } },
    { protocolVersion: 1, result: { status: 'noProject', path: '/outside' } },
    {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: { ...flow, steps: Array.from({ length: 51 }, () => flow.steps[0]) },
      },
    },
    {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: { ...flow, selection: { ...selection, callPath: Array(8).fill(1) } },
      },
    },
    {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: { ...flow, steps: [{ ...flow.steps[0], target: { ...selection, callPath: [2] } }] },
      },
    },
    {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: { ...flow, values: [{ ...flow.values[0], name: 'bad\u0000value' }] },
      },
    },
  ])('rejects malformed or over-budget response %#', (payload) => {
    expect(() => parseFlowResponse(payload)).toThrow();
  });
  it('does not invoke invalid queries', async () => {
    const invoke = vi.fn();
    await expect(
      queryFunctionFlows({ kind: 'catalog', term: '', offset: 1 }, invoke),
    ).rejects.toThrow();
    expect(invoke).not.toHaveBeenCalled();
  });
  it('requests source by step identity and reuses the strict safe-preview decoder', async () => {
    const preview = {
      highlight: { endColumn: 3, endLine: 1, startColumn: 0, startLine: 1 },
      language: 'typeScriptJavaScript',
      lineCount: 1,
      pathDisplay: 'src/a.ts',
      startLine: 1,
      text: 'B();\n',
      truncatedAfter: false,
      truncatedBefore: false,
    };
    const invoke = vi
      .fn()
      .mockResolvedValue({ protocolVersion: 1, result: { status: 'source', preview } });
    const response = await queryFunctionFlows({ kind: 'source', selection, step: 1 }, invoke);
    expect(response.result).toEqual({ status: 'source', preview });
    expect(invoke).toHaveBeenCalledWith('query_function_flows', {
      request: { protocolVersion: 1, query: { kind: 'source', selection, step: 1 } },
    });
    expect(() =>
      parseFlowResponse({
        protocolVersion: 1,
        result: { status: 'source', preview: { ...preview, path: '/secret' } },
      }),
    ).toThrow();
    await expect(
      queryFunctionFlows({ kind: 'source', selection, step: 0 }, invoke),
    ).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
