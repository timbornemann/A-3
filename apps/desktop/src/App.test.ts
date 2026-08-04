import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { HealthResponseV1 } from './lib/health';
import type { OpenProjectResponseV1 } from './lib/project';

const health: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: 1,
  status: 'ready',
};

const openedProject: OpenProjectResponseV1 = {
  protocolVersion: 1,
  result: {
    project: {
      head: { kind: 'unborn', reference: 'refs/heads/main' },
      repositoryId: '1'.repeat(64),
      worktreeId: '2'.repeat(64),
      worktreeRootDisplay: 'C:\\worktree',
    },
    status: 'opened',
  },
};

describe('A^3 desktop shell', () => {
  it('shows the exact product identity and mapped health state', async () => {
    render(App, {
      props: {
        healthLoader: async () => health,
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

    render(App, { props: { healthLoader } });

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Die Health-Abfrage ist fehlgeschlagen.');
    expect(alert.textContent).not.toContain('sensitive internal detail');

    await fireEvent.click(screen.getByRole('button', { name: 'Erneut prüfen' }));

    await waitFor(() => {
      expect(screen.getByText('Bereit')).toBeTruthy();
    });
    expect(healthLoader).toHaveBeenCalledTimes(2);
  });

  it('opens a project only after explicit interaction and shows the validated identity', async () => {
    const projectOpener = vi.fn(async () => openedProject);
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
      },
    });

    expect(projectOpener).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));

    await waitFor(() => {
      expect(screen.getByText('Worktree sicher geöffnet')).toBeTruthy();
    });
    expect(screen.getByText('C:\\worktree')).toBeTruthy();
    expect(projectOpener).toHaveBeenCalledTimes(1);
  });

  it('does not expose project-open adapter details in the UI error', async () => {
    const projectOpener = vi
      .fn<() => Promise<OpenProjectResponseV1>>()
      .mockRejectedValue(new Error('C:\\secret\\repository contains invalid config'));
    render(App, {
      props: {
        healthLoader: async () => health,
        projectOpener,
      },
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Projektordner auswählen' }));
    const alert = await screen.findByRole('alert');

    expect(alert.textContent).toContain('konnte nicht als sicherer Git-Worktree geöffnet werden');
    expect(alert.textContent).not.toContain('secret');
  });
});
