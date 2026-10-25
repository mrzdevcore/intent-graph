// Bits the panels share: editor links, finding rows, copy buttons and the events components send up.

import { html } from "lit";
import type { ViewData, ViewFinding } from "../data";

/** Sends `name` up to <ig-app>. */
export function emit(from: EventTarget, name: string, detail?: unknown) {
  from.dispatchEvent(new CustomEvent(name, { detail, bubbles: true, composed: true }));
}

/** Editor links: URL schemes become links; javascript:/data:/vbscript: never do. */
export function editorLink(D: ViewData, file: string, line: number) {
  const abs = D.root.replace(/\/$/, "") + "/" + file;
  const target = D.editor.split("{file}").join(abs).split("{line}").join(String(line || 1));
  const m = /^([a-z][a-z0-9+.-]*):/i.exec(target);
  if (m && ["javascript", "data", "vbscript"].indexOf(m[1].toLowerCase()) < 0) return html`<a href=${target}>${file}:${line}</a>`;
  return html`<code title=${target}>${file}:${line}</code>`;
}

/** A finding; clicking it focuses the node it is pinned to. */
export function findingItem(from: EventTarget, D: ViewData, f: ViewFinding) {
  return html`<li class="f" @click=${() => f.pinned && emit(from, "ig-focus", f.pinned)}>
    <span class="sev ${f.severity}">${f.severity}</span><span>${f.message}</span>
    <div class="loc">${f.ruleId} · ${editorLink(D, f.file, f.line)}</div>
  </li>`;
}

export function copyButton(label: string, text: string) {
  const copy = (ev: Event) => {
    const b = ev.target as HTMLButtonElement;
    (navigator.clipboard ? navigator.clipboard.writeText(text) : Promise.reject()).then(
      () => (b.textContent = "Copied"),
      () => window.prompt("Copy:", text),
    );
  };
  return html`<button .textContent=${label} @click=${copy}></button>`;
}
