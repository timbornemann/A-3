import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { DeepMapControlResponseV1, DeepMapStatusResponseV1 } from './lib/deep-map';
import type { HealthResponseV1 } from './lib/health';
import type { IndexActivityResponseV1 } from './lib/index-activity';
import type { IndexOverviewResponseV1 } from './lib/index-overview';
import type { ModuleCardFreshnessResponseV1 } from './lib/module-card-freshness';
import type { ModuleTreeResponseV1 } from './lib/module-tree';
import type { OpenProjectResponseV1, ProjectSummaryV1 } from './lib/project';
import type { RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
import type { RemoveProjectResponseV1 } from './lib/project-removal';
import type { ProjectStatusResponseV1 } from './lib/project-status';
import type { RecentProjectsResponseV1 } from './lib/recent-projects';
import type { RepositoryTreeResponseV1 } from './lib/repository-tree';

const health: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: 1,
  status: 'ready',
};

const projectSummary: ProjectSummaryV1 = {
  head: { kind: 'unborn', reference: 'refs/heads/main' },
  repositoryId: '1'.repeat(64),
  worktreeId: '2'.repeat(64),
  worktreeRootDisplay: 'C:\\worktree',
};

const openedProject: OpenProjectResponseV1 = {
  protocolVersion: 1,
  result: {
    project: projectSummary,
    status: 'opened',
  },
};

const emptyRecentProjects: RecentProjectsResponseV1 = {
  projects: [],
  protocolVersion: 1,
};

const noProjectStatus: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: { status: 'noProject' },
};

const activeProjectResult: Extract<ProjectStatusResponseV1['result'], { status: 'active' }> = {
  index: {
    latestAttemptSnapshotId: '4'.repeat(64),
    latestSnapshot: { generation: '2', snapshotId: '4'.repeat(64) },
    publishedSnapshotId: '4'.repeat(64),
    state: 'published',
  },
  project: projectSummary,
  projectId: '3'.repeat(64),
  rebuildState: 'idle',
  status: 'active',
  storageBytes: '4096',
};

const activeProjectStatus: ProjectStatusResponseV1 = {
  protocolVersion: 1,
  result: activeProjectResult,
};

const queuedRebuildStatus: ProjectStatusResponseV1 = {
  ...activeProjectStatus,
  result: { ...activeProjectResult, rebuildState: 'queued' },
};

const runningIndexActivity: IndexActivityResponseV1 = {
  protocolVersion: 1,
  result: {
    activity: {
      completedPhases: 3,
      phase: 'link',
      state: 'running',
      totalPhases: 6,
    },
    status: 'active',
  },
};

const publishedIndexOverview: IndexOverviewResponseV1 = {
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
      snapshotId: '4'.repeat(64),
    },
    status: 'published',
  },
};

