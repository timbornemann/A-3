import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { HealthResponseV1 } from './lib/health';
import type { OpenProjectResponseV1, ProjectSummaryV1 } from './lib/project';
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

  it('shows a safe error and supports retry', async () => {
    const healthLoader = vi
      .fn<() => Promise<HealthResponseV1>>()
      .mockRejectedValueOnce(new Error('sensitive internal detail'))
      .mockResolvedValueOnce(health);

    render(App, {
      props: { healthLoader, recentProjectsLoader: async () => emptyRecentProjects },
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
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        recentProjectsLoader,
      },
    });

    expect(projectOpener).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));

    await waitFor(() => {
      expect(screen.getByText('Worktree sicher geöffnet')).toBeTruthy();
      expect(screen.getAllByText('C:\\worktree')).toHaveLength(2);
    });
    expect(projectOpener).toHaveBeenCalledTimes(1);
    expect(recentProjectsLoader).toHaveBeenCalledTimes(2);
  });

  it('does not expose project-open adapter details in the UI error', async () => {
    const projectOpener = vi
      .fn<() => Promise<OpenProjectResponseV1>>()
      .mockRejectedValue(new Error('C:\\secret\\repository contains invalid config'));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
        recentProjectsLoader: async () => emptyRecentProjects,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('konnte nicht als sicherer Git-Worktree geöffnet werden');
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
        recentProjectsLoader: async () => linkedRecentProjects,
      },
    });

    expect(await screen.findByText('C:\\worktree')).toBeTruthy();
    expect(screen.getByText('C:\\linked-worktree')).toBeTruthy();
  });
});
