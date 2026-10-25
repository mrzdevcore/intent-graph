// The app: header (status, plan, modes, search, counts, breadcrumb, hint), and the state the tree,
// graph and side panel share: mode, Architecture scope, focus, journey.

import { LitElement, html, type PropertyValues } from "lit";
import { customElement, property, query, state } from "lit/decorators.js";
import type { ViewData } from "../data";
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

type Mode = "changed" | "architecture" | "focus" | "blueprint";

const MODES: [Mode, string, string][] = [
  ["changed", "Changed", "Changed nodes and their direct neighbours"],
  ["architecture", "Architecture", "Layers, then directories, modules and members. Click a box to open it."],
  ["focus", "Focus", "The neighbourhood of the selected node"],
  ["blueprint", "Blueprint", "A page and everything behind it: front, API, use cases, adapters, systems"],
];

@customElement("ig-app")
export class IgApp extends LitElement {
  @property({ attribute: false }) data?: ViewData;
  @state() private mode: Mode = "changed";
  @state() private scope!: Group;
  @state() private focusId: string | null = null;
  @state() private focusDepth = 1;
  @state() private journey: string | null = null;
  @state() private uiOnly = false;
  @state() private side: SideView = { kind: "overview" };
  @state() private hint = "";
  @state() private screenIds: string[] = [];
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
    }
  }

  // Drawn once the graph element exists: after the first render with data.
  protected updated(changed: PropertyValues<this>) {
    if (changed.has("data") && this.data) {
      this.draw();
      this.side = { kind: "overview" };
    }
  }

  /** Re-renders the graph for the current mode. `zoomTo` fits that box. */
  private draw(zoomTo?: string) {
    const ix = this.ix;
    const mode = this.mode;
    const els =
      mode === "architecture" ? architecture(ix, this.scope)
      : mode === "focus" ? exact(ix, focusIds(ix, this.focusId!, this.focusDepth))
      : mode === "blueprint" ? blueprint(ix, this.journey, this.uiOnly)
      : exact(ix, changedIds(ix));
    const fresh = mode !== this.lastMode;
    this.lastMode = mode;
    this.graph.show(els, { fresh, preset: mode === "blueprint", zoomTo });
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

  private setScope(t: Group | undefined) {
    this.scope = t || this.ix.root;
    this.mode = "architecture";
    this.draw();
  }

  private focusOn(id: string) {
    if (!this.ix.byId[id]) return;
    this.focusId = id;
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
        <span id="plan" class="status">${D.plan ? "Plan · " + D.plan.title : ""}</span>
        <div class="modes" role="group" aria-label="View mode">
          ${MODES.map(([m, label, title]) => html`<button data-mode=${m} title=${title} aria-pressed=${String(m === this.mode)}
            ?disabled=${m === "focus" && !this.focusId} @click=${() => this.setMode(m)}>${label}</button>`)}
        </div>
        <input id="search" type="search" placeholder="Find a node…" aria-label="Find a node" list="ids" autocomplete="off" @keydown=${this.onSearch}>
        <datalist id="ids">${D.nodes.filter((n) => n.kind !== "test").map((n) => html`<option value=${n.id}></option>`)}</datalist>
        <span id="counts" class="counts">${c.added} added · ${c.changed} changed · ${c.removed} removed</span>
        <span id="crumbs" class="crumbs">${this.crumbs()}</span>
        <span id="hint" class="counts">${this.hint}</span>
      </header>
      <main>
        <ig-tree id="tree" aria-label="Files" role="navigation" .ix=${this.ix} .mode=${this.mode} .screenIds=${this.screenIds}
          @ig-tree-select=${this.onTreeSelect} @ig-tree-folder=${this.onTreeFolder}></ig-tree>
        <ig-graph id="cy" role="img" aria-label="Code graph" @ig-tap=${this.onTap}></ig-graph>
        <ig-side-panel id="side" role="complementary" .ix=${this.ix} .view=${this.side} .mode=${this.mode} .focusId=${this.focusId}
          .focusDepth=${this.focusDepth} .journey=${this.journey} .uiOnly=${this.uiOnly}></ig-side-panel>
      </main>`;
  }

  connectedCallback() {
    super.connectedCallback();
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
