<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import {
    exportAgentDiagram,
    queryAgentDiagramArtifact,
    queryAgentDiagramArtifacts,
    type AgentDiagramArtifactV1,
    type AgentDiagramExportThemeV1,
    type AgentDiagramSummaryV1,
  } from './agent-diagram';
  import { mermaidConfig, prepareMermaidForRendering } from './agent-diagram-rendering';

  interface Props {
    artifactLoader?: typeof queryAgentDiagramArtifact;
    exporter?: typeof exportAgentDiagram;
    listLoader?: typeof queryAgentDiagramArtifacts;
    refreshKey: string;
    sessionId: string;
    summaries?: AgentDiagramSummaryV1[];
    userSequence: string;
    onregenerate?: (summary: AgentDiagramSummaryV1) => void | Promise<void>;
  }

  interface RenderedArtifact {
    artifact: AgentDiagramArtifactV1;
    error: 'runtime' | 'syntax' | null;
    exporting: boolean;
    exportMessage: string | null;
    regenerating: boolean;
    svg: string | null;
  }

  let {
    artifactLoader = queryAgentDiagramArtifact,
    exporter = exportAgentDiagram,
    listLoader = queryAgentDiagramArtifacts,
    refreshKey,
    sessionId,
    summaries,
    userSequence,
    onregenerate,
  }: Props = $props();
  let artifacts = $state<RenderedArtifact[]>([]);
  let theme = $state<AgentDiagramExportThemeV1>('light');
  let loadState = $state<'idle' | 'loading' | 'available' | 'error'>('idle');
  let request = 0;
  const instanceId = $props.id();
  let renderRevision = 0;
  let destroyed = false;
  let loadedInputKey: string | null = null;
  const summariesSignature = $derived(
    summaries === undefined
      ? null
      : JSON.stringify(
          summaries.map((summary) => ({
            artifactRef: summary.artifactRef,
            description: summary.description,
            kind: summary.kind,
            stale: summary.stale,
            title: summary.title,
            userSequence: summary.userSequence,
          })),
        ),
  );

  $effect(() => {
    const providedSummariesSignature = summariesSignature;
    const requestedSession = sessionId;
    const requestedTurn = userSequence;
    const loadKey =
      providedSummariesSignature === null
        ? `${requestedSession}:${requestedTurn}:listed:${refreshKey}`
        : `${requestedSession}:${requestedTurn}:provided:${providedSummariesSignature}`;
    if (loadKey === loadedInputKey) return;
    loadedInputKey = loadKey;
    const requestedSummaries = untrack(() => summaries);
    const current = ++request;
    void load(requestedSession, requestedTurn, current, requestedSummaries);
  });

  onDestroy(() => {
    destroyed = true;
    request += 1;
  });

  async function load(
    requestedSession: string,
    requestedTurn: string,
    current: number,
    requestedSummaries: AgentDiagramSummaryV1[] | undefined,
  ) {
    if (untrack(() => artifacts.length === 0)) loadState = 'loading';
    try {
      const listedArtifacts =
        requestedSummaries === undefined
          ? await listLoader(requestedSession, requestedTurn).then((response) =>
              response.result.kind === 'available' ? response.result.artifacts : [],
            )
          : requestedSummaries;
      if (staleRequest(requestedSession, requestedTurn, current)) return;
      if (listedArtifacts.length === 0) {
        artifacts = [];
        loadState = 'idle';
        return;
      }
      const loaded: RenderedArtifact[] = [];
      for (const summary of listedArtifacts) {
        const detail = await artifactLoader(requestedSession, summary.artifactRef);
        if (staleRequest(requestedSession, requestedTurn, current)) return;
        if (detail.result.kind !== 'available') continue;
        const artifact = detail.result.artifact;
        const retained = artifacts.find(
          (item) =>
            item.artifact.summary.artifactRef === summary.artifactRef &&
            item.artifact.mermaid === artifact.mermaid,
        );
        loaded.push(
          retained
            ? { ...retained, artifact: detail.result.artifact }
            : {
                artifact: detail.result.artifact,
                error: null,
                exporting: false,
                exportMessage: null,
                regenerating: false,
                svg: null,
              },
        );
      }
      artifacts = loaded;
      loadState = loaded.length > 0 ? 'available' : 'idle';
      await renderAll(current, false);
    } catch {
      if (!staleRequest(requestedSession, requestedTurn, current)) loadState = 'error';
    }
  }

  function staleRequest(requestedSession: string, requestedTurn: string, current: number): boolean {
    return (
      destroyed ||
      current !== request ||
      requestedSession !== sessionId ||
      requestedTurn !== userSequence
    );
  }

  async function renderAll(current = request, force = true): Promise<void> {
    if (artifacts.length === 0) return;
    const rendering = ++renderRevision;
    const mermaid = (await import('mermaid')).default;
    if (current !== request || rendering !== renderRevision || destroyed) return;
    mermaid.initialize(mermaidConfig(theme));
    for (let index = 0; index < artifacts.length; index += 1) {
      if (current !== request || rendering !== renderRevision || destroyed) return;
      if (!force && artifacts[index].svg) continue;
      try {
        const rendered = await mermaid.render(
          `a3-diagram-${instanceId.replace(/[^a-zA-Z0-9_-]/gu, '_')}-${rendering}-${index}`,
          prepareMermaidForRendering(artifacts[index].artifact.mermaid),
        );
        if (current !== request || rendering !== renderRevision || destroyed) return;
        const svg = sanitizeSvg(rendered.svg, theme === 'transparent');
        artifacts[index].svg = svg;
        artifacts[index].error = null;
      } catch (error) {
        if (current !== request || rendering !== renderRevision || destroyed) return;
        artifacts[index].svg = null;
        artifacts[index].error = isSyntaxError(error) ? 'syntax' : 'runtime';
      }
    }
  }

  function selectTheme(next: AgentDiagramExportThemeV1): void {
    if (theme === next) return;
    theme = next;
    for (const artifact of artifacts) {
      artifact.svg = null;
      artifact.error = null;
      artifact.exportMessage = null;
    }
    void renderAll();
  }

  async function regenerateArtifact(index: number): Promise<void> {
    const item = artifacts[index];
    if (!onregenerate || item.regenerating) return;
    item.regenerating = true;
    item.exportMessage = null;
    try {
      await onregenerate(item.artifact.summary);
    } catch {
      item.exportMessage = 'Die neue Diagrammerzeugung konnte nicht gestartet werden.';
    } finally {
      item.regenerating = false;
    }
  }

  function isSyntaxError(error: unknown): boolean {
    const message = error instanceof Error ? error.message.toLowerCase() : '';
    return message.includes('parse error') || message.includes('syntax error');
  }

  async function exportArtifact(index: number, format: 'svg' | 'png'): Promise<void> {
    const item = artifacts[index];
    if (!item.svg || item.exporting) return;
    item.exporting = true;
    item.exportMessage = null;
    try {
      const renderedPayload =
        format === 'svg' ? item.svg : await svgToPng(item.svg, theme === 'transparent', theme);
      const response = await exporter({
        artifactRef: item.artifact.summary.artifactRef,
        format,
        renderedPayload,
        sessionId,
        theme,
      });
      item.exportMessage =
        response.result.kind === 'exported'
          ? 'Exportiert.'
          : response.result.kind === 'cancelled'
            ? null
            : response.result.kind === 'invalidPayload'
              ? 'Das gerenderte Diagramm war nicht sicher exportierbar.'
              : response.result.kind === 'notFound'
                ? 'Das Diagramm ist nicht mehr verfügbar.'
                : 'Der Export konnte nicht abgeschlossen werden.';
    } catch {
      item.exportMessage = 'Der Export konnte nicht abgeschlossen werden.';
    } finally {
      item.exporting = false;
    }
  }

  function sanitizeSvg(source: string, transparent: boolean): string {
    const document = new DOMParser().parseFromString(source, 'image/svg+xml');
    const root = document.documentElement;
    if (root.localName !== 'svg' || document.querySelector('parsererror'))
      throw new Error('Invalid SVG');
    for (const element of document.querySelectorAll(
      'script, foreignObject, iframe, object, embed, a, animate, set, image, use, audio, video, canvas',
    ))
      element.remove();
    for (const style of document.querySelectorAll('style')) {
      const value = style.textContent?.toLowerCase() ?? '';
      if (value.includes('@import') || value.includes('expression(') || hasExternalCssUrl(value))
        style.remove();
    }
    for (const element of document.querySelectorAll('*')) {
      for (const attribute of Array.from(element.attributes)) {
        const name = attribute.name.toLowerCase();
        const value = attribute.value.toLowerCase();
        if (
          name.startsWith('on') ||
          name === 'href' ||
          name === 'xlink:href' ||
          name === 'xml:base' ||
          value.includes('javascript:') ||
          value.includes('data:') ||
          hasExternalCssUrl(value)
        )
          element.removeAttribute(attribute.name);
      }
    }
    if (transparent) root.style.backgroundColor = 'transparent';
    return new XMLSerializer().serializeToString(root);
  }

  function hasExternalCssUrl(value: string): boolean {
    return /url\(\s*['"]?(?!#)/iu.test(value);
  }

  async function svgToPng(
    svg: string,
    transparent: boolean,
    selectedTheme: AgentDiagramExportThemeV1,
  ): Promise<string> {
    const blobUrl = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }));
    try {
      const image = new Image();
      image.src = blobUrl;
      await image.decode();
      const naturalWidth = Math.max(1, image.naturalWidth || 1200);
      const naturalHeight = Math.max(1, image.naturalHeight || 800);
      const scale = Math.min(2, 4096 / naturalWidth, 4096 / naturalHeight);
      const width = Math.max(1, Math.floor(naturalWidth * scale));
      const height = Math.max(1, Math.floor(naturalHeight * scale));
      if (width * height > 16_777_216) throw new Error('Diagram too large');
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d');
      if (!context) throw new Error('Canvas unavailable');
      if (!transparent) {
        context.fillStyle = selectedTheme === 'dark' ? '#111827' : '#ffffff';
        context.fillRect(0, 0, width, height);
      }
      context.drawImage(image, 0, 0, width, height);
      return canvas.toDataURL('image/png');
    } finally {
      URL.revokeObjectURL(blobUrl);
    }
  }
