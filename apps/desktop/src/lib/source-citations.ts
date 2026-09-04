import type { AgentWorkTraceSourceV2 } from './agent-ask-research';

export type SourceCitationSegment =
  { kind: 'text'; text: string } | { kind: 'source'; text: string; source: AgentWorkTraceSourceV2 };

export function splitSourceCitations(
  text: string,
  sources: AgentWorkTraceSourceV2[],
): SourceCitationSegment[] {
  const byLabel = new Map(sources.map((source) => [source.referenceLabel, source]));
  const result: SourceCitationSegment[] = [];
  let plain = '';
  let index = 0;
  let inlineDelimiter = 0;
  const flush = (): void => {
    if (plain.length === 0) return;
    result.push({ kind: 'text', text: plain });
    plain = '';
  };
  while (index < text.length) {
    if (text[index] === '`') {
      let width = 1;
      while (text[index + width] === '`') width += 1;
      plain += text.slice(index, index + width);
      inlineDelimiter =
        inlineDelimiter === width ? 0 : inlineDelimiter === 0 ? width : inlineDelimiter;
      index += width;
      continue;
    }
    if (inlineDelimiter === 0 && text.startsWith('【S', index)) {
      const close = text.indexOf('】', index + 2);
      if (close !== -1) {
        const label = text.slice(index + 1, close);
        const source = byLabel.get(label);
        if (source) {
          flush();
          result.push({ kind: 'source', source, text: sourceCitationLabel(source) });
          index = close + 1;
          continue;
        }
      }
    }
    const character = text.codePointAt(index);
    if (character === undefined) break;
    plain += String.fromCodePoint(character);
    index += character > 0xffff ? 2 : 1;
  }
  flush();
  return result;
}

export function sourceCitationLabel(source: AgentWorkTraceSourceV2): string {
  const fileName = source.path.split(/[\\/]/u).at(-1) ?? source.path;
  return `【${source.referenceLabel}】 ${sourceLineLocation(fileName, source)}`;
}

export function sourceCitationAccessibleName(source: AgentWorkTraceSourceV2): string {
  if (source.startLine === null) return `Quelle ${source.referenceLabel}: ${source.path} öffnen`;
  if (source.endLine === null || source.startLine === source.endLine)
    return `Quelle ${source.referenceLabel}: ${source.path}, Zeile ${source.startLine} öffnen`;
  return `Quelle ${source.referenceLabel}: ${source.path}, Zeilen ${source.startLine} bis ${source.endLine} öffnen`;
}

function sourceLineLocation(path: string, source: AgentWorkTraceSourceV2): string {
  if (source.startLine === null) return path;
  if (source.endLine === null || source.startLine === source.endLine)
    return `${path}:${source.startLine}`;
  return `${path}:${source.startLine}–${source.endLine}`;
}
