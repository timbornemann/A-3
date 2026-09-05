import type { FlowEntry, FlowView, FlowSelection } from './function-flow';
export const selection: FlowSelection = {
  runId: 'a'.repeat(64),
  root: 'b'.repeat(64),
  callPath: [],
};
export const entry: FlowEntry = {
  selection,
  name: 'A',
  category: 'function',
  source: { path: 'src/a.ts', line: 1, preview: null, mapSelection: null },
};
export const flow: FlowView = {
  selection,
  name: 'A',
  source: entry.source,
  breadcrumbs: [entry],
  steps: [
    {
      id: 1,
      parent: null,
      kind: 'call',
      processMode: null,
      valuesTruncated: false,
      name: 'B',
      line: 2,
      target: { ...selection, callPath: [1] },
      inputs: [1],
      outputs: [2],
    },
  ],
  values: [
    { id: 1, name: 'input', kind: 'parameter', line: 1 },
    { id: 2, name: 'result', kind: 'callResult', line: 2 },
  ],
  stepTotal: 1,
  valueTotal: 2,
  gaps: [],
  gapsTruncated: false,
};
