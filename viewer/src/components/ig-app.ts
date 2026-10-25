// The app: header (status, plan, modes, search, counts, breadcrumb, hint), and the state the tree,
// graph and side panel share: mode, Architecture scope, focus, journey.

import { LitElement, html, type PropertyValues } from "lit";
import { customElement, property, query, state } from "lit/decorators.js";
import type { Note, NoteInput, ViewData } from "../data";
import { dropNote, ghosts, pins, type Client } from "../model/notes";
import { architecture, initialScope, rep } from "../model/architecture";
import { blueprint, journeyName, unitOf } from "../model/blueprint";
import { changedIds, exact } from "../model/changed";
import { focusIds } from "../model/focus";
import { ancestors, findNode, index, topMember, type Group, type Index } from "../model/graph";
import "./ig-graph";
import type { IgGraph } from "./ig-graph";
import "./ig-side-panel";
import type { SideView } from "./ig-side-panel";
import "./ig-tree";
import type { IgTree } from "./ig-tree";

const STATE_LABEL = { planned: "Planning", applying: "Applying", done: "Done", plain: "Plan" };

type Mode = "changed" | "architecture" | "focus" | "blueprint";

const MODES: [Mode, string, string][] = [
  ["changed", "Changed", "Changed nodes and their direct neighbours"],
  ["architecture", "Architecture", "Layers, then directories, modules and members. Click a box to open it."],
  ["focus", "Focus", "The neighbourhood of the selected node"],
  ["blueprint", "Blueprint", "A page and everything behind it: front, API, use cases, adapters, systems"],
];

const LINKS_KEY = "ig.links";

function readLinks(): boolean {
  try {
    return localStorage.getItem(LINKS_KEY) !== "0";
  } catch {
    return true;
  }
}

@customElement("ig-app")
export class IgApp extends LitElement {
  @property({ attribute: false }) data?: ViewData;
  @state() private mode: Mode = "changed";
  @state() private scope!: Group;
  @state() private focusId: string | null = null;
  @state() private focusDepth = 1;
  /** Extra start nodes from `D.focus`; cleared when the user focuses on one node. */
  private focusRoots: string[] = [];
  @state() private journey: string | null = null;
  @state() private uiOnly = false;
  @state() private side: SideView = { kind: "overview" };
  @state() private hint = "";
  @state() private screenIds: string[] = [];
  @state() private links = readLinks();
  /** Plan notes, and the API client, when served by `ig serve`. */
  @state() private notes: Note[] = [];
  private api?: Client;
  /** The open right-click menu: a node, or an edge between two nodes. */
  @state() private menu: { x: number; y: number; target: string; to?: string } | null = null;
  @query("ig-graph") private graph!: IgGraph;
  @query("ig-tree") private tree!: IgTree;
  private ix!: Index;
  private lastMode: Mode | null = null;

  createRenderRoot() {
    return this;
  }

  protected willUpdate(changed: PropertyValues<this>) {
    if (changed.has("data") && this.data) {
      this.ix = index(this.data);
      this.mode = this.data.mode;
      this.scope = initialScope(this.ix);
      this.focusRoots = (this.data.focus ?? []).filter((id) => this.ix.byId[id]);
      if (this.focusRoots.length) {
        this.focusId = this.focusRoots[0];
        this.mode = "focus";
      }
    }
  }

  // Drawn once the graph element exists: after the first render with data.
  protected updated(changed: PropertyValues<this>) {
    if (changed.has("data") && this.data) {
      this.draw();
      this.side = { kind: "overview" };
    }
  }

  /** Served by `ig serve`: notes can be read and added. */
  live(api: Client) {
    this.api = api;
    this.refreshNotes();
  }

  refreshNotes() {
    this.api?.notes().then((ns) => {
      this.notes = ns;
      this.graph.decorate(pins(ns), ghosts(ns));
    }, (e: Error) => (this.hint = e.message));
  }

