import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { HealthResponseV1 } from './lib/health';

const health: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: 1,
  status: 'ready',
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
});
