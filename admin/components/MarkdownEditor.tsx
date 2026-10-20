// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { lazy, Suspense, useDeferredValue, useState, type KeyboardEvent, type ReactNode } from "react";
import { uploadAsset } from "../lib/content";
import { describeError } from "../lib/graphql";
import {
  findHtml,
  insertBlock,
  insertLink,
  TABLE_TAIL,
  TABLE_TEMPLATE,
  toggleLinePrefix,
  wrap,
  type Edit,
} from "../lib/markdown";

// The renderer is only downloaded once a Markdown field is shown.
const MarkdownPreview = lazy(() => import("./MarkdownPreview"));

interface MarkdownEditorProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  required?: boolean;
}

type Tab = "write" | "preview";

/**
 * Two panes: a Markdown textarea with a formatting toolbar (for people who do
 * not know the syntax), and a live preview. Side by side when there is room,
 * Write/Preview tabs otherwise.
 */
export function MarkdownEditor({ id, value, onChange, disabled = false, required = false }: MarkdownEditorProps) {
  // Elements kept in state (not refs): the toolbar handlers are built during render.
  const [textarea, setTextarea] = useState<HTMLTextAreaElement | null>(null);
  const [fileInput, setFileInput] = useState<HTMLInputElement | null>(null);
  const [tab, setTab] = useState<Tab>("write");
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  // Typing stays responsive on long documents: the preview renders at lower priority.
  const preview = useDeferredValue(value);
  const html = findHtml(value);

  /** Apply a toolbar edit through the browser so Ctrl+Z still works. */
  const apply = (make: (value: string, start: number, end: number) => Edit) => {
    const ta = textarea;
    if (!ta || disabled) return;
    const edit = make(ta.value, ta.selectionStart, ta.selectionEnd);
    ta.focus();
    ta.setSelectionRange(edit.start, edit.end);
    // execCommand keeps the native undo stack; setRangeText is the fallback.
    if (!document.execCommand("insertText", false, edit.text)) {
      ta.setRangeText(edit.text, edit.start, edit.end, "end");
      ta.dispatchEvent(new Event("input", { bubbles: true }));
    }
    ta.setSelectionRange(edit.selStart, edit.selEnd);
  };

  const bold = () => apply((v, s, e) => wrap(v, s, e, "**", "bold text"));
  const italic = () => apply((v, s, e) => wrap(v, s, e, "_", "italic text"));
  const link = () => apply((v, s, e) => insertLink(v, s, e, false));
  const pickImage = () => fileInput?.click();

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
    const action = { b: bold, i: italic, k: link }[e.key.toLowerCase()];
    if (action) {
      e.preventDefault();
      action();
    }
  };

  const onImage = async (file: File) => {
    setUploading(true);
    setUploadError(null);
    try {
      const asset = await uploadAsset(file);
      const alt = file.name.replace(/\.[^.]+$/, "");
      // Without a selection, the file name becomes the alt text.
      apply((v, s, e) => insertLink(v, s, e, true, asset.url, alt));
    } catch (e) {
      setUploadError(describeError(e));
    } finally {
      setUploading(false);
    }
  };

  const tools: (ToolProps | "sep")[] = [
    { label: "Bold", shortcut: "Ctrl+B", icon: <b>B</b>, onClick: bold },
    { label: "Italic", shortcut: "Ctrl+I", icon: <i className="font-serif">I</i>, onClick: italic },
    { label: "Strikethrough", icon: <s>S</s>, onClick: () => apply((v, s, e) => wrap(v, s, e, "~~", "struck text")) },
    { label: "Inline code", icon: <code>{"<>"}</code>, onClick: () => apply((v, s, e) => wrap(v, s, e, "`", "code")) },
    "sep",
    ...([1, 2, 3] as const).map((n) => ({
      label: `Heading ${n}`,
      icon: <span className="text-xs font-bold">H{n}</span>,
      onClick: () => apply((v, s, e) => toggleLinePrefix(v, s, e, () => `${"#".repeat(n)} `, "heading")),
    })),
    "sep",
    {
      label: "Bulleted list",
      icon: <ListIcon />,
      onClick: () => apply((v, s, e) => toggleLinePrefix(v, s, e, () => "- ", "list")),
    },
    {
      label: "Numbered list",
      icon: <span className="text-xs font-bold">1.</span>,
      onClick: () => apply((v, s, e) => toggleLinePrefix(v, s, e, (i) => `${i + 1}. `, "list")),
    },
    {
      label: "Task list",
      icon: <span className="text-xs">☑</span>,
      onClick: () => apply((v, s, e) => toggleLinePrefix(v, s, e, () => "- [ ] ", "list")),
    },
    {
      label: "Quote",
      icon: <span className="font-serif text-lg leading-none">“</span>,
      onClick: () => apply((v, s, e) => toggleLinePrefix(v, s, e, () => "> ", "quote")),
    },
    "sep",
    { label: "Link", shortcut: "Ctrl+K", icon: <span>🔗</span>, onClick: link },
    { label: "Image from URL", icon: <span>🖼</span>, onClick: () => apply((v, s, e) => insertLink(v, s, e, true)) },
    {
      label: uploading ? "Uploading…" : "Upload an image",
      icon: uploading ? <span className="loading loading-spinner loading-xs" /> : <span>⤒</span>,
      onClick: pickImage,
      disabled: uploading,
    },
    "sep",
    {
      label: "Code block",
      icon: <span className="font-mono text-xs">{"{ }"}</span>,
      onClick: () => apply((v, s, e) => insertBlock(v, s, e, "```\n", "\n```", "code")),
    },
    {
      label: "Table",
      icon: <span>▦</span>,
      onClick: () => apply((v, s, e) => insertBlock(v, s, e, TABLE_TEMPLATE, TABLE_TAIL, "Cell")),
    },
    {
      label: "Horizontal rule",
      icon: <span>―</span>,
      onClick: () => apply((v, s, e) => insertBlock(v, s, e, "---", "", "")),
    },
  ];

  const tabButton = (t: Tab, label: string) => (
    <button
      type="button"
      role="tab"
      aria-selected={tab === t}
      className={`tab ${tab === t ? "tab-active" : ""}`}
      onClick={() => setTab(t)}
    >
      {label}
    </button>
  );

  return (
    <div className="@container flex flex-col gap-1">
      <div className="border-base-300 rounded-box overflow-hidden border">
        <div role="tablist" className="tabs tabs-border bg-base-200 @2xl:hidden">
          {tabButton("write", "Write")}
          {tabButton("preview", "Preview")}
        </div>
        <div className="grid @2xl:grid-cols-2">
          <div className={`flex min-w-0 flex-col ${tab === "write" ? "" : "hidden @2xl:flex"}`}>
            {!disabled && (
              <div
                role="toolbar"
                aria-label="Formatting"
                aria-controls={id}
                className="bg-base-200 flex flex-wrap items-center gap-0.5 p-1"
              >
                {tools.map((t, i) =>
                  t === "sep" ? (
                    <span key={i} className="bg-base-300 mx-1 h-5 w-px" aria-hidden="true" />
                  ) : (
                    <Tool key={t.label} {...t} />
                  ),
                )}
              </div>
            )}
            <textarea
              ref={setTextarea}
              id={id}
              className="textarea min-h-72 w-full flex-1 resize-y rounded-none border-0 font-mono text-sm focus:outline-none"
              rows={14}
              value={value}
              disabled={disabled}
              required={required}
              spellCheck
              placeholder="Write Markdown here, or use the toolbar above."
              onChange={(e) => onChange(e.target.value)}
              onKeyDown={onKeyDown}
            />
          </div>
          <div
            aria-label="Preview"
            className={`border-base-300 bg-base-100 max-h-[36rem] min-w-0 overflow-auto p-4 @2xl:border-l ${tab === "preview" ? "" : "hidden @2xl:block"}`}
          >
            <div className="text-base-content/50 mb-2 hidden text-xs font-semibold tracking-wide uppercase @2xl:block">
              Preview
            </div>
            <Suspense fallback={<span className="loading loading-dots loading-sm" aria-label="Loading preview" />}>
              <MarkdownPreview source={preview} />
            </Suspense>
          </div>
        </div>
      </div>
      <input
        ref={setFileInput}
        type="file"
        accept="image/*"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = "";
          if (file) void onImage(file);
        }}
      />
      {html && (
        <p className="text-error text-xs" role="alert">
          Raw HTML is not allowed ({html}). Use Markdown syntax or an MDX component (capitalized, e.g. {"<Callout>"}).
        </p>
      )}
      {uploadError && (
        <p className="text-error text-xs" role="alert">
          Image upload failed: {uploadError}
        </p>
      )}
    </div>
  );
}

interface ToolProps {
  label: string;
  icon: ReactNode;
  onClick: () => void;
  shortcut?: string;
  disabled?: boolean;
}

function Tool({ label, icon, onClick, shortcut, disabled }: ToolProps) {
  const title = shortcut ? `${label} (${shortcut})` : label;
  return (
    <button
      type="button"
      className="btn btn-ghost btn-xs btn-square"
      title={title}
      aria-label={title}
      disabled={disabled}
      // Keep the textarea selection: the button must not take the focus on click.
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {icon}
    </button>
  );
}

function ListIcon() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" className="size-4" aria-hidden="true">
      <path
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
        d="M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01"
      />
    </svg>
  );
}