  /** From `ig_focus`: Focus mode on these nodes. */
  focusAll(ids: string[]) {
    const known = ids.filter((id) => this.ix && this.ix.byId[id]);
    if (!known.length) return;
    this.focusRoots = known;
    this.focusId = known[0];
    this.mode = "focus";
    this.draw();
  }

  /** Gestures make notes only on a checked plan that is not being applied, in `ig serve`. */
  private get planning() {
    return !!this.api && this.data?.state === "planned";
  }

  private addNote(n: NoteInput) {
    this.api?.add(n).then(() => {
      this.hint = "Note added; the agent will pick it up.";
      this.refreshNotes();
    }, (e: Error) => (this.hint = e.message));
  }

  private onContext(e: CustomEvent<{ id: string; edge: boolean; source: string; target: string; x: number; y: number }>) {
    const d = e.detail;
    const by = this.ix.byId;
    if (!this.planning) return;
    if (d.edge ? by[d.source] && by[d.target] : by[d.id])
      this.menu = d.edge ? { x: d.x, y: d.y, target: d.source, to: d.target } : { x: d.x, y: d.y, target: d.id };
  }

  private menuAction(kind: "comment" | "remove") {
    const m = this.menu!;
    this.menu = null;
    const what = m.to ? m.target + " → " + m.to : m.target;
    if (kind === "remove") return this.addNote({ kind, target: m.target, to: m.to });
    const text = window.prompt("Comment on " + what);
    if (text && text.trim()) this.addNote({ kind, target: m.target, to: m.to, text });
  }

  private onDrop(e: CustomEvent<{ id: string; y: number }>) {
    const n = this.ix.byId[e.detail.id];
    if (!this.planning || this.mode !== "architecture" || !n) return;
    const note = dropNote(this.graph.lanes(this.data!.layers), n.id, e.detail.y, n.layer);
    if (note) this.addNote(note);
  }

  private onLink(e: CustomEvent<{ from: string; to: string }>) {
    const { from, to } = e.detail;
    if (!this.planning || !this.ix.byId[from] || !this.ix.byId[to]) return;
    const kind = window.prompt("Link " + from + " → " + to + ": kind", "calls");
    if (kind && kind.trim()) this.addNote({ kind: "link", target: from, to, text: kind.trim() });
  }

  private addBox(layer: string) {
    const v = window.prompt("Add to layer " + layer + ": <kind> <name>", "module ");
    const m = v && /^\s*(\S+)\s+(\S+)\s*$/.exec(v);
    if (m) this.addNote({ kind: "add", text: m[1], target: m[2], layer });
    else if (v) this.hint = "Expected <kind> <name>, like: module cache";
  }

  /** Re-renders the graph for the current mode. `zoomTo` fits that box. */
  private draw(zoomTo?: string) {
    const ix = this.ix;
    const mode = this.mode;
    const els =
      mode === "architecture" ? architecture(ix, this.scope, this.graph.clientWidth || 1200)
      : mode === "focus" ? exact(ix, focusIds(ix, this.focusRoots.length ? this.focusRoots : this.focusId!, this.focusDepth))
      : mode === "blueprint" ? blueprint(ix, this.journey, this.uiOnly)
      : exact(ix, changedIds(ix));
    if (mode === "architecture" && this.planning)
      els.filter((e) => e.data.kind === "lanelabel" && this.data!.layers.includes(e.data.label!)).forEach((l) =>
        els.push({ group: "nodes", data: { id: "laneadd:" + l.data.label, label: "+", kind: "laneadd" },
          position: { x: l.position!.x, y: l.position!.y + 28 }, grabbable: false }));
    const fresh = mode !== this.lastMode;
    this.lastMode = mode;
    this.graph.show(els, { fresh, preset: mode === "blueprint" || mode === "architecture", zoomTo });
    this.graph.decorate(pins(this.notes), ghosts(this.notes));
    if (mode === "focus" && this.focusId) this.graph.select(this.focusId);
    if (mode === "blueprint") this.side = { kind: "journeys" };
    this.screenIds = this.graph.ids();
    const focused = this.focusId && ix.byId[this.focusId];
    this.hint =
      mode === "blueprint"
        ? this.journey
          ? "Journey " + journeyName(this.journey).route + ": the page and everything behind it, by lane. Dashed blue: HTTP calls."
          : "Pick a page on the right."
        : mode === "architecture" ? "Click a box to open it. Dimmed boxes are outside, connected context."
        : mode === "focus" ? "Neighbourhood of " + (focused ? focused.name : "") + " (depth " + this.focusDepth + ")"
        : this.screenIds.length ? "" : "No changes against the base.";
  }

