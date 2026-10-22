/**
 * Release notes from GitHub (Markdown) as plain blocks for the update popup. Only what release notes use is kept:
 * headings, list items and paragraphs, with **bold** and `code` spans. Links keep their text; HTML is dropped. The
 * result is rendered as text nodes, never as HTML.
 */
export interface Span {
  text: string;
  bold?: boolean;
  code?: boolean;
}
export interface Block {
  kind: 'h' | 'li' | 'p';
  spans: Span[];
}

/** `**bold**`, `` `code` `` and `[text](url)` → spans. */
export function spans(line: string): Span[] {
  const clean = line.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/<[^>]*>/g, '');
  const out: Span[] = [];
  const re = /\*\*([^*]+)\*\*|`([^`]+)`/g;
  let at = 0;
  for (const m of clean.matchAll(re)) {
    if (m.index > at) out.push({ text: clean.slice(at, m.index) });
    out.push(m[1] !== undefined ? { text: m[1], bold: true } : { text: m[2], code: true });
    at = m.index + m[0].length;
  }
  if (at < clean.length) out.push({ text: clean.slice(at) });
  return out;
}

/** At most `max` blocks; the popup links to the full notes for the rest. */
export function parseNotes(md: string, max = 12): Block[] {
  const blocks: Block[] = [];
  for (const raw of md.replace(/\r/g, '').split('\n')) {
    const line = raw.trim();
    if (!line || /^(-{3,}|\*{3,}|_{3,})$/.test(line)) continue;
    const h = /^#{1,6}\s+(.*)$/.exec(line);
    const li = /^(?:[-*+]|\d+[.)])\s+(.*)$/.exec(line);
    const block: Block = h ? { kind: 'h', spans: spans(h[1]) } : li ? { kind: 'li', spans: spans(li[1]) } : { kind: 'p', spans: spans(line) };
    if (block.spans.length) blocks.push(block);
  }
  return blocks.slice(0, max);
}
