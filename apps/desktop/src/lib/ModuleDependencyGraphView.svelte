<script lang="ts">
  import type {
    ModuleDependencyEdgeEvidenceV1,
    ModuleDependencyGraphV1,
    ModuleDependencyNodeV1,
    ModuleDependencyRelationV1,
  } from './module-dependency-graph';

  interface Props {
    graph: ModuleDependencyGraphV1;
    onClearEvidence: () => void;
    onSelectEvidence: (evidence: ModuleDependencyEdgeEvidenceV1) => void;
    selectedEvidence: ModuleDependencyEdgeEvidenceV1 | null;
  }

  let { graph, onClearEvidence, onSelectEvidence, selectedEvidence }: Props = $props();
  let centerNode = $derived(graph.nodes.find((node) => node.moduleId === graph.centerModuleId));

  function relationLabel(relation: ModuleDependencyRelationV1): string {
    const labels: Record<ModuleDependencyRelationV1, string> = {
      builds: 'baut',
      calls: 'ruft auf',
      configures: 'konfiguriert',
      documents: 'dokumentiert',
      exports: 'exportiert nach',
      extends: 'erweitert',
      implements: 'implementiert',
      imports: 'importiert',
      reads: 'liest',
      tests: 'testet',
      writes: 'schreibt',
    };
    return labels[relation];
  }

  function nodeName(moduleId: string): string {
    return graph.nodes.find((node) => node.moduleId === moduleId)?.name ?? moduleId.slice(0, 12);
  }

  function nodeKind(node: ModuleDependencyNodeV1): string {
    return node.kind === 'manifestBoundary' ? 'Manifest-Grenze' : 'Pfad-Grenze';
  }

  function countLabel(value: string): string {
    return new Intl.NumberFormat('de-DE').format(BigInt(value));
  }

  function percentageLabel(value: number | null): string {
    return value === null
      ? 'Keine strukturellen Parserdaten'
      : new Intl.NumberFormat('de-DE', {
          maximumFractionDigits: 2,
          minimumFractionDigits: 2,
          style: 'percent',
        }).format(value / 10_000);
  }

  function pathDisplayFromHex(pathHex: string): string {
    const bytes = new Uint8Array(pathHex.length / 2);
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Number.parseInt(pathHex.slice(index * 2, index * 2 + 2), 16);
    }
    return Array.from(new TextDecoder().decode(bytes))
      .slice(0, 256)
      .map((character) => {
        const codePoint = character.codePointAt(0);
        return codePoint !== undefined &&
          (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f))
          ? '�'
          : character;
      })
      .join('');
  }
</script>

<p class="index-snapshot">
  Indexlauf <code>{graph.indexRunId}</code>
</p>
<dl class="module-tree-summary dependency-summary">
  <div>
    <dt>Beobachtete Nachbarn</dt>
    <dd>{countLabel(graph.observedNeighborCount)}{graph.nodesTruncated ? '+' : ''}</dd>
  </div>
  <div>
    <dt>Relationsgruppen</dt>
    <dd>{countLabel(graph.observedEdgeGroupCount)}{graph.edgesTruncated ? '+' : ''}</dd>
  </div>
  <div>
    <dt>Inspizierte Kanten</dt>
    <dd>{countLabel(graph.inspectedEdgeCount)}{graph.sourceEdgesTruncated ? '+' : ''}</dd>
  </div>
  <div>
    <dt>Nicht zugeordnet</dt>
    <dd>{countLabel(graph.unmappedEdgeCount)}</dd>
  </div>