  private setMode(m: Mode) {
    if (m === "focus" && !this.focusId) return;
    this.mode = m;
    this.draw();
  }

  private toggleLinks() {
    this.links = !this.links;
    try {
      localStorage.setItem(LINKS_KEY, this.links ? "1" : "0");
    } catch {
      // storage blocked: the choice lasts for this page only
    }
  }

  private setScope(t: Group | undefined) {
    this.scope = t || this.ix.root;
    this.mode = "architecture";
    this.draw();
  }

  private focusOn(id: string) {
    if (!this.ix.byId[id]) return;
    this.focusId = id;
    this.focusRoots = [];
    this.mode = "focus";
    this.draw();
    this.details(id);
  }

  private details(id: string, fromTree = false) {
    if (!this.ix.byId[id]) {
      this.side = this.ix.groups[id] ? { kind: "group", id } : { kind: "overview" };
      return;
    }
    if (!fromTree) this.tree.reveal(id);
    this.side = { kind: "node", id };
  }

  private onTap(e: CustomEvent<{ id: string; kind: string }>) {
    const { id, kind } = e.detail;
    if (kind === "laneadd") return this.addBox(id.slice("laneadd:".length));
    if (this.mode === "architecture" && (kind === "layer" || kind === "dir")) {
      this.setScope(this.ix.groups[id]);
      this.side = { kind: "group", id };
      return;
    }
    this.details(id);
    this.graph.fadeAround(id);
  }

  /** Tree → graph: selects the element that stands for `id` in this mode, or says it is not shown. */
  private onTreeSelect(e: CustomEvent<string>) {
    const id = e.detail;
    const ix = this.ix;
    const n = ix.byId[id];
    this.details(id, true);
    if (!n) return;
    const candidates =
      this.mode === "architecture" ? [rep(ix, this.scope, id)]
      : this.mode === "blueprint" ? [unitOf(ix, id)]
      : [id, topMember(ix, n).id, n.file];
    const x = this.graph.pick(candidates);
    this.graph.clear();
    if (x) {
      this.graph.select(x);
      this.graph.fadeAround(x);
      this.graph.center(x);
    } else {
      this.hint = "Not in this view; double-click it in the tree (or press Focus) to see its neighbourhood.";
    }
  }

  private onTreeFolder(e: CustomEvent<string>) {
    const prefix = e.detail + "/";
    this.graph.fadeExcept((id) => {
      const b = this.ix.byId[id];
      return !!b && !!b.file && b.file.indexOf(prefix) === 0;
    });
  }

  private onSearch(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    const hit = findNode(this.ix, (e.target as HTMLInputElement).value);
    if (hit) this.focusOn(hit);
  }

  private crumbs() {
    if (this.mode !== "architecture") return null;
    const root = this.ix.root;
    return [root, ...ancestors(this.ix, this.scope)].map((t, i) => {
      const label = i ? t.label : "All";
      const go = (e: Event) => {
        e.preventDefault();
        this.setScope(t);
        this.side = { kind: "group", id: t.id };
      };
      return html`${i ? " › " : ""}${t === this.scope ? html`<strong>${label}</strong>` : html`<a href="#" @click=${go}>${label}</a>`}`;
    });
  }

