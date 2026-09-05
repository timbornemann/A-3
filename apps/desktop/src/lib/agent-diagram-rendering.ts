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

/**
 * Makes flowchart edge labels emitted by the original V32 compiler compatible with Mermaid 11.
 *
 * Older persisted artifacts used an unquoted `-->|label|` form. Parentheses and a few other
 * perfectly ordinary characters are parsed as flowchart syntax in that position. The Core has
 * always emitted numeric aliases and encoded literal pipes, so this deliberately narrow rewrite
 * can quote only those legacy compiler lines without interpreting arbitrary Mermaid input.
 */
export function prepareMermaidForRendering(source: string): string {
  if (!source.startsWith('flowchart TD\n')) return source;
  return source.replace(/^(\s*n\d+\s+-->\|)(?!")([^|\r\n]+)(\|\s+n\d+\s*)$/gmu, '$1"$2"$3');
}