const moduleCardFreshness: ModuleCardFreshnessResponseV1 = {
  protocolVersion: 1,
  result: {
    freshness: {
      counts: {
        needsReviewCount: '1',
        publishedCount: '7',
        staleCount: '2',
        totalCount: '10',
      },
      indexRunId: '6'.repeat(64),
      reasons: [
        { count: '2', reason: 'evidenceChanged', status: 'stale' },
        { count: '1', reason: 'directDependencyChanged', status: 'needsReview' },
      ],
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const moduleTreeRoot: ModuleTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      entries: [
        {
          boundaryEvidence: {
            manifestRevision: {
              contentHash: '7'.repeat(64),
              pathHex: '436172676f2e746f6d6c',
            },
            representativeRevision: {
              contentHash: '8'.repeat(64),
              pathHex: '7372632f6c69622e7273',
            },
          },
          centralSymbols: { count: '1', truncated: false },
          childState: 'hasChildren',
          entrypoints: { count: '1', truncated: false },
          fileCount: '1',
          kind: 'manifestBoundary',
          manifestCount: '1',
          moduleId: 'a'.repeat(64),
          name: 'Repository',
          nameTruncated: false,
          rootPathHex: null,
          symbolCount: '1',
          tests: { count: '0', truncated: false },
        },
      ],
      graphCommunityCount: '1',
      indexRunId: '6'.repeat(64),
      nextAfterModuleId: null,
      parentModuleId: null,
      primaryModuleCount: '2',
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const moduleTreeRepository: ModuleTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      entries: [
        {
          boundaryEvidence: {
            manifestRevision: null,
            representativeRevision: {
              contentHash: '9'.repeat(64),
              pathHex: '746f6f6c732f6d61696e2e7273',
            },
          },
          centralSymbols: { count: '1', truncated: true },
          childState: 'leaf',
          entrypoints: { count: '0', truncated: false },
          fileCount: '1',
          kind: 'pathBoundary',
          manifestCount: '0',
          moduleId: 'b'.repeat(64),
          name: 'tools',
          nameTruncated: false,
          rootPathHex: '746f6f6c73',
          symbolCount: '1',
          tests: { count: '0', truncated: false },
        },
      ],
      graphCommunityCount: '1',
      indexRunId: '6'.repeat(64),
      nextAfterModuleId: null,
      parentModuleId: 'a'.repeat(64),
      primaryModuleCount: '2',
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const repositoryTreeRoot: RepositoryTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      directoryPathHex: null,
      entries: [
        {
          contentHash: '7'.repeat(64),
          descendantFileCount: '1',
          kind: 'file',
          name: 'README.md',
          nameTruncated: false,
          pathHex: '524541444d452e6d64',
        },
        {
          contentHash: null,
          descendantFileCount: '2',
          kind: 'directory',
          name: 'src',
          nameTruncated: false,
          pathHex: '737263',
        },
      ],
      indexRunId: '6'.repeat(64),
      nextAfterNameHex: null,
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const repositoryTreeSrc: RepositoryTreeResponseV1 = {
  protocolVersion: 1,
  result: {
    page: {
      directoryPathHex: '737263',
      entries: [
        {
          contentHash: '8'.repeat(64),
          descendantFileCount: '1',
          kind: 'file',
          name: 'lib.rs',
          nameTruncated: false,
          pathHex: '7372632f6c69622e7273',
        },
      ],
      indexRunId: '6'.repeat(64),
      nextAfterNameHex: null,
      snapshotId: '4'.repeat(64),
    },
    status: 'available',
  },
};

const idleDeepMapStatus: DeepMapStatusResponseV1 = {
  protocolVersion: 1,
  result: {
    status: 'available',
    configuration: {
      model: {
        profileId: '5'.repeat(64),
        profileVersion: 1,
        providerId: 'ollama',
        modelId: 'mapper:latest',
        contextTokens: 16_384,
        outputTokens: 2_048,
      },
      minimumBudget: { tokenLimit: 1, timeLimitMillis: 1, toolCallLimit: 1 },
      defaultBudget: { tokenLimit: 32_000, timeLimitMillis: 120_000, toolCallLimit: 64 },
      maximumBudget: {
        tokenLimit: 1_000_000,
        timeLimitMillis: 86_400_000,
        toolCallLimit: 4_096,
      },
    },
    activity: {
      state: 'idle',
      budget: null,
      progress: null,
      confirmedSteps: '0',
      totalSteps: '0',
    },
  },
};

const removedProject: RemoveProjectResponseV1 = {
  protocolVersion: 1,
  result: { retainedPrivateStorage: true, status: 'removed' },
};

const recentProjects: RecentProjectsResponseV1 = {
  projects: [
    {
      project: projectSummary,
      projectId: '3'.repeat(64),
    },
  ],
  protocolVersion: 1,
};

describe('A^3 desktop shell', () => {
  it('shows the exact product identity and mapped health state', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    expect(screen.getByRole('heading', { level: 1, name: 'A^3' })).toBeTruthy();
    expect(screen.getByText('Autonomous Agent Assistant')).toBeTruthy();

    await waitFor(() => {
      expect(screen.getByText('Bereit')).toBeTruthy();
    });

    expect(screen.getByText('0.1.0')).toBeTruthy();
    expect(screen.getByText('V1')).toBeTruthy();
    expect(screen.getByText('windows')).toBeTruthy();
  });

  it('shows live Fast-Index phase progress while keeping the published snapshot readable', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        indexActivityLoader: async () => runningIndexActivity,
        indexOverviewLoader: async () => publishedIndexOverview,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    expect(await screen.findByText('Phase 4 von 6: Beziehungen verknüpfen')).toBeTruthy();
    expect(
      screen.getByText(
        'Der zuletzt veröffentlichte Snapshot bleibt während dieses Laufs vollständig lesbar.',
      ),
    ).toBeTruthy();
    expect(screen.getByRole('progressbar', { name: 'Fast-Index-Fortschritt' })).toHaveProperty(
      'value',
      3,
    );
    expect(screen.getByRole('heading', { name: 'Veröffentlichter Fast Index' })).toBeTruthy();
    expect(screen.getAllByText(/80,00\s%/)).toHaveLength(2);
    expect(screen.getByText('src/lib.rs')).toBeTruthy();
    expect(screen.getByText(/Syntaxfehler · syntax error/)).toBeTruthy();
  });

  it('shows authoritative Stale and NeedsReview Module Card counts with causes', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleCardFreshnessLoader: async () => moduleCardFreshness,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Module-Card-Aktualität' })).toBeTruthy();
    expect(screen.getByText('Stale')).toBeTruthy();
    expect(screen.getByText('NeedsReview')).toBeTruthy();
    expect(screen.getByText(/Direkte Evidenz geändert · 2/)).toBeTruthy();
    expect(screen.getByText(/Direkte Abhängigkeit geändert · 1/)).toBeTruthy();
  });

  it('navigates the bounded published repository tree one directory at a time', async () => {
    const repositoryTreeLoader = vi.fn(async (query: { directoryPathHex: string | null }) =>
      query.directoryPathHex === null ? repositoryTreeRoot : repositoryTreeSrc,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
        repositoryTreeLoader,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Repository-Baum' })).toBeTruthy();
    expect(await screen.findByText('README.md')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Verzeichnis src öffnen' }));

    expect(await screen.findByText('lib.rs')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'src' }).getAttribute('aria-current')).toBe('page');
    expect(repositoryTreeLoader).toHaveBeenCalledWith({
      afterNameHex: null,
      directoryPathHex: '737263',
      limit: 50,
    });
  });

  it('navigates only direct primary modules while exposing graph communities as a count', async () => {
    const moduleTreeLoader = vi.fn(async (query: { parentModuleId: string | null }) =>
      query.parentModuleId === null ? moduleTreeRoot : moduleTreeRepository,
    );
    render(App, {
      props: {
        healthLoader: async () => health,
        moduleTreeLoader,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    expect(await screen.findByRole('heading', { name: 'Modulbaum' })).toBeTruthy();
    expect(await screen.findByRole('button', { name: 'Modul Repository öffnen' })).toBeTruthy();
    expect(screen.getByText('Graph-Communities')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Graph-Community/ })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Modul Repository öffnen' }));

    expect(await screen.findByText('tools')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Repository' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(moduleTreeLoader).toHaveBeenCalledWith({
      afterModuleId: null,
      limit: 50,
      parentModuleId: 'a'.repeat(64),
    });
  });

  it('shows verified model and budgets without starting Deep Map until the explicit click', async () => {
    const deepMapStarter = vi.fn<
      (budget: {
        tokenLimit: number;
        timeLimitMillis: number;
        toolCallLimit: number;
      }) => Promise<DeepMapControlResponseV1>
    >(async () => ({ accepted: true, protocolVersion: 1 }));
    render(App, {
      props: {
        deepMapStarter,
        deepMapStatusLoader: async () => idleDeepMapStatus,
        healthLoader: async () => health,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    expect(await screen.findByText('ollama / mapper:latest')).toBeTruthy();
    expect(screen.getByText('Bereit für einen bewussten Start')).toBeTruthy();
    expect(deepMapStarter).not.toHaveBeenCalled();
    expect((screen.getByLabelText('Tokenbudget') as HTMLInputElement).valueAsNumber).toBe(32_000);
    expect(
      (screen.getByLabelText('Zeitbudget in Millisekunden') as HTMLInputElement).valueAsNumber,
    ).toBe(120_000);
    expect(
      (screen.getByLabelText('Read-only-Werkzeugaufrufe') as HTMLInputElement).valueAsNumber,
    ).toBe(64);

    await fireEvent.click(screen.getByRole('button', { name: 'Deep Map bewusst starten' }));

    await waitFor(() => {
      expect(deepMapStarter).toHaveBeenCalledWith({
        tokenLimit: 32_000,
        timeLimitMillis: 120_000,
        toolCallLimit: 64,
      });
    });
  });

  it('shows a safe error and supports retry', async () => {
    const healthLoader = vi
      .fn<() => Promise<HealthResponseV1>>()
      .mockRejectedValueOnce(new Error('sensitive internal detail'))
      .mockResolvedValueOnce(health);

    render(App, {
      props: {
        healthLoader,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Die Health-Abfrage ist fehlgeschlagen.');
    expect(alert.textContent).not.toContain('sensitive internal detail');

    await fireEvent.click(screen.getByRole('button', { name: 'Erneut prüfen' }));

    await waitFor(() => {
      expect(screen.getByText('Bereit')).toBeTruthy();
    });
    expect(healthLoader).toHaveBeenCalledTimes(2);
  });

  it('persists a project after explicit interaction and refreshes the recent list', async () => {
    const projectOpener = vi.fn(async () => openedProject);
    const recentProjectsLoader = vi
      .fn<() => Promise<RecentProjectsResponseV1>>()
      .mockResolvedValueOnce(emptyRecentProjects)
      .mockResolvedValueOnce(recentProjects);
    const projectStatusLoader = vi
      .fn<() => Promise<ProjectStatusResponseV1>>()
      .mockResolvedValueOnce(noProjectStatus)
      .mockResolvedValueOnce(activeProjectStatus)
      .mockResolvedValueOnce(queuedRebuildStatus);
    const projectRebuilder = vi.fn<() => Promise<RebuildProjectIndexResponseV1>>(async () => ({
      protocolVersion: 1,
      state: 'queued',
    }));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectRebuilder,
        projectStatusLoader,
        recentProjectsLoader,
      },
    });

    expect(projectOpener).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));

    await waitFor(() => {
      expect(screen.getByText('Worktree sicher geöffnet')).toBeTruthy();
      expect(screen.getAllByText('C:\\worktree')).toHaveLength(2);
      expect(screen.getAllByText('main (unborn)')).toHaveLength(2);
      expect(screen.getByText('Veröffentlicht')).toBeTruthy();
      expect(screen.getByText(/Generation 2/)).toBeTruthy();
      expect(screen.getByText('4.096 Bytes')).toBeTruthy();
    });
    expect(projectOpener).toHaveBeenCalledTimes(1);
    expect(recentProjectsLoader).toHaveBeenCalledTimes(2);
    expect(
      screen.getByText(/Quellcode, Snapshots, Aufgaben, Entscheidungen und User-Evidence bleiben/),
    ).toBeTruthy();

    await fireEvent.click(
      screen.getByRole('button', { name: 'Regenerierbaren Index neu aufbauen' }),
    );
    await waitFor(() => expect(screen.getByText('Rebuild wartet')).toBeTruthy());
    expect(projectRebuilder).toHaveBeenCalledTimes(1);
  });

  it('does not expose project-open adapter details in the UI error', async () => {
    const projectOpener = vi
      .fn<() => Promise<OpenProjectResponseV1>>()
      .mockRejectedValue(new Error('C:\\secret\\repository contains invalid config'));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('erreichbarer Git-Worktree-Root');
    expect(alert.textContent).not.toContain('secret');
  });

  it('shows concrete recovery for a selected path that became unavailable', async () => {
    const projectOpener = vi.fn<() => Promise<OpenProjectResponseV1>>().mockRejectedValue({
      code: 'projectSelectionUnavailable',
      message: 'C:\\secret\\repository disappeared',
      protocolVersion: 1,
    });
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('Prüfe Laufwerk und Zugriffsrechte');
    expect(alert.textContent).toContain('wähle ihn erneut');
    expect(alert.textContent).not.toContain('secret');
  });

  it('keeps recent-project storage details out of the UI and supports retry', async () => {
    const recentProjectsLoader = vi
      .fn<() => Promise<RecentProjectsResponseV1>>()
      .mockRejectedValueOnce(new Error('D:\\private\\catalog.db is corrupt'))
      .mockResolvedValueOnce(recentProjects);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader,
      },
    });

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('lokale Projektliste konnte nicht geladen werden');
    expect(alert.textContent).not.toContain('catalog.db');

    await fireEvent.click(screen.getByRole('button', { name: 'Erneut laden' }));
    await waitFor(() => {
      expect(screen.getByText('C:\\worktree')).toBeTruthy();
    });
    expect(recentProjectsLoader).toHaveBeenCalledTimes(2);
  });

  it('renders linked worktrees that share one catalog project identity', async () => {
    const linkedRecentProjects: RecentProjectsResponseV1 = {
      projects: [
        recentProjects.projects[0],
        {
          project: {
            ...projectSummary,
            worktreeId: '4'.repeat(64),
            worktreeRootDisplay: 'C:\\linked-worktree',
          },
          projectId: recentProjects.projects[0].projectId,
        },
      ],
      protocolVersion: 1,
    };

    render(App, {
      props: {
        healthLoader: async () => health,
        projectStatusLoader: async () => noProjectStatus,
        recentProjectsLoader: async () => linkedRecentProjects,
      },
    });

    expect(await screen.findByText('C:\\worktree')).toBeTruthy();
    expect(screen.getByText('C:\\linked-worktree')).toBeTruthy();
  });

  it('requires explicit confirmation and explains non-destructive project removal', async () => {
    const projectRemover = vi.fn(async () => removedProject);
    const recentProjectsLoader = vi
      .fn<() => Promise<RecentProjectsResponseV1>>()
      .mockResolvedValueOnce(recentProjects)
      .mockResolvedValueOnce(emptyRecentProjects);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectRemover,
        projectStatusLoader: async () => activeProjectStatus,
        recentProjectsLoader,
      },
    });

    const removeButton = await screen.findByRole('button', { name: 'Nur aus A^3 entfernen' });
    expect(
      screen.getByText(/Repository-Dateien werden nie gelöscht.*Private A\^3-Daten bleiben/s),
    ).toBeTruthy();
    await fireEvent.click(removeButton);
    expect(projectRemover).not.toHaveBeenCalled();
    expect(screen.getByText(/Der lokale Worktree bleibt vollständig bestehen/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Entfernen bestätigen' }));

    await waitFor(() => {
      expect(screen.getByText(/Worktree aus der A\^3-Projektliste entfernt/)).toBeTruthy();
      expect(screen.getByText('Noch keine Projekte gespeichert.')).toBeTruthy();
    });
    expect(projectRemover).toHaveBeenCalledTimes(1);
    expect(recentProjectsLoader).toHaveBeenCalledTimes(2);
  });
});
