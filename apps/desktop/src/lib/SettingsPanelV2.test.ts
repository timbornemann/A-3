import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import SettingsPanel from './SettingsPanel.svelte';
import { layoutModelIds, settingsLayoutFixture } from './settings-layout.fixture';
import type { ModelProviderKindV2, ProviderModelsResponseV2, SettingsResponseV2 } from './settings';

const fixtureCatalog = (
  kind: ModelProviderKindV2,
  response = settingsLayoutFixture(),
): ProviderModelsResponseV2 => ({
  ...response,
  providerKind: kind,
  modelIds: layoutModelIds,
  truncated: false,
});
async function openModels(): Promise<void> {
  await fireEvent.click(await screen.findByRole('button', { name: 'KI & Modelle' }));
  await screen.findByRole('article', { name: 'Ollama' });
}
async function loadModels(name = 'Ollama'): Promise<void> {
  await fireEvent.click(
    within(screen.getByRole('article', { name })).getByRole('button', {
      name: 'Verbindung testen & Modelle laden',
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('article', { name }).textContent).toContain('240 Modelle'),
  );
}
async function chooseCoding(): Promise<HTMLElement> {
  const trigger = within(screen.getByRole('article', { name: 'Coding Agent' })).getByRole(
    'button',
    { name: 'Modell wählen' },
  );
  trigger.focus();
  await fireEvent.click(trigger);
  return screen.getByRole('dialog', { name: 'Coding Agent · Modell wählen' });
}

describe('SettingsPanel multi-provider layout', () => {
  it('keeps four independent cards and compact role rows without automatic discovery', async () => {
    const discover = vi.fn();
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: discover,
    });
    await openModels();
    for (const name of ['Ollama', 'Google Gemini', 'OpenAI', 'OpenAI-kompatibel']) {
      const card = screen.getByRole('article', { name });
      expect(
        within(card)
          .getByRole('button', { name: `${name} verwenden` })
          .getAttribute('aria-pressed'),
      ).toBe('true');
      expect(card.querySelector('details')?.open).toBe(false);
    }
    expect(discover).not.toHaveBeenCalled();
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    expect(
      screen
        .getAllByRole('button', { name: 'Modell wählen' })
        .every((button) => (button as HTMLButtonElement).disabled),
    ).toBe(true);
    const compatible = screen.getByRole('article', { name: 'OpenAI-kompatibel' });
    await fireEvent.click(within(compatible).getByText('Verbindung bearbeiten'));
    expect(
      (within(compatible).getByLabelText('OpenAI-kompatibel Adresse') as HTMLInputElement).value,
    ).toBe('https://openrouter.ai/api/v1');
    expect(within(compatible).getByLabelText('OpenAI-kompatibel API-Key')).toBeTruthy();

    const ollama = screen.getByRole('article', { name: 'Ollama' });
    await fireEvent.click(within(ollama).getByText('Verbindung bearbeiten'));
    expect(ollama.textContent).toContain('Private LAN-Adressen sind erlaubt');
  });

  it('bounds large catalogs, searches and pages locally, and preserves an unambiguous selection', async () => {
    const discover = vi.fn(async (_revision, kind: ModelProviderKindV2) => fixtureCatalog(kind));
    const probe = vi.fn().mockResolvedValue(settingsLayoutFixture());
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: discover,
      roleProberV2: probe,
    });
    await openModels();
    await loadModels();
    await loadModels('Google Gemini');
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    const dialog = await chooseCoding();
    expect(document.activeElement).toBe(within(dialog).getByRole('searchbox'));
    expect(within(dialog).getAllByRole('radio')).toHaveLength(40);
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Nächste Modellseite' }));
    expect(within(dialog).getByRole('radio', { name: 'fixture-model-041 Ollama' })).toBeTruthy();
    expect(within(dialog).queryByRole('radio', { name: 'fixture-model-001 Ollama' })).toBeNull();
    await fireEvent.input(within(dialog).getByRole('searchbox'), {
      target: { value: 'fixture-model-001' },
    });
    expect(within(dialog).getAllByRole('radio')).toHaveLength(2);
    await fireEvent.click(
      within(dialog).getByRole('radio', { name: 'fixture-model-001 Google Gemini' }),
    );
    expect(probe).not.toHaveBeenCalled();
    await fireEvent.input(within(dialog).getByRole('searchbox'), {
      target: { value: 'no-matches' },
    });
    expect(within(dialog).queryAllByRole('radio')).toHaveLength(0);
    expect(dialog.textContent).toContain('Google Gemini · fixture-model-001');
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }));
    expect(probe).toHaveBeenCalledWith('1', 'gemini', {
      role: 'coding',
      modelId: 'fixture-model-001',
      contextTokens: 16384,
      outputTokens: 2048,
      parallelism: 1,
    });
    expect(discover).toHaveBeenCalledTimes(2);
  });

  it('restores focus on Escape and filters providers without starting a probe', async () => {
    const probe = vi.fn();
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: async (_revision, kind) => fixtureCatalog(kind),
      roleProberV2: probe,
    });
    await openModels();
    await loadModels();
    await loadModels('OpenAI');
    const dialog = await chooseCoding();
    await fireEvent.change(within(dialog).getByRole('combobox', { name: 'Anbieter' }), {
      target: { value: 'openai' },
    });
    expect(
      within(dialog)
        .getAllByRole('radio')
        .every((radio) => radio.closest('label')?.textContent?.includes('OpenAI')),
    ).toBe(true);
    await fireEvent(dialog, new Event('cancel', { cancelable: true }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(
      within(screen.getByRole('article', { name: 'Coding Agent' })).getByRole('button', {
        name: 'Modell wählen',
      }),
    );
    expect(probe).not.toHaveBeenCalled();
  });

  it('shows a provider error beside its controls and allows a retry', async () => {
    const discover = vi
      .fn()
      .mockRejectedValueOnce(new Error('private adapter detail'))
      .mockResolvedValueOnce(fixtureCatalog('ollama'));
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: discover,
    });
    await openModels();
    const card = screen.getByRole('article', { name: 'Ollama' });
    await fireEvent.click(
      within(card).getByRole('button', { name: 'Verbindung testen & Modelle laden' }),
    );
    const alert = await within(card).findByRole('alert');
    expect(alert.textContent).not.toContain('private adapter detail');
    expect(
      (
        within(card).getByRole('button', {
          name: 'Verbindung testen & Modelle laden',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    await loadModels();
    expect(within(card).queryByRole('alert')).toBeNull();
    expect(discover).toHaveBeenCalledTimes(2);
  });

  it('keeps a pending probe serialized and cancellable until the original operation settles', async () => {
    let rejectProbe: (reason: Error) => void = () => {};
    const probe = vi.fn(
      () =>
        new Promise<SettingsResponseV2>((_resolve, reject) => {
          rejectProbe = reject;
        }),
    );
    const cancel = vi.fn().mockResolvedValue({ protocolVersion: 1, cancellationRequested: true });
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: async () => fixtureCatalog('ollama'),
      roleProberV2: probe,
      operationCanceller: cancel,
    });
    await openModels();
    await loadModels();
    const dialog = await chooseCoding();
    await fireEvent.click(within(dialog).getAllByRole('radio')[0]);
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }));
    expect((within(dialog).getByRole('searchbox') as HTMLInputElement).disabled).toBe(true);
    await fireEvent(dialog, new Event('cancel', { cancelable: true }));
    expect(screen.getByRole('dialog')).toBe(dialog);
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Prüfung abbrechen' }));
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(probe).toHaveBeenCalledTimes(1);
    expect(
      (within(dialog).getByRole('button', { name: 'Prüfung abbrechen' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    rejectProbe(new Error('cancelled'));
    await within(dialog).findByRole('alert');
    expect(
      (within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
  });

  it('retains saved assignments and their real capability status without a catalog', async () => {
    const response = settingsLayoutFixture();
    response.settings.codingProfile = {
      providerKind: 'gemini',
      modelId: 'saved-model',
      contextTokens: 32768,
      outputTokens: 4096,
      parallelism: 2,
      activation: 'capabilityLimited',
      structuredOutput: 'unavailable',
      toolCallMode: 'disabled',
      profileId: 'a'.repeat(64),
      probedAtUnixMillis: '1788732000000',
    };
    render(SettingsPanel, { settingsLoaderV2: async () => response });
    await openModels();
    const row = screen.getByRole('article', { name: 'Coding Agent' });
    expect(row.textContent).toContain('saved-model');
    expect(row.textContent).toContain('Google Gemini · Capability fehlt');
    expect(row.textContent).not.toContain('Verifiziert');
  });

  it('pairs address and key actions with their inputs and clears one-way credentials', async () => {
    let captured: Uint8Array | undefined;
    const setter = vi.fn(async (_revision, _kind, bytes: Uint8Array) => {
      captured = bytes;
      expect(new TextDecoder().decode(bytes)).toBe('fixture-key');
      return settingsLayoutFixture();
    });
    const configure = vi.fn().mockResolvedValue(settingsLayoutFixture());
    const rendered = render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      providerConfigurerV2: configure,
      credentialSetterV2: setter,
    });
    await openModels();
    const card = screen.getByRole('article', { name: 'Google Gemini' });
    const details = card.querySelector('details')!;
    details.open = true;
    await fireEvent(details, new Event('toggle'));
    const address = within(card).getByLabelText('Google Gemini Adresse');
    const input = within(card).getByLabelText('Google Gemini API-Key') as HTMLInputElement;
    expect(address.closest('form')?.textContent).toContain('Adresse speichern');
    expect(input.closest('form')?.textContent).toContain('Key speichern');
    await fireEvent.input(address, { target: { value: 'https://models.example.test' } });
    await fireEvent.submit(address.closest('form')!);
    expect(configure).toHaveBeenCalledWith('1', 'gemini', 'https://models.example.test');
    await waitFor(() => expect(input.disabled).toBe(false));
    await fireEvent.input(input, { target: { value: 'fixture-key' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(setter).toHaveBeenCalledTimes(1));
    expect(input.value).toBe('');
    expect(captured?.every((byte) => byte === 0)).toBe(true);
    await fireEvent.input(input, { target: { value: 'discard-on-close' } });
    details.open = false;
    await fireEvent(details, new Event('toggle'));
    expect(input.value).toBe('');
    details.open = true;
    await fireEvent(details, new Event('toggle'));
    await fireEvent.input(input, { target: { value: 'discard-on-unmount' } });
    rendered.unmount();
    expect(input.value).toBe('');
  });

  it('retries the V2 settings loader after a load error', async () => {
    const loader = vi
      .fn()
      .mockRejectedValueOnce(new Error('read failed'))
      .mockResolvedValueOnce(settingsLayoutFixture());
    render(SettingsPanel, { settingsLoaderV2: loader });
    await fireEvent.click(await screen.findByRole('button', { name: 'Erneut laden' }));
    await openModels();
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it('retries with the revision persisted by a failed discovery and keeps unrelated catalogs', async () => {
    const refreshed = settingsLayoutFixture();
    refreshed.settings.revision = '2';
    refreshed.settings.providers[0].health = {
      status: 'unreachable',
      checkedAtUnixMillis: '1788732000001',
    };
    const loader = vi
      .fn()
      .mockResolvedValueOnce(settingsLayoutFixture())
      .mockResolvedValue(refreshed);
    const discover = vi
      .fn()
      .mockResolvedValueOnce(fixtureCatalog('gemini'))
      .mockRejectedValueOnce(new Error('failed retest'))
      .mockResolvedValueOnce(fixtureCatalog('ollama', refreshed));
    render(SettingsPanel, { settingsLoaderV2: loader, modelDiscovererV2: discover });
    await openModels();
    await loadModels('Google Gemini');
    const card = screen.getByRole('article', { name: 'Ollama' });
    await fireEvent.click(
      within(card).getByRole('button', { name: 'Verbindung testen & Modelle laden' }),
    );
    await waitFor(() => expect(card.textContent).toContain('Nicht erreichbar'));
    await loadModels();
    expect(discover).toHaveBeenLastCalledWith('2', 'ollama');
    const dialog = await chooseCoding();
    expect(within(dialog).getByRole('option', { name: 'Google Gemini' })).toBeTruthy();
    expect(within(dialog).getByRole('option', { name: 'Ollama' })).toBeTruthy();
    expect(discover).toHaveBeenCalledTimes(3);
  });

  it('keeps other providers selectable when a provider is deactivated', async () => {
    const disabled = settingsLayoutFixture();
    disabled.settings.providers[0].enabled = false;
    const enable = vi.fn().mockResolvedValue(disabled);
    render(SettingsPanel, {
      settingsLoaderV2: async () => settingsLayoutFixture(),
      modelDiscovererV2: async (_revision, kind) => fixtureCatalog(kind),
      providerEnablerV2: enable,
    });
    await openModels();
    await loadModels();
    await loadModels('OpenAI');
    await fireEvent.click(screen.getByRole('button', { name: 'Ollama verwenden' }));
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Ollama verwenden' }).getAttribute('aria-pressed'),
      ).toBe('false'),
    );
    expect(enable).toHaveBeenCalledWith('1', 'ollama', false);
    const dialog = await chooseCoding();
    expect(within(dialog).queryByRole('option', { name: 'Ollama' })).toBeNull();
    expect(within(dialog).getByRole('option', { name: 'OpenAI' })).toBeTruthy();
  });

  it('preserves saved limits and blocks invalid limits before requesting a new probe', async () => {
    const response = settingsLayoutFixture();
    response.settings.codingProfile = {
      providerKind: 'ollama',
      modelId: layoutModelIds[0],
      contextTokens: 32768,
      outputTokens: 4096,
      parallelism: 2,
      activation: 'executable',
      structuredOutput: 'verified',
      toolCallMode: 'disabled',
      profileId: 'a'.repeat(64),
      probedAtUnixMillis: '1788732000000',
    };
    const probe = vi.fn().mockResolvedValue(response);
    render(SettingsPanel, {
      settingsLoaderV2: async () => response,
      modelDiscovererV2: async () => fixtureCatalog('ollama', response),
      roleProberV2: probe,
    });
    await openModels();
    await loadModels();
    await fireEvent.click(
      within(screen.getByRole('article', { name: 'Coding Agent' })).getByRole('button', {
        name: 'Modell ändern',
      }),
    );
    const dialog = screen.getByRole('dialog');
    const limits = dialog.querySelector('details')!;
    limits.open = true;
    await fireEvent(limits, new Event('toggle'));
    expect((within(dialog).getByLabelText('Kontext') as HTMLInputElement).value).toBe('32768');
    expect((within(dialog).getByLabelText('Parallelität') as HTMLInputElement).value).toBe('2');
    await fireEvent.input(within(dialog).getByLabelText('Kontext'), { target: { value: '1024' } });
    expect(
      (within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    await fireEvent.submit(dialog.querySelector('form')!);
    expect(probe).not.toHaveBeenCalled();
    await fireEvent.input(within(dialog).getByLabelText('Output'), { target: { value: '512' } });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Auswählen und prüfen' }));
    expect(probe).toHaveBeenCalledWith('1', 'ollama', {
      role: 'coding',
      modelId: layoutModelIds[0],
      contextTokens: 1024,
      outputTokens: 512,
      parallelism: 2,
    });
  });

  it('offers cancellation and a local status refresh for an already running probe', async () => {
    const active = settingsLayoutFixture();
    active.settings.probeActive = true;
    const loader = vi
      .fn()
      .mockResolvedValueOnce(active)
      .mockResolvedValueOnce(settingsLayoutFixture());
    const cancel = vi
      .fn()
      .mockRejectedValueOnce(new Error('cancel unavailable'))
      .mockResolvedValueOnce({ protocolVersion: 1, cancellationRequested: true });
    render(SettingsPanel, { settingsLoaderV2: loader, operationCanceller: cancel });
    await openModels();
    await fireEvent.click(screen.getByRole('button', { name: 'Prüfung abbrechen' }));
    expect((await screen.findByRole('alert')).textContent).toContain(
      'Abbruch konnte nicht angefordert',
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Prüfung abbrechen' }));
    expect(cancel).toHaveBeenCalledTimes(2);
    await fireEvent.click(screen.getByRole('button', { name: 'Status aktualisieren' }));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Status aktualisieren' })).toBeNull(),
    );
    expect(
      (
        within(screen.getByRole('article', { name: 'Ollama' })).getByRole('button', {
          name: 'Verbindung testen & Modelle laden',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });
});
