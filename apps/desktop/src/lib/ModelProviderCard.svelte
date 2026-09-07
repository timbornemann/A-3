<script lang="ts">
  import { onMount } from 'svelte';
  import type { ProviderSettingsV2 } from './settings';

  interface Props {
    slot: ProviderSettingsV2;
    label: string;
    health: string;
    credential: string;
    busy: boolean;
    loading: boolean;
    cancelling: boolean;
    message: string | null;
    catalogCount?: number;
    truncated?: boolean;
    onconfigure: (origin: string) => Promise<void>;
    oncredential: (bytes: Uint8Array) => Promise<void>;
    ondeletecredential: () => Promise<void>;
    ondiscover: () => Promise<void>;
    onenable: () => Promise<void>;
    oncancel: () => Promise<void>;
  }
  let {
    slot,
    label,
    health,
    credential,
    busy,
    loading,
    cancelling,
    message,
    catalogCount,
    truncated = false,
    onconfigure,
    oncredential,
    ondeletecredential,
    ondiscover,
    onenable,
    oncancel,
  }: Props = $props();
  let apiKeyInput = $state<HTMLInputElement | undefined>();
  let hasKey = $state(false);
  const savedOrigin = $derived(slot.endpoint?.origin ?? slot.defaultOrigin);
  let origin = $derived(savedOrigin);
  const needsKey = $derived(slot.providerKind !== 'ollama');
  const canConnect = $derived(
    slot.endpoint !== null && (!needsKey || slot.credential?.status === 'configured'),
  );
  onMount(() => clearKey);

  function clearKey(): void {
    if (apiKeyInput) apiKeyInput.value = '';
    hasKey = false;
  }
  async function saveKey(): Promise<void> {
    if (busy || !apiKeyInput?.value) return;
    const bytes = new TextEncoder().encode(apiKeyInput.value);
    clearKey();
    try {
      await oncredential(bytes);
    } finally {
      bytes.fill(0);
    }
  }
</script>

