import { mount, unmount } from 'svelte';
import '../styles.css';
import AgentWorkspace from '../lib/AgentWorkspace.svelte';
import {
  activeAgentActivity,
  activeAgentSession,
  adaptiveWorkPlan,
} from '../lib/agent-execution.fixture';
import projections from '../lib/agent-execution-contract.fixture.json';
import { parseAgentInspectionResponseV1 } from '../lib/agent-inspection';
import {
  parseAgentApprovalResponseV1,
  type AgentApprovalControlResponseV1,
} from '../lib/agent-approval';
import type { GlobalRunStatus } from '../lib/global-status';
import { queryTaskLensTask } from '../lib/task-lens';

// Offline projection fixture. No provider, Tauri commands or repository writes.
const root = document.getElementById('profile-root');
const output = document.getElementById('profile-result');
if (!root || !output) throw new Error('Missing profile host');
const sessionResponse = activeAgentSession();
const activityResponse = activeAgentActivity();
const planResponse = adaptiveWorkPlan();
if (
  sessionResponse.result.status !== 'available' ||
  activityResponse.result.status !== 'available' ||
  !activityResponse.result.activity.run ||
  planResponse.result.status !== 'available'
)
  throw new Error('Invalid fixture');
const session = sessionResponse.result.session;
const run = activityResponse.result.activity.run;
const plan = planResponse.result;
for (let index = 3; index < 40; index += 1)
  plan.steps.push({
    intendedOutcome: `Weitere Prüfung ${index + 1}`,
    status: 'pending',
    stepId: index.toString(16).padStart(64, '0'),
  });
