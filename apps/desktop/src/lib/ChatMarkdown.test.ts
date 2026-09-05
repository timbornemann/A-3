import { render } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import ChatMarkdown from './ChatMarkdown.svelte';
import * as markdown from './chat-markdown';

afterEach(() => vi.restoreAllMocks());

it('parses published text once while source projections and callbacks update', async () => {
  const parse = vi.spyOn(markdown, 'parseChatMarkdown');
  const view = render(ChatMarkdown, {
    text: '## Antwort\n\nUnveränderter Text',
    sources: [],
    onsource: () => {},
  });
  const paragraph = view.container.querySelector('p');
  for (let poll = 0; poll < 20; poll += 1)
    await view.rerender({
      text: '## Antwort\n\nUnveränderter Text',
      sources: [],
      onsource: () => {},
    });
  expect(parse).toHaveBeenCalledOnce();
  expect(view.container.querySelector('p')).toBe(paragraph);
  await view.rerender({ text: 'Tatsächlich neuer Text' });
  expect(parse).toHaveBeenCalledTimes(2);
});
