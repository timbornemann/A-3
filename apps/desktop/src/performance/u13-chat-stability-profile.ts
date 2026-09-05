import { mount, tick, unmount } from 'svelte';
import '../styles.css';
import AgentWorkspace from '../lib/AgentWorkspace.svelte';
import type { AgentSessionV1 } from '../lib/agent-session';
import type { AgentAskResearchDetailV1, AgentWorkTraceSourceV2 } from '../lib/agent-ask-research';
import type { AgentDiagramSummaryV1 } from '../lib/agent-diagram';

// Deterministic, offline UI fixture. No Tauri runtime, provider or repository access.
const root = document.getElementById('profile-root');
const output = document.getElementById('profile-result');
if (!root || !output) throw new Error('Missing profile host');
const id = 'a'.repeat(64);
const session: AgentSessionV1 = {
  activeTaskId: null,
  hasOlderEntries: false,
  entries: [],
  summary: {
    currentPlanRevision: null,
    mode: 'ask',
    revision: '1',
    sessionId: id,
    state: 'running',
    title: 'Langer Chat mit Diagrammen',
    updatedAtUnixMillis: '100',
  },
};
const diagrams = new Map<string, AgentDiagramSummaryV1>();
for (let turn = 0; turn < 20; turn += 1) {
  const sequence = String(turn * 2 + 1);
  const summary: AgentDiagramSummaryV1 = {
    artifactRef: String(turn).padStart(128, 'a'),
    description: 'Offline-Fixture mit echtem lokalen Mermaid-Renderer',
    kind: 'flowchart',
    stale: false,
    title: `Diagramm ${turn}`,
    userSequence: sequence,
  };
  if (turn >= 18) diagrams.set(summary.artifactRef, summary);
  session.entries.push({
    sequence,
    createdAtUnixMillis: '100',
    kind: 'userMessage',
    planRevision: null,
    text: `Frage ${turn + 1}: Erkläre den Aufbau.`,
    diagrams: turn >= 18 ? [summary] : [],
  });
  session.entries.push({
    sequence: String(turn * 2 + 2),
    createdAtUnixMillis: '101',
    kind: 'finalReport',
    planRevision: null,
    text: `## Antwort ${turn + 1}\n\n${'Der Manager lädt aktuelle Aufgaben, prüft den Inhalt und informiert die Plugins. Die Quellen bleiben an diese Antwort gebunden. '.repeat(6)}\n\n【S1】`,
  });
}
let turnSequence = '41';
session.entries.push({
  sequence: turnSequence,
  createdAtUnixMillis: '102',
  kind: 'userMessage',
  planRevision: null,
  text: 'Verfolge die nächste Recherche schrittweise.',
  diagrams: [],
});
type Step = AgentAskResearchDetailV1['steps'][number];
let steps: Step[] = [
  {
    action: 'Aktuellen Projektstand binden',
    completeness: 'notApplicable',
    occurredAtUnixMillis: '200',
    note: null,
    phase: 'preparing',
    query: null,
    state: 'running',
  },
];
const source: AgentWorkTraceSourceV2 = {
  referenceLabel: 'S1',
  sourceRef: 'b'.repeat(128),
  path: 'src/manager.py',
  startLine: 18,
  endLine: 32,
  kind: 'file',
  reason: 'exactNameOrPath',
  symbol: null,
  usedForAnswer: false,
};
let sessionReads = 0;
let traceReads = 0;
let artifactReads = 0;
let profiling = false;
let stopped = false;
const timers = new Set<ReturnType<typeof setTimeout>>();
const delay = (ms: number): Promise<void> =>
  new Promise((resolve) => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      resolve();
    }, ms);
    timers.add(timer);
  });
