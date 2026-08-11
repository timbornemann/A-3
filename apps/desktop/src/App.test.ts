import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { HealthResponseV1 } from './lib/health';
import type { IndexActivityResponseV1 } from './lib/index-activity';
import type { OpenProjectResponseV1, ProjectSummaryV1 } from './lib/project';
import type { RebuildProjectIndexResponseV1 } from './lib/project-rebuild';
import type { RemoveProjectResponseV1 } from './lib/project-removal';
import type { ProjectStatusResponseV1 } from './lib/project-status';
import type { RecentProjectsResponseV1 } from './lib/recent-projects';

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
