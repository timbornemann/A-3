import { describe, expect, it } from 'vitest';
import { parseChatMarkdown } from './chat-markdown';

describe('parseChatMarkdown', () => {
  it('renders structure without producing executable markup', () => {
    expect(
      parseChatMarkdown(
        '## Summary\nText\n\n- Ask bleibt lesend\n- Agent arbeitet\n\n```ts\n<script>x()</script>\n```',
      ),
    ).toEqual([
      { kind: 'heading', level: 2, text: 'Summary' },
      { kind: 'paragraph', text: 'Text' },
      { items: ['Ask bleibt lesend', 'Agent arbeitet'], kind: 'list', ordered: false },
      { kind: 'code', language: 'ts', text: '<script>x()</script>' },
    ]);
  });

  it('keeps an unterminated code fence as inert text', () => {
    expect(parseChatMarkdown('```rust\nfn main() {}')).toEqual([
      { kind: 'code', language: 'rust', text: 'fn main() {}' },
    ]);
  });
});