let reads = 0;
let conflicts = false;
let planReadFailure = new URLSearchParams(window.location.search).has('plan-read-failure');
let grant: 'pending' | 'active' = 'pending';
let approvalStage = 1;
const approvals = projections.filter((projection) => projection.kind === 'approval');
const inspections = projections.filter((projection) => projection.kind === 'inspection');
let status: GlobalRunStatus = { kind: 'idle' };
const statuses: GlobalRunStatus['kind'][] = [];
const owner = new AbortController();
async function delay(milliseconds: number): Promise<void> {
  if (owner.signal.aborted) return;
  await new Promise<void>((resolve) => {
    const finish = (): void => {
      window.clearTimeout(timer);
      owner.signal.removeEventListener('abort', finish);
      resolve();
    };
    const timer = window.setTimeout(finish, milliseconds);
    owner.signal.addEventListener('abort', finish, { once: true });
  });
}
const component = mount(AgentWorkspace, {
  target: root,
  props: {
    activeProject: true,
    pollIntervalMs: 250,
    sessionsLoader: async () => ({
      protocolVersion: 1,
      result: {
        status: 'available',
        nextCursor: null,
        sessions: [structuredClone(session.summary)],
      },
    }),
    sessionLoader: async () => structuredClone(sessionResponse),
    activityLoader: async () => {
      reads += 1;
      await delay(100);
      if (conflicts) throw new Error('Offline fixture read conflict');
      run.updatedAtUnixMillis = String(100 + reads);
      return structuredClone(activityResponse);
    },
    workPlanLoader: async (query) => {
      await delay(100);
      if (planReadFailure) throw new Error('Offline fixture work-plan read failure');
      return queryTaskLensTask(query, async () => structuredClone(planResponse));
    },
    approvalLoader: async () => {
      await delay(80);
      if (conflicts) return { protocolVersion: 1, result: { status: 'activityChanged' } };
      const response = parseAgentApprovalResponseV1(
        structuredClone(approvals[approvalStage === 1 ? 0 : 1].response),
      );
      if (response.result.status === 'available') {
        const approval = response.result.approval;
        approval.status = grant;
        approval.canAllowOnce = approval.canDeny = grant === 'pending';
        approval.canContinue = approval.canRevoke = grant === 'active';
      }
      return response;
    },
    approvalController: async (
      _task,
      _approval,
      action,
    ): Promise<AgentApprovalControlResponseV1> => {
      if (action === 'allowOnce') grant = 'active';
      else if (action === 'revoke') grant = 'pending';
      else {
        session.summary.state = action === 'deny' ? 'failed' : 'running';
        run.state = action === 'deny' ? 'failed' : 'execute';
      }
      return {
        protocolVersion: 1,
        result: {
          status: 'applied',
          approvalRevision: '4',
          ledgerStoreVersion: '7',
          runtimeStart: action === 'continue' ? 'queued' : null,
          outcome:
            action === 'allowOnce'
              ? 'grantStored'
              : action === 'continue'
                ? 'continueRequested'
                : action === 'revoke'
                  ? 'revoked'
                  : 'denied',
        },
      };
    },
    inspectionLoader: async () => {
      await delay(120);
      return conflicts
        ? { protocolVersion: 1, result: { status: 'inspectionChanged' } }
        : parseAgentInspectionResponseV1(
            structuredClone(inspections[approvalStage === 1 ? 0 : 1].response),
          );
    },
    researchProjectionLoader: async () => ({ protocolVersion: 1, result: { status: 'notFound' } }),
    onRunStatusChange: (next) => {
      status = next;
      statuses.push(next.kind);
    },
  },
});
function button(id: string, action: () => void): void {
  document.getElementById(id)?.addEventListener('click', action, { signal: owner.signal });
}
button('approval', () => {
  session.summary.state = 'awaitingApproval';
  run.state = 'awaitApproval';
  grant = 'pending';
  approvalStage = 1;
});
button('process-approval', () => {
  session.summary.state = 'awaitingApproval';
  run.state = 'awaitApproval';
  grant = 'pending';
  approvalStage = 3;
});
button('conflict', () => {
  conflicts = true;
});
button('recover', () => {
  conflicts = false;
  planReadFailure = false;
});
button('theme', () => {
  document.documentElement.dataset.theme =
    document.documentElement.dataset.theme === 'light' ? 'dark' : 'light';
});
button('compact', () => root.classList.toggle('compact'));
let measuring = false;
button('measure', () => {
  void measure();
});
async function measure(): Promise<void> {
  if (measuring || !root || !output) return;
  measuring = true;
  const viewport = root.querySelector<HTMLElement>('.message-scroll');
  const card = root.querySelector<HTMLElement>('.execution-card');
  const history = root.querySelector<HTMLDetailsElement>('.execution-activity');
  if (!viewport || !card || !history) {
    measuring = false;
    return;
  }
  // Simulate a deliberate reader scroll; subsequent resizes must not take ownership back.
  viewport.dispatchEvent(new WheelEvent('wheel', { deltaY: -60, bubbles: true }));
  viewport.scrollTop = Math.max(0, viewport.scrollTop - 60);
  const samples: number[][] = [];
  const priorReads = reads;
  const priorStatuses = statuses.length;
  let planLoadingSamples = 0;
  for (let index = 0; index < 20 && !owner.signal.aborted; index += 1) {
    samples.push([
      viewport.scrollTop,
      viewport.scrollHeight,
      card.getBoundingClientRect().height,
      history.getBoundingClientRect().top,
    ]);
    if (card.textContent?.includes('Arbeitsplan wird geladen')) planLoadingSamples += 1;
    await delay(100);
  }
  const spread = (column: number): number =>
    Math.max(...samples.map((row) => row[column])) - Math.min(...samples.map((row) => row[column]));
  output.textContent = JSON.stringify({
    samples: samples.length,
    reads: reads - priorReads,
    scrollDrift: spread(0),
    contentHeightDrift: spread(1),
    cardHeightDrift: spread(2),
    historyTopDrift: spread(3),
    sameHistory: root.querySelector('.execution-activity') === history,
    historyOpen: history.open,
    planLoadingSamples,
    sameCard: root.querySelector('.execution-card') === card,
    status,
    statusResets: statuses.slice(priorStatuses).filter((kind) => kind !== 'available').length,
  });
  measuring = false;
}
window.addEventListener(
  'beforeunload',
  () => {
    owner.abort();
    void unmount(component);
  },
  { once: true },
);
