export interface AnimationFrameClock {
  cancel(frameId: number): void;
  request(callback: FrameRequestCallback): number;
}

type Cleanup = () => void;
type PollTask = (generation: number) => Promise<void>;

interface PendingCommit {
  commit: () => void;
  generation: number;
}

interface PollState {
  generation: number;
  queued: boolean;
  task: PollTask;
}

/**
 * Owns transient WebView work without becoming a source of domain truth.
 *
 * Project generations reject late async commits. Render commits use latest-wins batching per key,
 * while poll sources can retain at most one queued rerun behind the currently executing request.
 */
export class UiScheduler {
  readonly #appCleanups = new Set<Cleanup>();
  readonly #clock: AnimationFrameClock;
  readonly #pendingCommits = new Map<string, PendingCommit>();
  readonly #polls = new Map<string, PollState>();
  readonly #projectCleanups = new Set<Cleanup>();
  #disposed = false;
  #frameId: number | null = null;
  #generation = 0;
  #projectKey: string | null = null;

  constructor(clock: AnimationFrameClock) {
    this.#clock = clock;
  }

  get generation(): number {
    return this.#generation;
  }

  get pendingCommitCount(): number {
    return this.#pendingCommits.size;
  }

  /** Starts a new project generation only when the stable project key changed. */
  beginProject(projectKey: string | null): boolean {
    if (this.#disposed || this.#projectKey === projectKey) return false;
    this.#projectKey = projectKey;
    this.#generation += 1;
    this.#resetProjectWork();
    return true;
  }

  /** Returns whether an async result still belongs to the active mounted project generation. */
  isCurrent(generation: number): boolean {
    return !this.#disposed && generation === this.#generation;
  }

  /** Coalesces a named presentation commit into the next animation frame. */
  queueCommit(key: string, generation: number, commit: () => void): void {
    if (key.length === 0 || !this.isCurrent(generation)) return;
    this.#pendingCommits.set(key, { commit, generation });
    if (this.#frameId !== null) return;
    this.#frameId = this.#clock.request(() => this.#flush());
  }

  /**
   * Runs one poll source at a time. Calls received while it runs collapse into one latest rerun.
   */
  poll(key: string, task: PollTask): void {
    if (this.#disposed || key.length === 0) return;
    const current = this.#polls.get(key);
    if (current !== undefined && current.generation === this.#generation) {
      current.queued = true;
      current.task = task;
      return;
    }

    const state: PollState = {
      generation: this.#generation,
      queued: false,
      task,
    };
    this.#polls.set(key, state);
    void this.#runPoll(key, state);
  }

  ownAppCleanup(cleanup: Cleanup): void {
    if (this.#disposed) {
      cleanup();
      return;
    }
    this.#appCleanups.add(cleanup);
  }

  ownProjectCleanup(cleanup: Cleanup): void {
    if (this.#disposed) {
      cleanup();
      return;
    }
    this.#projectCleanups.add(cleanup);
  }

  /** Releases all app- and project-owned work. Safe to call more than once. */
  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#generation += 1;
    this.#resetProjectWork();
    this.#runCleanups(this.#appCleanups);
  }

  #flush(): void {
    this.#frameId = null;
    const commits = [...this.#pendingCommits.values()];
    this.#pendingCommits.clear();
    for (const pending of commits) {
      if (this.isCurrent(pending.generation)) pending.commit();
    }
  }

  #resetProjectWork(): void {
    if (this.#frameId !== null) {
      this.#clock.cancel(this.#frameId);
      this.#frameId = null;
    }
    this.#pendingCommits.clear();
    for (const poll of this.#polls.values()) poll.queued = false;
    this.#polls.clear();
    this.#runCleanups(this.#projectCleanups);
  }

  async #runPoll(key: string, state: PollState): Promise<void> {
    do {
      state.queued = false;
      const task = state.task;
      try {
        await task(state.generation);
      } catch {
        // Poll tasks own their bounded error projection; the scheduler only owns concurrency.
      }
    } while (state.queued && this.isCurrent(state.generation));

    if (this.#polls.get(key) === state) this.#polls.delete(key);
  }

  #runCleanups(cleanups: Set<Cleanup>): void {
    const owned = [...cleanups];
    cleanups.clear();
    for (const cleanup of owned) cleanup();
  }
}
