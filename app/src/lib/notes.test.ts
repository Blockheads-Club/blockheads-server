import { describe, expect, it } from 'vitest';
import { parseNotes, spans } from './notes';

describe('release notes', () => {
  it('splits bold, code and link text into spans', () => {
    expect(spans('a **b** `c` [d](https://e) <i>f</i>')).toEqual([
      { text: 'a ' },
      { text: 'b', bold: true },
      { text: ' ' },
      { text: 'c', code: true },
      { text: ' d f' }
    ]);
    expect(spans('**x**')).toEqual([{ text: 'x', bold: true }]);
    expect(spans('')).toEqual([]);
  });

  it('keeps headings, list items and paragraphs, drops rules and empty lines', () => {
    const md = "## What's new\r\n\r\n- One\n* Two\n1. Three\n---\nPlain text\n<br>\n";
    expect(parseNotes(md).map((b) => [b.kind, b.spans.map((s) => s.text).join('')])).toEqual([
      ['h', "What's new"],
      ['li', 'One'],
      ['li', 'Two'],
      ['li', 'Three'],
      ['p', 'Plain text']
    ]);
    expect(parseNotes('- a\n- b\n- c', 2)).toHaveLength(2);
    expect(parseNotes('')).toEqual([]);
  });
});
