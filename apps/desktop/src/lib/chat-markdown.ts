export type ChatMarkdownBlock =
  | { kind: 'code'; language: string | null; text: string }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'list'; items: string[]; ordered: boolean }
  | { kind: 'paragraph'; text: string }
  | { kind: 'quote'; text: string };

const MAX_RENDER_CHARACTERS = 65_536;
const MAX_RENDER_LINES = 512;

/** Parses a deliberately small Markdown subset into text-only render instructions. */
export function parseChatMarkdown(text: string): ChatMarkdownBlock[] {
  const blocks: ChatMarkdownBlock[] = [];
  let paragraph: string[] = [];
  let listItems: string[] = [];
  let listOrdered = false;
  let codeLines: string[] | null = null;
  let codeLanguage: string | null = null;

  const flushParagraph = (): void => {
    if (paragraph.length > 0) {
      blocks.push({ kind: 'paragraph', text: paragraph.join('\n') });
      paragraph = [];
    }
  };
  const flushList = (): void => {
    if (listItems.length > 0) {
      blocks.push({ items: listItems, kind: 'list', ordered: listOrdered });
      listItems = [];
    }
  };

  const normalized = text.replaceAll('\r\n', '\n');
  const characterBounded = normalized.slice(0, MAX_RENDER_CHARACTERS);
  const allLines = characterBounded.split('\n');
  const lines = allLines.slice(0, MAX_RENDER_LINES);
  const truncated = normalized.length > characterBounded.length || allLines.length > lines.length;
  for (const line of lines) {
    const trimmed = line.trim();
    if (codeLines !== null) {
      if (trimmed.startsWith('```')) {
        blocks.push({ kind: 'code', language: codeLanguage, text: codeLines.join('\n') });
        codeLines = null;
        codeLanguage = null;
      } else {
        codeLines.push(line);
      }
      continue;
    }
    if (trimmed.startsWith('```')) {
      flushParagraph();
      flushList();
      codeLines = [];
      codeLanguage = trimmed.slice(3).trim() || null;
      continue;
    }
    if (trimmed.length === 0) {
      flushParagraph();
      flushList();
      continue;
    }
    const heading = /^(#{1,4})\s+(.+)$/u.exec(trimmed);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      continue;
    }
    const unordered = /^(?:-|\*)\s+(.+)$/u.exec(trimmed);
    const ordered = /^\d+[.)]\s+(.+)$/u.exec(trimmed);
    if (unordered || ordered) {
      flushParagraph();
      const nextOrdered = ordered !== null;
      if (listItems.length > 0 && listOrdered !== nextOrdered) flushList();
      listOrdered = nextOrdered;
      listItems.push((ordered ?? unordered)?.[1] ?? trimmed);
      continue;
    }
    if (trimmed.startsWith('> ')) {
      flushParagraph();
      flushList();
      blocks.push({ kind: 'quote', text: trimmed.slice(2) });
      continue;
    }
    flushList();
    paragraph.push(line.trimEnd());
  }
  flushParagraph();
  flushList();
  if (codeLines !== null) {
    blocks.push({ kind: 'code', language: codeLanguage, text: codeLines.join('\n') });
  }
  if (truncated) blocks.push({ kind: 'paragraph', text: '… Darstellung sicher gekürzt.' });
  return blocks;
}