  render() {
    const D = this.data;
    if (!D) return null;
    const c = D.counts;
    return html`<header>
        <h1>ig</h1>
        <span id="status" class="status ${D.blocking ? "block" : "ok"}">${D.blocking ? "blocking" : "not blocking"}</span>
        <span id="plan" class="status">${D.plan ? STATE_LABEL[D.state] + " · " + D.plan.title : ""}</span>
        <div class="modes" role="group" aria-label="View mode">
          ${MODES.map(([m, label, title]) => html`<button data-mode=${m} title=${title} aria-pressed=${String(m === this.mode)}
            ?disabled=${m === "focus" && !this.focusId} @click=${() => this.setMode(m)}>${label}</button>`)}
        </div>
        <button id="links" class="toggle" aria-pressed=${String(this.links)} title="Show or hide the links (the selected node's stay)"
          @click=${this.toggleLinks}>Links</button>
        <input id="search" type="search" placeholder="Find a node…" aria-label="Find a node" list="ids" autocomplete="off" @keydown=${this.onSearch}>
        <datalist id="ids">${D.nodes.filter((n) => n.kind !== "test").map((n) => html`<option value=${n.id}></option>`)}</datalist>
        <span id="counts" class="counts">${c.added} added · ${c.changed} changed · ${c.removed} removed</span>
        <span id="crumbs" class="crumbs">${this.crumbs()}</span>
        <span id="hint" class="counts">${this.hint}</span>
      </header>
      <main>
        <ig-tree id="tree" aria-label="Files" role="navigation" .ix=${this.ix} .mode=${this.mode} .screenIds=${this.screenIds}
          @ig-tree-select=${this.onTreeSelect} @ig-tree-folder=${this.onTreeFolder}></ig-tree>
        <ig-graph id="cy" role="img" .links=${this.links} aria-label="Code graph" @ig-tap=${this.onTap}
          @ig-context=${this.onContext} @ig-drop=${this.onDrop} @ig-link=${this.onLink}></ig-graph>
        <ig-side-panel id="side" role="complementary" .ix=${this.ix} .view=${this.side} .mode=${this.mode} .focusId=${this.focusId}
          .focusDepth=${this.focusDepth} .journey=${this.journey} .uiOnly=${this.uiOnly} .notes=${this.notes}></ig-side-panel>
      </main>
      ${this.menu
        ? html`<div class="ctx-menu" role="menu" style="left:${this.menu.x}px;top:${this.menu.y}px">
            <button role="menuitem" @click=${() => this.menuAction("comment")}>Comment</button>
            <button role="menuitem" @click=${() => this.menuAction("remove")}>Remove</button>
          </div>`
        : null}`;
  }

  connectedCallback() {
    super.connectedCallback();
    // Any click or Escape closes the right-click menu (its buttons act first).
    window.addEventListener("click", () => (this.menu = null));
    window.addEventListener("keydown", (e) => e.key === "Escape" && (this.menu = null));
    this.addEventListener("ig-focus", (e) => this.focusOn((e as CustomEvent<string>).detail));
    this.addEventListener("ig-overview", () => {
      this.graph.clear();
      this.side = { kind: "overview" };
    });
    this.addEventListener("ig-depth", (e) => {
      this.focusDepth = 3 - this.focusDepth;
      this.draw();
      this.details((e as CustomEvent<string>).detail);
    });
    this.addEventListener("ig-arch", (e) => {
      const n = this.ix.byId[(e as CustomEvent<string>).detail];
      this.setScope(this.ix.treeOf[n.file]);
      this.graph.select(n.file);
    });
    this.addEventListener("ig-journey", (e) => {
      this.journey = (e as CustomEvent<string>).detail;
      this.draw();
    });
    this.addEventListener("ig-ui-only", (e) => {
      this.uiOnly = (e as CustomEvent<boolean>).detail;
      if (this.journey) this.draw();
    });
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-app": IgApp;
  }
}
