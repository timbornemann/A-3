<script module lang="ts">
  export interface ModelOption {
    kind: ModelProviderKindV1;
    modelId: string;
    label: string;
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import type {
    EmbeddingRoleProfileV2,
    LlmRoleProfileV2,
    ModelProbeInputV1,
    ModelProviderKindV1,
    ModelRoleV1,
  } from './settings';

  interface Props {
    role: ModelRoleV1;
    title: string;
    purpose: string;
    options: ModelOption[];
    profile: LlmRoleProfileV2 | EmbeddingRoleProfileV2 | null;
    busy: boolean;
    cancelling: boolean;
    error: string | null;
    onclose: () => void;
    oncancel: () => Promise<void>;
    onchoose: (option: ModelOption, input: ModelProbeInputV1) => Promise<void>;
  }
  let {
    role,
    title,
    purpose,
    options,
    profile,
    busy,
    cancelling,
    error,
    onclose,
    oncancel,
    onchoose,
  }: Props = $props();
  const PAGE_SIZE = 40;
  const optionKey = (option: { kind: ModelProviderKindV1; modelId: string }): string =>
    JSON.stringify([option.kind, option.modelId]);
  let selectedKey = $state(
    untrack(() =>
      profile ? optionKey({ kind: profile.providerKind, modelId: profile.modelId }) : '',
    ),
  );
  let query = $state('');
  let provider = $state('all');
  let page = $state(0);
  let contextTokens = $state(
    untrack(() => (profile && 'contextTokens' in profile ? profile.contextTokens : 16_384)),
  );
  let outputTokens = $state(
    untrack(() => (profile && 'outputTokens' in profile ? profile.outputTokens : 2_048)),
  );
  let parallelism = $state(
    untrack(() => (profile && 'parallelism' in profile ? profile.parallelism : 1)),
  );
  let batchSize = $state(
    untrack(() => (profile && 'maxBatchSize' in profile ? profile.maxBatchSize : 8)),
  );
  const providers = $derived(
    options.filter(
      (option, index) => options.findIndex((candidate) => candidate.kind === option.kind) === index,
    ),
  );
  const filtered = $derived(
    options.filter(
      (option) =>
        (provider === 'all' || option.kind === provider) &&
        `${option.label} ${option.modelId}`
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase()),
    ),
  );
  const pageCount = $derived(Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)));
  const currentPage = $derived(Math.min(page, pageCount - 1));
  const visibleOptions = $derived(
    filtered.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE),
  );
  const selected = $derived(options.find((option) => optionKey(option) === selectedKey) ?? null);
  const validLimits = $derived(
    role === 'embedding'
      ? Number.isInteger(batchSize) && batchSize >= 1 && batchSize <= 64
      : Number.isInteger(contextTokens) &&
          contextTokens >= 1024 &&
          contextTokens <= 1_048_576 &&
          Number.isInteger(outputTokens) &&
          outputTokens >= 1 &&
          outputTokens <= 262_144 &&
          outputTokens < contextTokens &&
          Number.isInteger(parallelism) &&
          parallelism >= 1 &&
          parallelism <= 64,
  );

  function present(node: HTMLDialogElement): { destroy: () => void } {
    const trigger = document.activeElement;
    if (typeof node.showModal === 'function') node.showModal();
    else node.setAttribute('open', '');
    node.querySelector<HTMLInputElement>('input[type="search"]')?.focus();
    return {
      destroy: () => {
        if (typeof node.close === 'function') node.close();
        if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus();
      },
    };
  }
  async function choose(): Promise<void> {
    if (busy || selected === null || !validLimits) return;
    const input: ModelProbeInputV1 =
      role === 'embedding'
        ? { role, modelId: selected.modelId, maxBatchSize: batchSize }
        : { role, modelId: selected.modelId, contextTokens, outputTokens, parallelism };
    await onchoose(selected, input);
  }
</script>

