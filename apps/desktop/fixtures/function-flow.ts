// Local browser QA fixture. Not imported by the production entrypoint; no IPC or execution.
import { mount } from 'svelte';
import FlowWorkspace from '../src/lib/FlowWorkspace.svelte';
import { entry, flow, selection } from '../src/lib/function-flow.test-fixtures';
import type { FlowQuery, FlowResponse, FlowView } from '../src/lib/function-flow';
import '../src/styles.css';

const target = document.getElementById('app');
if (!target) throw new Error('Missing fixture mount');
const details: FlowView = {
  ...flow,
  name: 'Bestellung abschließen',
  steps: Array.from({ length: 50 }, (_, i) => ({
    ...flow.steps[0],
    id: i + 1,
    name:
      i === 0
        ? 'B · Eingaben prüfen'
        : i === 1
          ? 'C · Bericht erstellen'
          : `Arbeitsschritt ${i + 1}`,
    line: i + 2,
    target: { ...selection, callPath: [i + 1] },
  })),
  values: Array.from({ length: 50 }, (_, i) => ({
    ...flow.values[0],
    id: i + 1,
    name: `Eingabe ${i + 1}`,
  })),
  stepTotal: 4096,
  valueTotal: 4096,
};
async function loader(q: FlowQuery): Promise<FlowResponse> {
  if (q.kind === 'source') return { protocolVersion: 1, result: { status: 'selectionChanged' } };
  if (q.kind === 'catalog')
    return {
      protocolVersion: 1,
      result: {
        status: 'catalog',
        page: { entries: [{ ...entry, name: details.name }], hasMore: false },
      },
    };
  if (q.kind === 'inspect')
    return {
      protocolVersion: 1,
      result: {
        status: 'flow',
        flow: {
          ...details,
          selection: q.selection,
          name: q.selection.callPath.length ? 'B · Eingaben prüfen' : details.name,
          breadcrumbs: [{ ...entry, name: details.name }],
          steps: details.steps.slice(0, 4096 - q.stepOffset).map((step, i) => ({
            ...step,
            id: q.stepOffset + i + 1,
            target:
              q.selection.callPath.length < 7
                ? { ...q.selection, callPath: [...q.selection.callPath, q.stepOffset + i + 1] }
                : null,
          })),
          values: details.values
            .slice(0, 4096 - q.valueOffset)
            .map((value, i) => ({ ...value, id: q.valueOffset + i + 1 })),
        },
      },
    };
  return {
    protocolVersion: 1,
    result: {
      status: 'trace',
      trace: {
        direction: q.direction,
        truncated: true,
        nodes: Array.from({ length: 50 }, (_, i) => ({
          selection: q.selection,
          value: { ...details.values[i] },
          functionName: 'B · Eingaben prüfen',
          path: 'src/bestellung.ts',
          unknown: i % 3 === 0,
        })),
      },
    },
  };
}
mount(FlowWorkspace, {
  target,
  props: { projectKey: 'browser-fixture', publicationKey: 'fixture-v1', loader },
});
const metrics = document.createElement('output');
metrics.setAttribute('aria-label', 'Fixture-Messwerte');
metrics.style.cssText = 'display:block;padding:1rem';
document.body.append(metrics);
let longest = 0;
let maximumNodes = 0;
const observer = new PerformanceObserver((list) => {
  for (const item of list.getEntries()) longest = Math.max(longest, item.duration);
});
if (PerformanceObserver.supportedEntryTypes.includes('longtask'))
  observer.observe({ type: 'longtask' });
function measure() {
  maximumNodes = Math.max(maximumNodes, document.querySelectorAll('*').length);
  metrics.textContent = `Lokales UI-Fixture · DOM maximal: ${maximumNodes} · längster Long Task: ${longest.toFixed(1)} ms`;
}
document.addEventListener('click', () => {
  requestAnimationFrame(() => requestAnimationFrame(measure));
});
requestAnimationFrame(measure);
window.addEventListener('pagehide', () => observer.disconnect(), { once: true });
