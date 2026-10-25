// The graph canvas: owns the Cytoscape instance. <ig-app> hands it elements; taps on nodes go back
// up as `ig-tap`.

import { LitElement } from "lit";
import { customElement, property } from "lit/decorators.js";
import cytoscape from "cytoscape";
import fcose from "cytoscape-fcose";
import { iconFor, iconUri } from "../icons";
import type { Lane } from "../model/architecture";
import { applyLinks, type El } from "../model/graph";
import { emit } from "./shared";

cytoscape.use(fcose);

function css(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function stylesheet(): cytoscape.Stylesheet[] {
  const color: Record<string, string> = { added: css("--added"), changed: css("--changed"), removed: css("--removed"), unchanged: css("--unchanged") };
  const [muted, line, err, accent, panel] = ["--muted", "--line", "--error", "--accent", "--panel"].map(css);
  const fg = css("--fg");
  const statusColor = (n: cytoscape.NodeSingular) => color[n.data("status")] || color.unchanged;
  const s = [
    { selector: "node", style: {
      "label": "data(label)", "font-size": 10, "color": fg, "text-valign": "bottom", "text-margin-y": 3,
      "min-zoomed-font-size": 7, "background-color": statusColor, "width": 14, "height": 14,
      "text-wrap": "ellipsis", "text-max-width": 160 } },
    { selector: "node[kind = 'module']", style: { "width": 20, "height": 20, "shape": "round-rectangle" } },
    { selector: "node[kind = 'layer'], node[kind = 'dir']", style: {
      "shape": "round-rectangle", "height": "mapData(count, 1, 300, 34, 80)",
      "width": (n: cytoscape.NodeSingular) => Math.max(70, n.data("label").length * 8.5 + 24),
      "text-valign": "center", "text-margin-y": 0, "font-size": 13, "font-weight": 600, "min-zoomed-font-size": 4,
      "background-color": panel, "border-width": 2, "border-color": statusColor, "text-max-width": 400 } },
    { selector: "node[kind = 'layer']", style: { "border-width": 3, "font-size": 15 } },
    { selector: "node[kind = 'open'], node[kind = 'openmodule']", style: {
      "shape": "round-rectangle", "background-color": panel, "background-opacity": 0.5, "border-width": 1.5, "border-style": "dashed",
      "border-color": statusColor, "text-valign": "top", "text-halign": "center", "text-margin-y": -4, "font-size": 12,
      "font-weight": 600, "padding": 14, "min-zoomed-font-size": 5 } },
    { selector: "node[?bp]", style: { "font-size": 14, "min-zoomed-font-size": 4, "text-max-width": 230, "text-wrap": "ellipsis" } },
    { selector: "edge[kind = 'dispatch']", style: { "line-style": "dashed" } },
    { selector: "node[kind = 'lane']", style: {
      "shape": "rectangle", "width": "data(w)", "height": "data(h)", "background-color": line, "background-opacity": 0.18,
      "border-width": 0, "label": "", "events": "no", "z-index": 0 } },
    { selector: "node[kind = 'lanelabel']", style: {
      "shape": "rectangle", "width": 1, "height": 1, "background-opacity": 0, "border-width": 0, "text-valign": "center",
      "text-halign": "center", "font-size": 18, "font-weight": 700, "color": muted, "events": "no", "min-zoomed-font-size": 3 } },
    { selector: "node[kind = 'journey']", style: { "shape": "round-rectangle", "width": 34, "height": 34, "background-color": accent, "font-size": 12, "font-weight": 700 } },
    { selector: "node[kind = 'route']", style: { "shape": "tag", "width": 22, "height": 22, "background-color": css("--note") } },
    { selector: "node[kind = 'system']", style: {
      "shape": "round-rectangle", "width": 44, "height": 44, "background-color": panel, "border-width": 1, "border-color": line,
      "background-image": (n: cytoscape.NodeSingular) => iconUri(iconFor(n.data("label"), n.data("packages") || [], n.data("category")), fg),
      "background-width": "80%", "background-height": "80%", "font-size": 12, "font-weight": 700 } },
    { selector: "edge[kind = 'http']", style: { "line-color": css("--note"), "target-arrow-color": css("--note"), "line-style": "dashed", "opacity": 0.95, "width": 2 } },
    { selector: "edge[kind = 'uses']", style: { "line-color": css("--warning"), "target-arrow-color": css("--warning"), "opacity": 0.9 } },
    { selector: "node[?outer]", style: { "opacity": 0.55, "border-style": "dashed" } },
    { selector: "node[status = 'removed']", style: { "border-style": "dashed", "background-opacity": 0.35 } },
    { selector: "node[?planned]", style: { "border-style": "dashed", "border-width": 2, "border-color": statusColor, "background-opacity": 0.35 } },
    { selector: "node[findings > 0]", style: { "border-width": 3, "border-color": err, "border-style": "solid" } },
    { selector: "node:selected", style: { "border-width": 4, "border-color": accent, "border-style": "solid" } },
    { selector: "edge", style: {
      "width": "mapData(count, 1, 40, 1, 7)", "curve-style": "bezier", "target-arrow-shape": "triangle", "arrow-scale": 0.8,
      "line-color": muted, "target-arrow-color": muted, "opacity": 0.55,
      "label": "data(count)", "font-size": 9, "color": muted, "min-zoomed-font-size": 9, "text-background-color": panel,
      "text-background-opacity": 1, "text-background-padding": 1 } },
    { selector: "edge[count = 1]", style: { "label": "" } },
    { selector: "edge[status = 'added']", style: { "line-color": color.added, "target-arrow-color": color.added, "opacity": 0.9 } },
    { selector: "edge[status = 'removed']", style: { "line-style": "dashed", "line-color": color.removed, "target-arrow-color": color.removed } },
    { selector: "edge[?guessed]", style: { "line-style": "dotted" } },
    { selector: "edge[?planned]", style: { "line-style": "dashed" } },
    { selector: "edge[?ctx]", style: { "opacity": 0.22 } },
    { selector: "edge[?violation]", style: { "line-color": err, "target-arrow-color": err, "opacity": 1, "width": "mapData(count, 1, 40, 2.5, 8)" } },
    { selector: ".faded", style: { "opacity": 0.12 } },
    { selector: "node[?pinned]", style: { "border-width": 3, "border-color": accent, "border-style": "solid", "color": accent } },
    { selector: "node[?ghost]", style: { "opacity": 0.4, "border-style": "dotted", "border-width": 2, "border-color": accent } },
    { selector: "node[kind = 'laneadd']", style: {
      "shape": "ellipse", "width": 22, "height": 22, "background-color": accent, "color": "#fff", "label": "+", "text-valign": "center",
      "text-margin-y": 0, "font-size": 16, "font-weight": 700, "min-zoomed-font-size": 3 } },
    { selector: "edge.nolink", style: { "display": "none" } },
  ];
  return s as unknown as cytoscape.Stylesheet[];
}

const FCOSE = {
  name: "fcose", quality: "proof", randomize: true, animate: false, fit: false,
  nodeDimensionsIncludeLabels: true, packComponents: true, nodeRepulsion: () => 60000,
  idealEdgeLength: () => 160, edgeElasticity: () => 0.2, nestingFactor: 0.4,
  gravity: 0.15, gravityRange: 5, numIter: 4000, tile: true, tilingPaddingVertical: 20, tilingPaddingHorizontal: 20,
  nodeSeparation: 120, padding: 20,
} as unknown as cytoscape.LayoutOptions;

@customElement("ig-graph")
export class IgGraph extends LitElement {
  private _cy?: cytoscape.Core;
  /** Edges shown; when false only the selected node's are. */
  @property({ type: Boolean }) links = true;

  createRenderRoot() {
    return this;
  }

  get cy(): cytoscape.Core {
    if (!this._cy) {
      const cy = (this._cy = cytoscape({ container: this, minZoom: 0.05, maxZoom: 3, style: stylesheet() }));
      (window as unknown as { igCy: cytoscape.Core }).igCy = cy; // for debugging from the console
      cy.on("tap", (e) => {
        const t = e.target;
        if (t === cy) {
          cy.elements().removeClass("faded");
          return;
        }
        if (t.isNode()) emit(this, "ig-tap", { id: t.id(), kind: t.data("kind") });
      });
      cy.on("select unselect", "node", () => this.applyLinks());
      // Planning gestures, turned into notes by <ig-app>: right-click, drop, shift-drag a link.
      cy.on("cxttap", "node, edge", (e) => {
        const t = e.target;
        const o = e.originalEvent as MouseEvent;
        emit(this, "ig-context", { id: t.id(), edge: t.isEdge(), source: t.data("source"), target: t.data("target"), x: o.clientX, y: o.clientY });
      });
      cy.on("dragfree", "node", (e) => emit(this, "ig-drop", { id: e.target.id(), y: e.target.position().y }));
      // Shift holds the boxes still for drawing a link; keyup restores what was set before.
      let grabBefore: boolean | null = null;
      const shift = (e: KeyboardEvent) => {
        if (e.key !== "Shift") return;
        if (e.type === "keydown" && grabBefore === null) {
          grabBefore = cy.autoungrabify();
          cy.autoungrabify(true);
        } else if (e.type === "keyup" && grabBefore !== null) {
          cy.autoungrabify(grabBefore);
          grabBefore = null;
        }
      };
      window.addEventListener("keydown", shift);
      window.addEventListener("keyup", shift);
      let linkFrom: string | null = null;
      cy.on("tapstart", "node", (e) => void (linkFrom = (e.originalEvent as MouseEvent)?.shiftKey ? e.target.id() : null));
      cy.on("tapend", (e) => {
        const from = linkFrom;
        linkFrom = null;
        if (!from) return;
        const p = e.position;
        const hit = cy.nodes().filter((n) => {
          const b = n.boundingBox({});
          return !String(n.data("kind")).startsWith("lane") && n.id() !== from && p.x >= b.x1 && p.x <= b.x2 && p.y >= b.y1 && p.y <= b.y2;
        });
        if (hit.length) emit(this, "ig-link", { from, to: hit.last().id() });
      });
    }
    return this._cy;
  }

  protected updated() {
    this.applyLinks();
  }

  private applyLinks() {
    applyLinks(this.cy, this.links);
  }

  /** Replaces the elements. Unless `fresh`, nodes keep their positions and new ones start next to the
   *  box they came out of, so opening a box doesn't reshuffle the picture. `zoomTo` fits that box. */
  show(els: El[], o: { fresh: boolean; preset: boolean; zoomTo?: string }) {
    const cy = this.cy;
    const prev: Record<string, cytoscape.Position> = {};
    if (!o.fresh) cy.nodes().forEach((n) => void (prev[n.id()] = Object.assign({}, n.position())));
    cy.startBatch();
    cy.elements().remove();
    cy.add(els as cytoscape.ElementDefinition[]);
    const byParent: Record<string, string | undefined> = {};
    const placed: Record<string, boolean> = {};
    els.forEach((x) => {
      if (x.group === "nodes") byParent[x.data.id] = x.data.parent;
      if (x.position) placed[x.data.id] = true;
    });
    cy.nodes().forEach((n) => {
      if (placed[n.id()]) return; // a preset position wins over the previous one
      let p: cytoscape.Position | undefined = prev[n.id()];
      let up: string | undefined = n.id();
      while (!p && up) {
        up = byParent[up];
        p = up ? prev[up] : undefined;
      }
      if (p) n.position({ x: p.x + (prev[n.id()] ? 0 : Math.random() * 40 - 20), y: p.y + (prev[n.id()] ? 0 : Math.random() * 40 - 20) });
    });
    cy.endBatch();
    this.applyLinks();
    const layout = cy.layout(o.preset ? { name: "preset", fit: false } : FCOSE);
    layout.one("layoutstop", () => {
      const target = o.zoomTo ? cy.getElementById(o.zoomTo) : null;
      if (target && target.length) cy.fit(target.union(target.descendants()).union(target.neighborhood()), 40);
      else cy.fit(undefined, 30);
      if (cy.zoom() > 1.4) cy.zoom({ level: 1.4, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
    });
    layout.run();
  }

  /** Shows note pins (ids after the label) and ghosts (open moves) on the drawn nodes. */
  decorate(pins: Record<string, string[]>, ghosts: Set<string>) {
    this.cy.nodes().forEach((n) => {
      if (n.data("base") === undefined) n.data("base", n.data("label") ?? "");
      const p = pins[n.id()];
      n.data({ label: p ? n.data("base") + "  ◉ " + p.join(" ") : n.data("base"), pinned: !!p, ghost: ghosts.has(n.id()) });
    });
  }

  /** The drawn layer rows (Architecture), for drops. */
  lanes(layers: string[]): Lane[] {
    return this.cy.nodes("[kind = 'lane']").map((n) => {
      const y = n.position().y;
      const h = n.data("h") as number;
      const name = n.data("label") as string;
      return { row: 0, name, kind: layers.includes(name) ? "layer" : "other", top: y - h / 2, bottom: y + h / 2 };
    });
  }

  ids(): string[] {
    return this.cy.nodes().map((n) => n.id());
  }

  /** The first of `candidates` that is on the graph. */
  pick(candidates: (string | null)[]): string | null {
    return candidates.find((c) => c && this.cy.getElementById(c).length) ?? null;
  }

  select(id: string) {
    this.cy.getElementById(id).select();
  }

  /** Unselects everything and clears the fading. */
  clear() {
    this.cy.$(":selected").unselect();
    this.cy.elements().removeClass("faded");
  }

  /** Fades everything but `id`, its neighbours and their parents. */
  fadeAround(id: string) {
    const x = this.cy.getElementById(id);
    const nb = x.closedNeighborhood().union(x.ancestors());
    this.cy.elements().addClass("faded");
    nb.removeClass("faded");
    nb.ancestors().removeClass("faded");
  }

  /** Fades everything but the nodes `keep` accepts (and their parents); nothing if none does. */
  fadeExcept(keep: (id: string) => boolean) {
    const xs = this.cy.nodes().filter((n) => keep(n.id()));
    this.cy.elements().removeClass("faded");
    if (xs.length) {
      this.cy.elements().addClass("faded");
      xs.removeClass("faded");
      xs.ancestors().removeClass("faded");
    }
  }

  center(id: string) {
    this.cy.animate({ center: { eles: this.cy.getElementById(id) }, duration: 200 });
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "ig-graph": IgGraph;
  }
}