</script>

{#if loadState === 'available'}
  <section class="diagram-section" aria-label="Evidenzgebundene Diagramme">
    <header class="diagram-section-header">
      <div><span>Diagramme</span><strong>Aus belegten Quellen erstellt</strong></div>
      <div class="theme-switch" aria-label="Diagramm-Darstellung">
        <button type="button" aria-pressed={theme === 'light'} onclick={() => selectTheme('light')}
          >Hell</button
        >
        <button type="button" aria-pressed={theme === 'dark'} onclick={() => selectTheme('dark')}
          >Dunkel</button
        >
        <button
          type="button"
          aria-pressed={theme === 'transparent'}
          onclick={() => selectTheme('transparent')}>Transparent</button
        >
      </div>
    </header>
    {#each artifacts as item, index (item.artifact.summary.artifactRef)}
      <article class:dark={theme === 'dark'} class="diagram-card">
        <header>
          <div>
            <span>{item.artifact.summary.kind}</span>
            <h4>{item.artifact.summary.title}</h4>
            <p>{item.artifact.summary.description}</p>
          </div>
          {#if item.artifact.summary.stale}<span class="stale">Älterer Projektstand</span>{/if}
        </header>
        {#if item.error}
          <div class="render-error" role="alert">
            <div>
              <strong
                >{item.error === 'syntax'
                  ? 'Die Diagrammbeschreibung konnte nicht sicher dargestellt werden.'
                  : 'Der lokale Diagramm-Renderer konnte nicht abgeschlossen werden.'}</strong
              >
              {#if item.exportMessage}<small>{item.exportMessage}</small>{/if}
            </div>
            <div class="render-error-actions">
              {#if item.error === 'syntax' && onregenerate}
                <button
                  type="button"
                  disabled={item.regenerating}
                  onclick={() => void regenerateArtifact(index)}
                  >{item.regenerating ? 'Wird neu erzeugt …' : 'Diagramm neu erzeugen'}</button
                >
              {:else}
                <button type="button" onclick={() => void renderAll()}>Erneut rendern</button>
              {/if}
            </div>
          </div>
        {:else if item.svg}
          <div class="diagram-canvas" role="img" aria-label={item.artifact.summary.title}>
            <!-- Core-compiled Mermaid is revalidated by IPC and sanitized again above. -->
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html item.svg}
          </div>
          <div class="diagram-actions">
            <button
              type="button"
              disabled={item.exporting}
              onclick={() => void exportArtifact(index, 'svg')}>Als SVG exportieren</button
            >
            <button
              type="button"
              disabled={item.exporting}
              onclick={() => void exportArtifact(index, 'png')}>Als PNG exportieren</button
            >
            {#if item.exportMessage}<span role="status">{item.exportMessage}</span>{/if}
          </div>
        {:else}
          <div class="diagram-loading" role="status">Diagramm wird lokal gerendert …</div>
        {/if}
      </article>
    {/each}
  </section>
{:else if loadState === 'error'}
  <div class="diagram-list-error" role="status">
    Diagramme konnten nicht vollständig geladen werden.
  </div>
{/if}

<style>
  .diagram-section {
    display: grid;
    margin-top: var(--space-3);
    gap: var(--space-3);
  }
  .diagram-section-header,
  .diagram-card > header,
  .diagram-actions {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .diagram-section-header > div:first-child,
  .diagram-card > header > div {
    display: grid;
    gap: 0.15rem;
  }
  .diagram-section-header span,
  .diagram-card header span,
  .stale {
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .theme-switch {
    display: flex;
    padding: 0.15rem;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-surface-subtle);
  }
  button {
    min-height: 2rem;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-control);
    color: var(--color-heading);
    background: var(--color-surface);
    cursor: pointer;
  }
  .theme-switch button {
    border-color: transparent;
    background: transparent;
  }
  .theme-switch button[aria-pressed='true'] {
    border-color: var(--color-border);
    background: var(--color-surface);
  }
  .diagram-card {
    display: grid;
    min-width: 0;
    padding: var(--space-3);
    gap: var(--space-3);
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-card);
    background: var(--color-surface-subtle);
  }
  .diagram-card h4,
  .diagram-card p {
    margin: 0;
  }
  .diagram-card p {
    color: var(--color-muted);
    font-size: var(--font-size-sm);
  }
  .diagram-canvas {
    max-width: 100%;
    min-height: 8rem;
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--color-border-soft);
    border-radius: var(--radius-control);
    background: var(--color-diagram-light-surface);
  }
  .diagram-card.dark .diagram-canvas {
    background: var(--color-diagram-dark-surface);
  }
  .diagram-canvas :global(svg) {
    display: block;
    max-width: none;
    margin: auto;
  }
  .diagram-actions {
    justify-content: flex-start;
  }
  .diagram-actions span {
    align-self: center;
    color: var(--color-muted);
    font-size: var(--font-size-xs);
  }
  .render-error,
  .diagram-loading,
  .diagram-list-error {
    padding: var(--space-3);
    border-inline-start: 3px solid var(--color-warning);
    background: var(--color-surface);
  }
  .render-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .render-error > div:first-child {
    display: grid;
    gap: 0.2rem;
  }
  .render-error small {
    color: var(--color-muted);
  }
  .render-error-actions {
    display: flex;
    gap: var(--space-2);
  }
  @media (max-width: 720px) {
    .diagram-section-header,
    .diagram-card > header {
      display: grid;
    }
    .theme-switch {
      width: fit-content;
    }
  }
</style>
