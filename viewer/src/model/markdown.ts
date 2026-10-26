// The agent's markdown as safe HTML: markdown-it with raw HTML and images off, then DOMPurify
// with web links only. Clicks on links go to the system browser (Chat), never the webview.

import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: false }).disable("image");

export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(md.render(text), {
    FORBID_TAGS: ["img", "style", "form", "input", "button", "iframe"],
    ALLOWED_URI_REGEXP: /^https?:\/\//i,
  });
}

/** Text → a graph node id: an exact id or file (relative, or absolute under `root`, with an optional
 *  `:line`), or, when `code` (inline code), a name exactly one node has. Unique matches only. */
export function nodeMatcher(nodes: { id: string; name: string }[], root: string) {
  const ids = new Set(nodes.map((n) => n.id));
  const names = new Map<string, string | null>();
  for (const n of nodes) names.set(n.name, names.has(n.name) ? null : n.id);
  const prefix = root.replace(/\/$/, "") + "/";
  return (text: string, code = false): string | null => {
    const t = text.trim();
    const p = (t.startsWith(prefix) ? t.slice(prefix.length) : t).replace(/:\d+(:\d+)?$/, "");
    if (ids.has(p)) return p;
    return (code && names.get(t)) || null;
  };
}

/** Graph refs in rendered (sanitized) markdown become buttons with `data-node`: whole inline code,
 *  and words in plain text. Code blocks, links and existing buttons are left alone. DOM nodes only,
 *  never HTML strings. */
export function linkNodes(el: HTMLElement, match: (text: string, code?: boolean) => string | null) {
  const button = (id: string, child: Node) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "node-link";
    b.dataset.node = id;
    b.title = "Show " + id + " in the graph";
    b.append(child);
    return b;
  };
  for (const c of [...el.querySelectorAll("code")]) {
    if (c.closest("pre, a, button")) continue;
    const id = match(c.textContent ?? "", true);
    if (id) c.replaceWith(button(id, c.cloneNode(true)));
  }
  const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  const texts: Text[] = [];
  while (walk.nextNode()) if (!walk.currentNode.parentElement?.closest("pre, code, a, button")) texts.push(walk.currentNode as Text);
  for (const t of texts) {
    const parts = (t.data).split(/(\s+)/);
    if (!parts.some((w) => w.length > 2 && match(trim(w)))) continue;
    const frag = document.createDocumentFragment();
    for (const w of parts) {
      const core = trim(w);
      const id = core && match(core);
      if (!id) {
        frag.append(w);
        continue;
      }
      const at = w.indexOf(core);
      frag.append(w.slice(0, at), button(id, document.createTextNode(core)), w.slice(at + core.length));
    }
    t.replaceWith(frag);
  }
}

/** A word without the punctuation around it: "(a/b.ts," → "a/b.ts". */
const trim = (w: string) => w.replace(/^[("'[{<]+|[)"'\]}>,.;!?]+$/g, "");
