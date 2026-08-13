<script lang="ts">
  import type { WorkspaceArea } from './global-status';

  interface Props {
    current: WorkspaceArea;
    onNavigate: (area: WorkspaceArea) => void;
  }

  const entries: ReadonlyArray<{ area: WorkspaceArea; label: string }> = [
    { area: 'projects', label: 'Projects' },
    { area: 'map', label: 'Map' },
    { area: 'agent', label: 'Agent' },
    { area: 'settings', label: 'Settings' },
  ];

  let { current, onNavigate }: Props = $props();
</script>

<nav class="primary-navigation" aria-label="Hauptbereiche">
  <ul>
    {#each entries as entry (entry.area)}
      <li>
        <a
          href={`#${entry.area}`}
          aria-current={current === entry.area ? 'page' : undefined}
          onclick={() => onNavigate(entry.area)}
        >
          {entry.label}
        </a>
      </li>
    {/each}
  </ul>
</nav>
