// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

/**
 * Markdown helpers of the editor: the server's raw-HTML rule, and the pure text
 * edits behind the toolbar buttons.
 */

/**
 * First raw HTML tag of `markdown`, or null. Mirrors `find_html` in
 * crates/core/src/validate.rs: lowercase tags, `<!…` and `<?…` are HTML;
 * capitalized MDX components, fragments, autolinks and code are not.
 */
export function findHtml(markdown: string): string | null {
  let fence: { char: string; run: number } | null = null;
  for (const line of markdown.split(/\r?\n/)) {
    const trimmed = line.trimStart();
    const c = trimmed[0];
    if (c === "`" || c === "~") {
      const run = runLength(trimmed, 0, c);
      if (run >= 3) {
        if (!fence) fence = { char: c, run };
        else if (fence.char === c && run >= fence.run && trimmed.slice(run).trim() === "") fence = null;
        continue;
      }
    }
    if (fence) continue;
    const tag = htmlInLine(line);
    if (tag) return tag;
  }
  return null;
}

function runLength(s: string, from: number, c: string): number {
  let i = from;
  while (s[i] === c) i++;
  return i - from;
}

function htmlInLine(line: string): string | null {
  let i = 0;
  while (i < line.length) {
    const c = line[i];
    if (c === "\\") {
      i += 2;
    } else if (c === "`") {
      // Skip a code span: the next run of the same length closes it.
      const run = runLength(line, i, "`");
      let j = i + run;
      let closed: number | null = null;
      while (j < line.length) {
        if (line[j] === "`") {
          const r = runLength(line, j, "`");
          if (r === run) {
            closed = j + r;
            break;
          }
          j += r;
        } else {
          j++;
        }
      }
      i = closed ?? i + run;
    } else if (c === "<") {
      const rest = line.slice(i + 1);
      if (rest.startsWith("!") || rest.startsWith("?")) return `<${rest.slice(0, 8)}`;
      const closing = rest.startsWith("/");
      const name = /^[A-Za-z0-9-]*/.exec(rest.slice(closing ? 1 : 0))![0];
      const after = rest[(closing ? 1 : 0) + name.length];
      if (/^[a-z]/.test(name) && (after === undefined || " \t>/".includes(after))) {
        return `<${closing ? "/" : ""}${name}>`;
      }
      i++;
    } else {
      i++;
    }
  }
  return null;
}

// ---- toolbar edits ----------------------------------------------------------

/** Replace `[start, end)` of the text with `text`, then select `[selStart, selEnd)`. */
export interface Edit {
  start: number;
  end: number;
  text: string;
  selStart: number;
  selEnd: number;
}

/** `**bold**`-style wrapping; unwraps when the selection is already wrapped. */
export function wrap(value: string, start: number, end: number, marker: string, placeholder: string): Edit {
  const m = marker.length;
  if (value.slice(start - m, start) === marker && value.slice(end, end + m) === marker) {
    const inner = value.slice(start, end);
    return { start: start - m, end: end + m, text: inner, selStart: start - m, selEnd: end - m };
  }
  const inner = value.slice(start, end) || placeholder;
  return {
    start,
    end,
    text: `${marker}${inner}${marker}`,
    selStart: start + m,
    selEnd: start + m + inner.length,
  };
}

/** Bounds of the full lines touched by the selection. */
function lineBounds(value: string, start: number, end: number): [number, number] {
  const from = value.lastIndexOf("\n", start - 1) + 1;
  // A selection ending right after a newline does not include the next line.
  const last = end > start && value[end - 1] === "\n" ? end - 1 : end;
  const nl = value.indexOf("\n", last);
  return [from, nl === -1 ? value.length : nl];
}

const HEADING = /^#{1,6} /;
const LIST = /^(?:[-*+] \[[ xX]\] |[-*+] |\d+[.)] )/;

/**
 * Toggle a line prefix (`> `, `- `, `1. `, `## `…) on every selected line.
 * `prefix(i)` gives the prefix of the i-th line (numbered lists).
 */
export function toggleLinePrefix(
  value: string,
  start: number,
  end: number,
  prefix: (i: number) => string,
  kind: "heading" | "list" | "quote",
): Edit {
  const [from, to] = lineBounds(value, start, end);
  const lines = value.slice(from, to).split("\n");
  const existing = kind === "heading" ? HEADING : kind === "list" ? LIST : /^> ?/;
  const all = lines.every((l, i) => l.startsWith(prefix(i)));
  const next = lines.map((l, i) => (all ? l.slice(prefix(i).length) : prefix(i) + l.replace(existing, "")));
  const text = next.join("\n");
  return { start: from, end: to, text, selStart: from, selEnd: from + text.length };
}

/** Insert a block (code block, table, rule) on its own lines. */
export function insertBlock(
  value: string,
  start: number,
  end: number,
  before: string,
  after: string,
  placeholder: string,
): Edit {
  const inner = value.slice(start, end) || placeholder;
  const lead = start === 0 || value.slice(0, start).endsWith("\n\n") ? "" : value[start - 1] === "\n" ? "\n" : "\n\n";
  const trail = value.slice(end).startsWith("\n") ? "" : "\n";
  const text = `${lead}${before}${inner}${after}${trail}`;
  const selStart = start + lead.length + before.length;
  return { start, end, text, selStart, selEnd: selStart + inner.length };
}

/** `[text](url)` or `![alt](url)`: selects the URL when the text was selected. */
export function insertLink(value: string, start: number, end: number, image: boolean, url = "", fallback = ""): Edit {
  const label = value.slice(start, end) || fallback || (image ? "alt text" : "link text");
  const target = url || "https://";
  const text = `${image ? "!" : ""}[${label}](${target})`;
  const urlStart = start + text.length - target.length - 1;
  // Nothing was selected: select the label to type over it; otherwise the URL.
  return start === end && !url
    ? { start, end, text, selStart: start + (image ? 2 : 1), selEnd: start + (image ? 2 : 1) + label.length }
    : { start, end, text, selStart: urlStart, selEnd: urlStart + target.length };
}

export const TABLE_TEMPLATE = "| Column 1 | Column 2 |\n| -------- | -------- |\n| ";
export const TABLE_TAIL = " |          |";