const app = mount(AgentWorkspace, {
  target: root,
  props: {
    activeProject: true,
    pollIntervalMs: 80,
    sessionsLoader: async () => ({
      protocolVersion: 1,
      result: {
        status: 'available',
        sessions: [structuredClone(session.summary)],
        nextCursor: null,
      },
    }),
    sessionLoader: async () => {
      sessionReads += 1;
      return {
        protocolVersion: 1,
        result: { status: 'available', session: structuredClone(session) },
      };
    },
    messageSubmitter: async () => {
      next();
      return {
        protocolVersion: 1,
        result: { status: 'available', session: structuredClone(session) },
      };
    },
    slashCommandsLoader: async () => ({
      protocolVersion: 1,
      catalogVersion: 1,
      commands: [],
    }),
    researchProjectionLoader: async (_sessionId, userSequence) => {
      traceReads += 1;
      const current = userSequence === turnSequence;
      const completed = !current || session.summary.state === 'completed';
      return {
        protocolVersion: 1,
        result: {
          status: 'available',
          projectionRef: 'c'.repeat(128),
          nextCursor: null,
          sources: [{ ...source, usedForAnswer: completed }],
          detail: {
            citedSourceCount: completed ? 1 : 0,
            depth: 'standard',
            legacy: false,
            mode: 'ask',
            sourceCount: 1,
            stale: false,
            userSequence,
            steps: current
              ? structuredClone(steps)
              : [
                  {
                    ...steps[0],
                    action: 'Antwort veröffentlicht',
                    phase: 'completed',
                    state: 'completed',
                  },
                ],
          },
        },
      };
    },
    diagramArtifactLoader: async (_sessionId, artifactRef) => {
      artifactReads += 1;
      const summary = diagrams.get(artifactRef);
      return {
        protocolVersion: 1,
        result: summary
          ? {
              kind: 'available',
              artifact: {
                summary,
                mermaid:
                  'flowchart TD\n n0["Manager"]\n n1["Speicher"]\n n2["Plugin"]\n n0 -->|"load_tasks()"| n1\n n1 -->|"on_task_created()"| n2\n',
              },
            }
          : { kind: 'notFound' },
      };
    },
  },
});

function finish(): void {
  if (session.summary.state !== 'running') return;
  steps.push({
    ...steps[0],
    action: 'Antwort und Quellen veröffentlicht',
    occurredAtUnixMillis: String(200 + steps.length),
    phase: 'completed',
    state: 'completed',
  });
  session.entries.push({
    sequence: String(Number(turnSequence) + 1),
    createdAtUnixMillis: '300',
    kind: 'finalReport',
    planRevision: null,
    text: 'Die neue Recherche ist vollständig abgeschlossen. 【S1】',
  });
  session.summary.state = 'completed';
  session.summary.revision = String(Number(session.summary.revision) + 1);
}
function next(): void {
  if (session.summary.state === 'running') return;
  turnSequence = String(Number(turnSequence) + 2);
  session.entries.push({
    sequence: turnSequence,
    createdAtUnixMillis: '301',
    kind: 'userMessage',
    planRevision: null,
    text: 'Und wie funktioniert der nächste Teil?',
    diagrams: [],
  });
  session.summary.state = 'running';
  session.summary.revision = String(Number(session.summary.revision) + 1);
  steps = [
    { ...steps[0], action: 'Folgerecherche vorbereiten', state: 'running', phase: 'preparing' },
  ];
}

