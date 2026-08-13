import { UiScheduler } from '../lib/ui-scheduler';

const BURST_SIZE = 10_000;
const DOM_ROW_LIMIT = 50;
const SAMPLE_COUNT = 30;
const UI_BLOCK_BUDGET_MS = 100;

interface BurstSample {
  enqueueDurationMs: number;
  interactionLatencyMs: number;
  renderedCommits: number;
}

interface ProfileResult {
  budgetMs: number;
  burstSize: number;
  enqueueP95Ms: number;
  interactionP95Ms: number;
  longTaskCount: number;
  maxLongTaskMs: number;
  maxPendingCommits: number;
  renderedCommitsPerSample: number;
  renderedRows: number;
  samples: number;
  status: 'pass' | 'fail';
  userAgent: string;
}

function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!(element instanceof HTMLElement)) throw new Error(`Missing profile element: ${id}`);
  return element as T;
}

function percentile95(values: readonly number[]): number {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(sorted.length * 0.95) - 1] ?? 0;
}

function rounded(value: number): number {
  return Number(value.toFixed(3));
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

async function sampleIndexBurst(
  status: HTMLElement,
  interactionTarget: HTMLButtonElement,
): Promise<BurstSample & { maxPendingCommits: number }> {
  const scheduler = new UiScheduler({
    cancel: (frameId) => cancelAnimationFrame(frameId),
    request: (callback) => requestAnimationFrame(callback),
  });
  scheduler.beginProject('u10-browser-profile');
  let renderedCommits = 0;
  let resolveCommit: (() => void) | undefined;
  const committed = new Promise<void>((resolve) => {
    resolveCommit = resolve;
  });
  const interactionStartedAt = performance.now();
  const interactionCompleted = new Promise<number>((resolve) => {
    const onClick = () => {
      interactionTarget.removeEventListener('click', onClick);
      resolve(performance.now() - interactionStartedAt);
    };
    interactionTarget.addEventListener('click', onClick);
    setTimeout(() => interactionTarget.click(), 0);
  });

  const enqueueStartedAt = performance.now();
  for (let sequence = 0; sequence < BURST_SIZE; sequence += 1) {
    scheduler.queueCommit('index-activity', scheduler.generation, () => {
      status.textContent = `Letztes gebündeltes Indexereignis: ${sequence + 1}`;
      renderedCommits += 1;
      resolveCommit?.();
    });
  }
  const enqueueDurationMs = performance.now() - enqueueStartedAt;
  const maxPendingCommits = scheduler.pendingCommitCount;
  const interactionLatencyMs = await interactionCompleted;
  await committed;
  scheduler.dispose();

  return {
    enqueueDurationMs,
    interactionLatencyMs,
    maxPendingCommits,
    renderedCommits,
  };
}

function renderBoundedIndexRows(list: HTMLOListElement): number {
  const fragment = document.createDocumentFragment();
  for (let row = 0; row < DOM_ROW_LIMIT; row += 1) {
    const item = document.createElement('li');
    item.textContent = `Indexprojektion ${row + 1}`;
    fragment.append(item);
  }
  list.replaceChildren(fragment);
  return list.childElementCount;
}

async function runProfile(): Promise<void> {
  const status = requiredElement('profile-status');
  const resultElement = requiredElement('profile-result');
  const interactionTarget = requiredElement<HTMLButtonElement>('interaction-target');
  const boundedIndexView = requiredElement<HTMLOListElement>('bounded-index-view');
  const longTasks: number[] = [];
  const longTaskObserver = PerformanceObserver.supportedEntryTypes.includes('longtask')
    ? new PerformanceObserver((entries) => {
        for (const entry of entries.getEntries()) longTasks.push(entry.duration);
      })
    : null;
  longTaskObserver?.observe({ entryTypes: ['longtask'] });

  const samples: Array<BurstSample & { maxPendingCommits: number }> = [];
  for (let sample = 0; sample < SAMPLE_COUNT; sample += 1) {
    samples.push(await sampleIndexBurst(status, interactionTarget));
    await nextFrame();
  }
  await new Promise((resolve) => setTimeout(resolve, 0));
  longTaskObserver?.disconnect();

  const renderedRows = renderBoundedIndexRows(boundedIndexView);
  const enqueueP95Ms = percentile95(samples.map((sample) => sample.enqueueDurationMs));
  const interactionP95Ms = percentile95(samples.map((sample) => sample.interactionLatencyMs));
  const maxLongTaskMs = Math.max(0, ...longTasks);
  const maxPendingCommits = Math.max(...samples.map((sample) => sample.maxPendingCommits));
  const renderedCommitsPerSample = Math.max(...samples.map((sample) => sample.renderedCommits));
  const passes =
    enqueueP95Ms <= UI_BLOCK_BUDGET_MS &&
    interactionP95Ms <= UI_BLOCK_BUDGET_MS &&
    maxLongTaskMs <= UI_BLOCK_BUDGET_MS &&
    maxPendingCommits === 1 &&
    renderedCommitsPerSample === 1 &&
    renderedRows === DOM_ROW_LIMIT;

  const result: ProfileResult = {
    budgetMs: UI_BLOCK_BUDGET_MS,
    burstSize: BURST_SIZE,
    enqueueP95Ms: rounded(enqueueP95Ms),
    interactionP95Ms: rounded(interactionP95Ms),
    longTaskCount: longTasks.length,
    maxLongTaskMs: rounded(maxLongTaskMs),
    maxPendingCommits,
    renderedCommitsPerSample,
    renderedRows,
    samples: SAMPLE_COUNT,
    status: passes ? 'pass' : 'fail',
    userAgent: navigator.userAgent,
  };
  status.textContent = passes ? 'Profil bestanden.' : 'Profil fehlgeschlagen.';
  resultElement.textContent = JSON.stringify(result, null, 2);
  document.documentElement.dataset.profileStatus = result.status;
}

void runProfile().catch((error: unknown) => {
  requiredElement('profile-status').textContent = 'Profil fehlgeschlagen.';
  requiredElement('profile-result').textContent =
    error instanceof Error ? error.message : 'Unbekannter Profilfehler.';
  document.documentElement.dataset.profileStatus = 'error';
});
