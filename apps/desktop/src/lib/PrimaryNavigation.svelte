<script lang="ts">
  import type { WorkspaceArea } from './global-status';

  interface Props {
    current: WorkspaceArea;
    onNavigate: (area: WorkspaceArea) => void;
  }

  const entries: ReadonlyArray<{ area: WorkspaceArea; label: string }> = [
    { area: 'projects', label: 'Projects' },
    { area: 'map', label: 'Map' },
    { area: 'flows', label: 'Abläufe' },
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
          aria-label={entry.label}
          onclick={() => onNavigate(entry.area)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            {#if entry.area === 'projects'}
              <path
                d="M3 6.5h6l1.8 2H21v9.75A1.75 1.75 0 0 1 19.25 20H4.75A1.75 1.75 0 0 1 3 18.25V6.5Z"
              />
              <path d="M3 9h18" />
            {:else if entry.area === 'map'}
              <circle cx="6" cy="6" r="2" />
              <circle cx="18" cy="6" r="2" />
              <circle cx="12" cy="18" r="2" />
              <path d="m7.8 7 3.1 8.9M16.2 7l-3.1 8.9M8 6h8" />
            {:else if entry.area === 'flows'}
              <path d="M5 5h5v4H5zM14 15h5v4h-5zM7.5 9v8H14M10 7h7v8" />
            {:else if entry.area === 'agent'}
              <path d="M5 5.5h14v13H5z" />
              <path d="m8.5 10 2 2-2 2M13 14h3" />
            {:else}
              <path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 5v4M8 15v4" />
            {/if}
          </svg>
          <span>{entry.label}</span>
        </a>
      </li>
    {/each}
  </ul>
</nav>
