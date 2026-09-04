import { describe, expect, it } from 'vitest';
import type { AgentWorkTraceSourceV2 } from './agent-ask-research';
import {
  sourceCitationAccessibleName,
  sourceCitationLabel,
  splitSourceCitations,
} from './source-citations';

const source = (label: string, path: string, startLine: number | null, endLine: number | null) =>
  ({
    endLine,
    kind: 'symbol',
    path,
    reason: 'sourceText',
    referenceLabel: label,
    sourceRef: label.replace('S', '').padStart(64, 'a'),
    startLine,
    symbol: null,
    usedForAnswer: true,
  }) satisfies AgentWorkTraceSourceV2;

describe('source citations', () => {
  it('enriches known markers with filename and line range', () => {
    const storage = source('S1', 'taskflow/storage/base.py', 18, 20);
    expect(sourceCitationLabel(storage)).toBe('【S1】 base.py:18–20');
    expect(sourceCitationAccessibleName(storage)).toBe(
      'Quelle S1: taskflow/storage/base.py, Zeilen 18 bis 20 öffnen',
    );
    expect(splitSourceCitations('Gespeichert 【S1】.', [storage])).toEqual([
      { kind: 'text', text: 'Gespeichert ' },
      { kind: 'source', source: storage, text: '【S1】 base.py:18–20' },
      { kind: 'text', text: '.' },
    ]);
  });

  it('leaves unknown and inline-code markers inert', () => {
    const storage = source('S1', 'storage.py', 18, 18);
    expect(splitSourceCitations('`【S1】` und 【S9】', [storage])).toEqual([
      { kind: 'text', text: '`【S1】` und 【S9】' },
    ]);
  });
});
