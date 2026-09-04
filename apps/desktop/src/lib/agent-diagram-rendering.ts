import type { MermaidConfig } from 'mermaid';
import type { AgentDiagramExportThemeV1 } from './agent-diagram';

export function mermaidConfig(theme: AgentDiagramExportThemeV1): MermaidConfig {
  return {
    // Mermaid 11 class nodes consult the top-level setting. Keeping it disabled makes every
    // label ordinary SVG text, which survives A^3's deliberate foreignObject removal.
    htmlLabels: false,
    class: { htmlLabels: false },
    flowchart: { htmlLabels: false },
    securityLevel: 'strict',
    startOnLoad: false,
    suppressErrorRendering: true,
    theme: theme === 'dark' ? 'dark' : 'default',
  };
}
