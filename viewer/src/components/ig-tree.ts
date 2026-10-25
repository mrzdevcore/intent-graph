// The file tree, synced with the graph: a filter, "only what is on screen", folders, files and members.
// Click selects (`ig-tree-select`), double-click focuses (`ig-focus`), a folder click highlights it
// (`ig-tree-folder`).

import { LitElement, html, type PropertyValues } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import type { Index } from "../model/graph";
import { fileTree, onScreenFiles, revealKeys, treeRows, type FileTree, type Row } from "../model/tree";
import { emit } from "./shared";

@customElement("ig-tree")
export class IgTree extends LitElement {
  @property({ attribute: false }) ix!: Index;
  @property({ attribute: false }) mode = "";
  /** The ids of the graph's nodes, after each render. */
  @property({ attribute: false }) screenIds: string[] = [];
  @state() private filter = "";
  @state() private inView = false;
  @state() private sel: string | null = null;
  private expanded: Record<string, boolean> = {};
  private onScreen: Record<string, boolean> | null = null;
  private tree?: FileTree;

  createRenderRoot() {
    return this;
  }

  /** Graph → tree: opens the rows down to `id` and selects it. */
  reveal(id: string) {
    const n = this.ix.byId[id];
    if (!n || n.kind === "system") return;
    revealKeys(this.ix, id).forEach((k) => (this.expanded[k] = true));
    this.sel = id;
    this.requestUpdate();
  }

  protected willUpdate(changed: PropertyValues<this>) {
    if (changed.has("ix")) this.tree = fileTree(this.ix);
    // Off in Architecture (the boxes are whole directories), on elsewhere.
    if (changed.has("mode")) this.inView = this.mode !== "architecture";
    this.onScreen = this.inView ? onScreenFiles(this.ix, this.screenIds) : null;
  }

  protected updated() {
    this.querySelector("#tree-body .sel")?.scrollIntoView({ block: "nearest" });
  }

  private onRow(r: Row) {
    if (r.dir) {
      this.expanded[r.key] = !r.open;
      this.requestUpdate();
      emit(this, "ig-tree-folder", r.key);
      return;
    }
    if (r.parent) this.expanded[r.key] = !r.open;
    this.sel = r.key;
    this.requestUpdate();
    emit(this, "ig-tree-select", r.key);
  }

  private row(r: Row) {
    const sel = !r.dir && r.key === this.sel;
    return html`<div class="row${r.dir ? " dir" : ""}${sel ? " sel" : ""}" role="treeitem" aria-selected=${sel ? "true" : "false"}
        style="padding-left:${8 + r.depth * 12}px" title=${r.title}
        @click=${() => this.onRow(r)} @dblclick=${() => !r.dir && emit(this, "ig-focus", r.key)}>
      <span class="caret">${r.parent ? (r.open ? "▾" : "▸") : ""}</span>
      ${r.status !== "unchanged" ? html`<span class="dot ${r.status}"></span>` : null}
      <span class="name">${r.name}</span>
      ${r.kind ? html`<span class="kind">${r.kind}</span>` : null}
      ${r.findings ? html`<span class="nf">● ${r.findings}</span>` : null}
    </div>`;
  }

  render() {
    if (!this.tree) return null;
    const rows = treeRows(this.ix, this.tree, { filter: this.filter, expanded: this.expanded, onScreen: this.onScreen });
    return html`<input id="tree-filter" type="search" placeholder="Filter files and functions…" aria-label="Filter files and functions"
        autocomplete="off" @input=${(e: Event) => (this.filter = (e.target as HTMLInputElement).value)}>
      <label class="loc"><input id="tree-inview" type="checkbox" .checked=${this.inView}
        @change=${(e: Event) => (this.inView = (e.target as HTMLInputElement).checked)}> only what is on screen</label>
      <div id="tree-body" role="tree">${rows.length ? rows.map((r) => this.row(r)) : html`<p class="loc">  Nothing matches.</p>`}</div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-tree": IgTree;
  }
}