</dl>
{#if graph.sourceEdgesTruncated || graph.nodesTruncated || graph.edgesTruncated || graph.unmappedEdgeCount !== '0'}
  <div class="dependency-boundary-note" role="note">
    <strong>Begrenzter Ausschnitt.</strong>
    {#if graph.sourceEdgesTruncated}
      Weitere Graphkanten liegen hinter der 4.096-Kanten-Grenze.
    {/if}
    {#if graph.nodesTruncated}
      Weitere beobachtete Module sind nicht gerendert.
    {/if}
    {#if graph.edgesTruncated}
      Weitere Relationsgruppen der sichtbaren Module sind ausgeblendet.
    {/if}
    {#if graph.unmappedEdgeCount !== '0'}
      {countLabel(graph.unmappedEdgeCount)} inspizierte Kanten besitzen keinen eindeutig zuordenbaren
      Modulendpunkt.
    {/if}
  </div>
{/if}
<div class="module-dependency-graph" aria-label="Begrenzter Modulabhängigkeitsgraph">
  {#if centerNode !== undefined}
    <div class="dependency-center-node">
      <span>Zentrum</span>
      <strong>{centerNode.name}{centerNode.nameTruncated ? '…' : ''}</strong>
      <small>{nodeKind(centerNode)}</small>
    </div>
  {/if}
  {#if graph.edges.length === 0}
    <p class="ready-label">Keine zugeordneten direkten Modulabhängigkeiten beobachtet.</p>
  {:else}
    <ol class="dependency-edge-list">
      {#each graph.edges as edge (edge.sourceModuleId + edge.targetModuleId + edge.relation)}
        <li>
          <div class="dependency-relation">
            <strong>{nodeName(edge.sourceModuleId)}</strong>
            <span>{relationLabel(edge.relation)}</span>
            <strong>{nodeName(edge.targetModuleId)}</strong>
          </div>
          <span>{countLabel(edge.observedEvidenceCount)} beobachtete Belege</span>
          <button
            type="button"
            aria-label={`Evidence für ${nodeName(edge.sourceModuleId)} ${relationLabel(edge.relation)} ${nodeName(edge.targetModuleId)} anzeigen`}
            aria-pressed={selectedEvidence?.evidenceId === edge.representativeEvidence.evidenceId}
            onclick={() => onSelectEvidence(edge.representativeEvidence)}
          >
            Evidence anzeigen
          </button>
        </li>
      {/each}
    </ol>
  {/if}
  <ul class="dependency-node-list" aria-label="Gerenderte Module">
    {#each graph.nodes as node (node.moduleId)}
      <li class:dependency-node-center={node.moduleId === graph.centerModuleId}>
        <strong>{node.name}{node.nameTruncated ? '…' : ''}</strong>
        <span>{nodeKind(node)}</span>
        {#if node.representativeEvidence !== null}
          <code>{node.representativeEvidence.evidenceId.slice(0, 12)}</code>
        {:else}
          <span>Kein struktureller Repräsentant</span>
        {/if}
      </li>
    {/each}
  </ul>
</div>
{#if selectedEvidence !== null}
  <aside class="dependency-evidence" aria-labelledby="dependency-evidence-heading">
    <div>
      <h5 id="dependency-evidence-heading">Repräsentative Graph-Evidence</h5>
      <button type="button" onclick={onClearEvidence}>Schließen</button>
    </div>
    <dl>
      <div>
        <dt>Evidence-ID</dt>
        <dd><code>{selectedEvidence.evidenceId}</code></dd>
      </div>
      <div>
        <dt>Aktuelle Revision</dt>
        <dd>
          <code>{pathDisplayFromHex(selectedEvidence.pathHex)}</code>
          · {selectedEvidence.contentHash.slice(0, 12)}
        </dd>
      </div>
      <div>
        <dt>Bereich</dt>
        <dd>
          Bytes {selectedEvidence.range.startByte}–{selectedEvidence.range.endByte} · Zeile
          {selectedEvidence.range.start.row + 1}
        </dd>
      </div>
      <div>
        <dt>Confidence</dt>
        <dd>{percentageLabel(selectedEvidence.confidenceBasisPoints)}</dd>
      </div>
    </dl>
  </aside>
{/if}