async function runProfile(): Promise<void> {
  if (profiling || stopped) return;
  profiling = true;
  const viewport = root?.querySelector<HTMLElement>('.message-scroll');
  if (!viewport || !output || !root) return;
  // Include both real Mermaid SVGs in the retained set, even on a cold lazy load.
  for (
    let attempt = 0;
    attempt < 100 && root.querySelectorAll('.diagram-canvas svg').length < 2;
    attempt += 1
  )
    await delay(50);
  await delay(200);
  const following = !root.querySelector('.follow-latest');
  const initialScrollTop = viewport.scrollTop;
  let maxManualDrift = 0;
  const retained = [...root.querySelectorAll('.conversation-turn')]
    .slice(0, 20)
    .flatMap((turn) => [...turn.querySelectorAll('.ask-research, .diagram-canvas svg')]);
  const heights: number[] = [];
  let removals = 0;
  let historicMutations = 0;
  const observer = new MutationObserver((records) => {
    for (const record of records) {
      if (retained.some((node) => node === record.target || node.contains(record.target)))
        historicMutations += 1;
      for (const removed of record.removedNodes)
        if (retained.some((node) => removed === node || removed.contains(node))) removals += 1;
    }
  });
  observer.observe(viewport, { childList: true, subtree: true, characterData: true });
  const startReads = { sessionReads, traceReads, artifactReads };
  let maxTailDistance = 0;
  const latencies: number[] = [];
  for (let poll = 0; poll < 60 && !stopped; poll += 1) {
    if (poll % 10 === 0) {
      const round = Math.floor(poll / 10) + 1;
      steps.push({
        ...steps[0],
        action: `Recherche-Runde ${round}: nächsten Beleg auswählen`,
        phase: 'deciding',
        occurredAtUnixMillis: String(200 + steps.length),
      });
      steps.push({
        ...steps[0],
        action: `Quellen für Runde ${round} prüfen`,
        phase: 'reading',
        occurredAtUnixMillis: String(200 + steps.length),
        query: 'manager.py:18–32',
        note: {
          goal: 'Den Aufruf belegen.',
          finding: 'Aktuelle Quelle gelesen.',
          findingKind: 'observation',
          gap: 'Weitere direkte Aufrufer prüfen.',
          nextStep: 'Nächsten aktuellen Quellbereich lesen.',
          sourceRefs: [source.sourceRef],
        },
      });
    }
    const start = performance.now();
    await delay(100);
    latencies.push(Math.max(0, performance.now() - start - 100));
    heights.push(viewport.scrollHeight);
    if (!following)
      maxManualDrift = Math.max(maxManualDrift, Math.abs(viewport.scrollTop - initialScrollTop));
    // Sample settled frames, not the unavoidable frame between layout and follow.
    if (poll % 10 >= 7) {
      const tail = viewport.querySelector(
        '.conversation-turn:last-child .research-steps li:last-child',
      );
      maxTailDistance = Math.max(
        maxTailDistance,
        tail
          ? Math.abs(
              viewport.getBoundingClientRect().bottom - tail.getBoundingClientRect().bottom - 12,
            )
          : viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop,
      );
    }
  }
  observer.disconnect();
  latencies.sort((a, b) => a - b);
  const shrinkCount = heights.filter(
    (height, index) => index > 0 && height < heights[index - 1],
  ).length;
  output.textContent = JSON.stringify({
    status:
      removals === 0 &&
      historicMutations === 0 &&
      shrinkCount === 0 &&
      (following ? maxTailDistance <= 2 : maxManualDrift <= 1)
        ? 'pass'
        : 'fail',
    historicalTurns: 20,
    samples: heights.length,
    retainedNodes: retained.length,
    removals,
    historicMutations,
    shrinkCount,
    maxTailDistance,
    following,
    maxManualDrift,
    interactionP95Ms: latencies[Math.floor(latencies.length * 0.95)],
    domNodes: root.querySelectorAll('*').length,
    sessionReads: sessionReads - startReads.sessionReads,
    traceReads: traceReads - startReads.traceReads,
    artifactReads: artifactReads - startReads.artifactReads,
    horizontalOverflow:
      root.scrollWidth > window.innerWidth || viewport.scrollWidth > viewport.clientWidth,
  });
  profiling = false;
}
document.getElementById('run-profile')?.addEventListener('click', () => void runProfile());
document.getElementById('finish-turn')?.addEventListener('click', finish);
document.getElementById('next-turn')?.addEventListener('click', async () => {
  finish();
  const composer = root.querySelector<HTMLTextAreaElement>('textarea');
  if (!composer) return;
  composer.value = 'Und wie funktioniert der nächste Teil?';
  composer.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  root.querySelector<HTMLButtonElement>('button[aria-label="Nachricht senden"]')?.click();
});
window.addEventListener(
  'pagehide',
  () => {
    stopped = true;
    for (const timer of timers) clearTimeout(timer);
    void unmount(app);
  },
  { once: true },
);