<article class="connection-card" aria-labelledby={`provider-${slot.providerKind}`}>
  <header>
    <div class="connection-identity">
      <span class="connection-mark" aria-hidden="true">{label.slice(0, 1)}</span>
      <div>
        <strong id={`provider-${slot.providerKind}`}>{label}</strong><span
          >{needsKey ? 'Cloud' : 'Lokal'} · {health}</span
        >
      </div>
    </div>
    <div class="connection-test">
      {#if loading}
        <button type="button" disabled={cancelling} onclick={oncancel}
          >{cancelling ? 'Abbruch angefordert …' : 'Laden abbrechen'}</button
        >
      {:else}
        <button type="button" disabled={busy || !canConnect} onclick={ondiscover}>
          {catalogCount !== undefined
            ? 'Modelle aktualisieren'
            : 'Verbindung testen & Modelle laden'}
        </button>
      {/if}
    </div>
    <button
      type="button"
      class="connection-toggle"
      aria-label={`${label} verwenden`}
      aria-pressed={slot.enabled}
      disabled={busy || (!slot.enabled && slot.connectionVerifiedAtUnixMillis === null)}
      onclick={onenable}
    >
      <span class="toggle-track" aria-hidden="true"><span></span></span>{slot.enabled
        ? 'Aktiv'
        : 'Inaktiv'}
    </button>
  </header>
  <details
    class="connection-details"
    open={slot.endpoint === null || (needsKey && slot.credential?.status !== 'configured')}
    ontoggle={(event) => {
      if (!event.currentTarget.open) clearKey();
    }}
  >
    <summary>
      {slot.endpoint === null ? 'Verbindung einrichten' : 'Verbindung bearbeiten'}
      <span class="connection-catalog" role="status"
        >{catalogCount !== undefined
          ? `${catalogCount} Modelle${truncated ? ' · Katalog gekürzt' : ''}`
          : slot.endpoint === null
            ? 'Adresse erforderlich'
            : !canConnect
              ? 'API-Key erforderlich'
              : 'Katalog noch nicht geladen'}</span
      >
    </summary>
    <div class="connection-fields">
      <form
        onsubmit={(event) => {
          event.preventDefault();
          if (!busy) void onconfigure(origin);
        }}
      >
        <label for={`origin-${slot.providerKind}`}
          >{needsKey ? 'API-Adresse' : 'Serveradresse'}</label
        >
        <div class="connection-field-action">
          <input
            id={`origin-${slot.providerKind}`}
            aria-label={`${label} Adresse`}
            type="url"
            required
            spellcheck="false"
            bind:value={origin}
            disabled={busy}
          />
          <button
            type="submit"
            disabled={busy ||
              !origin.trim() ||
              (slot.endpoint !== null && origin.trim() === slot.endpoint.origin)}
            >Adresse speichern</button
          >
        </div>
      </form>
      {#if needsKey}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void saveKey();
          }}
        >
          <label for={`key-${slot.providerKind}`}>API-Key</label>
          <div class="connection-field-action">
            <input
              id={`key-${slot.providerKind}`}
              aria-label={`${label} API-Key`}
              type="password"
              autocomplete="off"
              spellcheck="false"
              placeholder={slot.credential?.status === 'configured'
                ? '********'
                : 'API-Key eingeben'}
              bind:this={apiKeyInput}
              oninput={(event) => {
                hasKey = event.currentTarget.value.length > 0;
              }}
              disabled={busy || slot.endpoint === null}
            />
            <button type="submit" disabled={busy || !hasKey || slot.endpoint === null}
              >Key speichern</button
            >
          </div>
          <small>{credential}</small>
        </form>
        <p class="connection-note">
          Verbindungstests fragen die Modellliste bei der gespeicherten Adresse ab. Modellprüfungen
          senden eine Testanfrage und können Kosten verursachen.
        </p>
      {:else}<p class="connection-note">
          Ollama muss unter dieser Adresse erreichbar sein. Ein API-Key ist nicht erforderlich.
        </p>{/if}
      <div class="connection-maintenance">
        {#if slot.credential?.status === 'configured'}<button
            type="button"
            disabled={busy}
            onclick={ondeletecredential}>Key löschen</button
          >{/if}
        <button
          type="button"
          disabled={busy || slot.endpoint === null}
          onclick={() => onconfigure('')}>Verbindung zurücksetzen</button
        >
      </div>
    </div>
  </details>

  {#if message}<p class="connection-error" role="alert">{message}</p>{/if}
</article>

<style>
  .connection-card {
    min-width: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-surface);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
  }
  .connection-identity {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    flex: 1;
  }
  .connection-identity > div {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }
  .connection-identity strong {
    color: var(--color-heading);
  }
  .connection-identity span:not(.connection-mark),
  small,
  .connection-note {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  .connection-mark {
    display: grid;
    place-items: center;
    width: 2.5rem;
    height: 2.5rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-heading);
    background: var(--color-surface-raised);
    font-weight: 750;
    flex: 0 0 auto;
  }
  .connection-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    border: 0;
    background: transparent;
    flex-shrink: 0;
  }
  .toggle-track {
    display: flex;
    align-items: center;
    width: 1.9rem;
    padding: 0.2rem;
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    background: var(--color-surface-raised);
  }
  .toggle-track > span {
    width: 0.7rem;
    height: 0.7rem;
    background: var(--color-muted);
    border-radius: 50%;
    transition:
      transform 120ms ease,
      background 120ms ease;
  }
  [aria-pressed='true'] .toggle-track {
    background: var(--color-accent-surface);
    border-color: var(--color-accent-text);
  }
  [aria-pressed='true'] .toggle-track > span {
    background: var(--color-accent-text);
    transform: translateX(0.65rem);
  }
  .connection-details {
    border-top: 1px solid var(--color-border-soft);
  }
  summary {
    min-height: var(--control-min-size);
    align-content: center;
    padding: var(--space-2) var(--space-4);
    color: var(--color-muted);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }
  summary:hover {
    color: var(--color-text);
  }
  .connection-catalog {
    float: right;
    margin-inline-start: var(--space-3);
    font-size: var(--font-size-xs);
    line-height: 1.8;
  }
  .connection-fields {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4) var(--space-4);
  }
  form {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }
  label {
    color: var(--color-text);
    font-size: var(--font-size-sm);
    font-weight: 650;
  }
  .connection-field-action {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 10rem;
    gap: var(--space-2);
  }
  input {
    min-width: 0;
    width: 100%;
    min-height: var(--control-min-size);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-text);
    background: var(--color-canvas);
    font: inherit;
  }
  .connection-note {
    margin: 0;
    line-height: 1.5;
  }
  .connection-maintenance {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .connection-maintenance button {
    color: var(--color-danger);
    background: transparent;
  }
  .connection-error {
    color: var(--color-danger);
    margin: 0;
    padding: 0 var(--space-4) var(--space-4);
    line-height: 1.5;
  }
  @media (width <= 1000px) {
    header {
      flex-wrap: wrap;
    }
    .connection-test {
      order: 1;
      flex-basis: 100%;
    }
    .connection-test button {
      width: 100%;
    }
  }
  @media (width <= 680px) {
    .connection-field-action {
      grid-template-columns: minmax(0, 1fr);
    }
    .connection-catalog {
      float: none;
      display: block;
      margin-inline-start: var(--space-4);
    }
    header {
      gap: var(--space-2);
    }
    .connection-mark {
      display: none;
    }
  }
</style>