<dialog
  class="modal-dialog settings-dialog model-picker"
  aria-labelledby="model-picker-heading"
  use:present
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) onclose();
  }}
>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void choose();
    }}
  >
    <header class="modal-heading">
      <div>
        <h3 id="model-picker-heading">{title} · Modell wählen</h3>
        <p>{purpose}</p>
      </div>
      <button type="button" aria-label="Dialog schließen" disabled={busy} onclick={onclose}
        >×</button
      >
    </header>
    <div class="picker-body">
      <div class="picker-filters">
        <label
          >Modelle suchen<input
            type="search"
            placeholder="Name oder Anbieter …"
            bind:value={query}
            disabled={busy}
            oninput={() => {
              page = 0;
            }}
          /></label
        >
        <label
          >Anbieter<select
            bind:value={provider}
            disabled={busy}
            onchange={() => {
              page = 0;
            }}
            ><option value="all">Alle Anbieter</option
            >{#each providers as option (option.kind)}<option value={option.kind}
                >{option.label}</option
              >{/each}</select
          ></label
        >
      </div>
      <div class="picker-count" role="status">
        {filtered.length}
        {filtered.length === 1 ? 'Modell' : 'Modelle'}{query || provider !== 'all'
          ? ' gefunden'
          : ' verfügbar'}
      </div>
      {#key `${query}:${provider}:${currentPage}`}
        <section class="picker-list" aria-label="Verfügbare Modelle">
          {#if visibleOptions.length === 0}
            <p class="picker-empty">
              {options.length === 0
                ? 'Lade zuerst Modelle bei einem aktiven Anbieter.'
                : 'Keine passenden Modelle. Ändere die Suche oder den Anbieter.'}
            </p>
          {:else}
            <fieldset disabled={busy}>
              <legend class="sr-only">Modell auswählen</legend>
              {#each visibleOptions as option (optionKey(option))}
                <label class="picker-option" class:is-selected={selectedKey === optionKey(option)}>
                  <input
                    type="radio"
                    aria-label={`${option.modelId} ${option.label}`}
                    name="model-choice"
                    value={optionKey(option)}
                    bind:group={selectedKey}
                  />
                  <span title={option.modelId}>{option.modelId}</span><small>{option.label}</small>
                </label>
              {/each}
            </fieldset>
          {/if}
        </section>
      {/key}
      <nav class="picker-paging" aria-label="Modellseiten">
        <span>Seite {currentPage + 1} von {pageCount}</span>
        <div>
          <button
            type="button"
            aria-label="Vorherige Modellseite"
            disabled={busy || currentPage === 0}
            onclick={() => {
              page = currentPage - 1;
            }}>←</button
          ><button
            type="button"
            aria-label="Nächste Modellseite"
            disabled={busy || currentPage + 1 >= pageCount}
            onclick={() => {
              page = currentPage + 1;
            }}>→</button
          >
        </div>
      </nav>
      <details class="advanced-model-settings">
        <summary>Erweiterte Limits</summary>
        <fieldset disabled={busy} class="picker-limits">
          <legend class="sr-only">Prüflimits</legend>
          {#if role === 'embedding'}
            <label
              >Maximale Batch-Größe<input
                type="number"
                min="1"
                max="64"
                required
                bind:value={batchSize}
              /></label
            >
          {:else}
            <label
              >Kontext<input
                type="number"
                min="1024"
                max="1048576"
                required
                bind:value={contextTokens}
              /></label
            >
            <label
              >Output<input
                type="number"
                min="1"
                max="262144"
                required
                bind:value={outputTokens}
              /></label
            >
            <label
              >Parallelität<input
                type="number"
                min="1"
                max="64"
                required
                bind:value={parallelism}
              /></label
            >
          {/if}
        </fieldset>
      </details>
      <p class="picker-hint">
        {selected?.kind === 'gemini' || selected?.kind === 'openai'
          ? 'Die Prüfung sendet eine Testanfrage an den gewählten Anbieter und kann Kosten verursachen.'
          : 'A^3 prüft vor dem Speichern, ob das Modell diese Aufgabe unterstützt.'}
      </p>
      {#if error}<p class="picker-error" role="alert">{error}</p>{/if}
      {#if busy}<p role="status" class="picker-hint">
          {cancelling ? 'Abbruch angefordert …' : 'Modell wird geprüft …'}
        </p>{/if}
    </div>
    <div class="picker-footer">
      <div class="picker-selection">
        <span>Auswahl</span><strong
          title={selected ? `${selected.label} · ${selected.modelId}` : undefined}
          >{selected
            ? `${selected.label} · ${selected.modelId}`
            : 'Noch kein Modell gewählt'}</strong
        >
      </div>
      <div class="modal-actions">
        {#if busy}<button type="button" disabled={cancelling} onclick={oncancel}
            >Prüfung abbrechen</button
          >
        {:else}<button type="button" onclick={onclose}>Abbrechen</button><button
            class="primary-action"
            type="submit"
            disabled={!selected || !validLimits}>Auswählen und prüfen</button
          >{/if}
      </div>
    </div>
  </form>
</dialog>

<style>
  .model-picker {
    width: min(calc(100% - 2rem), 42rem);
    max-height: calc(100dvh - 2rem);
    padding: 0;
    overflow: hidden;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .model-picker > form {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    max-height: calc(100dvh - 2rem - 2px);
  }
  .picker-body {
    display: grid;
    align-content: start;
    gap: var(--space-2);
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4);
  }
  .picker-filters {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(8rem, 0.6fr);
    gap: var(--space-3);
  }
  label {
    display: grid;
    gap: var(--space-1);
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  input:not([type='radio']),
  select {
    width: 100%;
    min-width: 0;
    padding-inline: var(--space-3);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    background: var(--color-surface-raised);
  }
  .picker-count,
  .picker-paging,
  .picker-hint {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  .picker-count {
    padding-block: var(--space-1);
  }
  .picker-list {
    height: clamp(9rem, 28dvh, 18rem);
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    background: var(--color-canvas);
  }
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .picker-option {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--control-min-size);
    padding: var(--space-2) var(--space-3);
    color: var(--color-text);
    border-bottom: 1px solid var(--color-border-soft);
    cursor: pointer;
    transition: background 120ms ease;
  }
  .picker-option:hover {
    background: var(--color-surface-raised);
  }
  .picker-option.is-selected {
    background: var(--color-accent-surface);
    box-shadow: inset 2px 0 0 var(--color-accent);
  }
  .picker-option input {
    margin: 0;
    min-height: 0;
    width: 1rem;
    height: 1rem;
    accent-color: var(--color-accent-strong);
  }
  .picker-option span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .picker-option small {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .picker-empty {
    padding: var(--space-4);
    margin: 0;
    color: var(--color-muted);
    line-height: 1.5;
  }
  .picker-paging {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-2);
  }
  .picker-paging > div {
    display: flex;
    gap: var(--space-1);
  }
  .picker-paging button {
    min-width: var(--control-min-size);
  }
  .picker-limits {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .picker-hint,
  .picker-error {
    margin: 0;
    line-height: 1.5;
  }
  .picker-error {
    color: var(--color-danger);
  }
  .picker-footer {
    min-width: 0;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .picker-selection {
    display: grid;
    min-width: 0;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4) 0;
    font-size: var(--font-size-sm);
  }
  .picker-selection span {
    color: var(--color-muted);
  }
  .picker-selection strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .modal-actions {
    border: 0;
  }
  @media (width <= 480px) {
    .picker-filters,
    .picker-limits {
      grid-template-columns: minmax(0, 1fr);
    }
    .picker-option {
      gap: var(--space-2);
    }
    .modal-actions {
      flex-wrap: wrap;
    }
  }
</style>
