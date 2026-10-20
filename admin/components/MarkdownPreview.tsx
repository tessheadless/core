// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

/** The few mdast fields `remarkMdxPlaceholders` touches. */
interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: Record<string, unknown>;
}

type HastChild =
  { type: "text"; value: string } | { type: "element"; tagName: string; properties: object; children: HastChild[] };

const text = (value: string): HastChild => ({ type: "text", value });
const el = (tagName: string, className: string, children: HastChild[]): HastChild => ({
  type: "element",
  tagName,
  properties: { className },
  children,
});

/**
 * Raw HTML is never rendered (no rehype-raw): `html` nodes become visible
 * placeholders instead. Capitalized tags and fragments are MDX components,
 * rendered by the client apps; lowercase tags are HTML, which the server rejects.
 */
function remarkMdxPlaceholders() {
  const transform = (node: MdNode, block: boolean) => {
    for (const [i, child] of (node.children ?? []).entries()) {
      if (child.type !== "html") {
        transform(child, block && child.type !== "paragraph" && child.type !== "heading");
        continue;
      }
      const raw = child.value ?? "";
      const name = /^<\/?([A-Za-z][\w.:-]*)?/.exec(raw.trimStart())?.[1];
      const mdx = !name || /^[A-Z]/.test(name);
      const label = mdx ? `${name ?? "Fragment"} component` : `HTML not allowed: <${name}>`;
      const tone = mdx ? "border-info text-info" : "border-error text-error";
      const children: HastChild[] = block
        ? [
            el("div", "text-xs font-semibold not-prose", [text(label)]),
            el("pre", "not-prose text-xs whitespace-pre-wrap opacity-80 mt-1", [text(raw)]),
          ]
        : [text(mdx ? raw : label)];
      node.children![i] = {
        type: "mdxPlaceholder",
        data: {
          hName: block ? "div" : "code",
          hProperties: {
            className: block
              ? `not-prose my-3 rounded-box border border-dashed p-3 ${tone}`
              : `border border-dashed px-1 ${tone}`,
            title: mdx ? "MDX component: rendered by your client app" : "Raw HTML is rejected when saving",
          },
          hChildren: children,
        },
      };
    }
  };
  return (tree: MdNode) => transform(tree, true);
}

/** Live preview of a Markdown field (GFM, safe: no raw HTML, sanitized URLs). */
export default function MarkdownPreview({ source }: { source: string }) {
  if (!source.trim()) return <p className="text-base-content/60 text-sm italic">Nothing to preview yet.</p>;
  return (
    <div className="prose prose-sm max-w-none">
      <Markdown
        remarkPlugins={[remarkGfm, remarkMdxPlaceholders]}
        components={{
          a: ({ node: _node, children, ...props }) => (
            <a {...props} target="_blank" rel="noopener noreferrer">
              {children}
            </a>
          ),
          img: ({ node: _node, alt, ...props }) => (
            <img {...props} alt={alt ?? ""} loading="lazy" className="max-h-96" />
          ),
        }}
      >
        {source}
      </Markdown>
    </div>
  );
}
