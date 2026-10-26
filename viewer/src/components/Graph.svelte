<!-- The graph canvas: owns the sigma.js (WebGL) renderer, created when the element mounts and destroyed with it.
     <App> hands it elements; taps and planning gestures go back up as callbacks. Rows and group boxes, which
     sigma has no notion of, are drawn on a canvas under the edges and re-projected after every frame. -->
<script module lang="ts">
  import Sigma from "sigma";
  import { NodeCircleProgram } from "sigma/rendering";
  import type { NodeDisplayData, PartialButFor } from "sigma/types";
  import { createNodeImageProgram } from "@sigma/node-image";
  import { EdgeProgramWithDash } from "./dashed";
  import { palette, setPalette } from "../model/sigma";

  type Drawn = PartialButFor<NodeDisplayData, "x" | "y" | "size" | "label" | "color"> & {
    ring?: string; ringDash?: number; labelColor?: string; labelSize?: number; kind?: string; anchor?: boolean };

  const FONT = "system-ui, -apple-system, 'Segoe UI', sans-serif";

  /** Screen cells (16 px) already under a label in this frame. */
  const taken = new Set<number>();

  /** Claims the cells under a label's box; false when one is already taken. Forced labels (selection,
   *  `ig_show`) always draw; the rest only where they fit, so labels never overlap at any zoom. */
  function claim(d: Drawn, size: number): boolean {
    const w = d.label!.length * size * 0.6;
    const [x0, x1] = [Math.floor((d.x - w / 2) / 16), Math.floor((d.x + w / 2) / 16)];
    const [y0, y1] = [Math.floor((d.y + d.size + 2) / 16), Math.floor((d.y + d.size + size + 4) / 16)];
    const cells: number[] = [];
    for (let i = x0; i <= x1; i++) for (let j = y0; j <= y1; j++) cells.push(i * 100003 + j);
    if (!d.forceLabel && cells.some((c) => taken.has(c))) return false;
    cells.forEach((c) => taken.add(c));
    return true;
  }

  /** sigma's label pass: the label, where it fits. */
  function drawLabel(ctx: CanvasRenderingContext2D, d: Drawn) {
    if (d.label && (d.kind === "laneadd" || claim(d, d.labelSize ?? 11))) paintLabel(ctx, d);
  }

  /** A label centred under its node. */
  function paintLabel(ctx: CanvasRenderingContext2D, d: Drawn) {
    if (!d.label) return;
    const size = d.labelSize ?? 11;
    ctx.font = `${d.kind === "laneadd" ? 700 : 400} ${size}px ${FONT}`;
    ctx.fillStyle = d.kind === "laneadd" ? "#161615" : d.labelColor ?? palette.fg;
    ctx.textAlign = "center";
    ctx.textBaseline = d.kind === "laneadd" ? "middle" : "top";
    ctx.fillText(d.label, d.x, d.kind === "laneadd" ? d.y : d.y + d.size + 3);
    ctx.textAlign = "start";
    ctx.textBaseline = "alphabetic";
  }

  /** Hovered and highlighted nodes: their ring (selection, `ig_show`, findings, notes), then the label on a plate. */
  function drawHover(ctx: CanvasRenderingContext2D, d: Drawn) {
    if (d.anchor || d.kind === "layer" || d.kind === "dir") return;
    if (d.ring) {
      ctx.beginPath();
      ctx.setLineDash(d.ringDash ? [d.ringDash * 2, d.ringDash * 2] : []);
      ctx.strokeStyle = d.ring;
      ctx.lineWidth = 3;
      ctx.arc(d.x, d.y, d.size + 2.5, 0, Math.PI * 2);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    if (!d.label || d.kind === "laneadd") return paintLabel(ctx, d);
    const size = d.labelSize ?? 11;
    ctx.font = `400 ${size}px ${FONT}`;
    const w = ctx.measureText(d.label).width + 8;
    ctx.fillStyle = palette.panel;
    ctx.globalAlpha = 0.85;
    ctx.fillRect(d.x - w / 2, d.y + d.size + 2, w, size + 4);
    ctx.globalAlpha = 1;
    paintLabel(ctx, d);
  }

  /** An edge's count or step badge, upright on a plate a third of the way from its source (two edges
   *  between the same boxes, one each way, keep apart; one passing through a box's row skips its middle). */
  function drawEdgeLabel(ctx: CanvasRenderingContext2D, d: { label?: string | null; color: string }, s: { x: number; y: number }, t: { x: number; y: number }) {
    if (!d.label) return;
    const [x, y] = [s.x + (t.x - s.x) * 0.35, s.y + (t.y - s.y) * 0.35];
    ctx.font = `700 11px ${FONT}`;
    const w = ctx.measureText(d.label).width + 6;
    ctx.fillStyle = palette.panel;
    ctx.fillRect(x - w / 2, y - 8, w, 16);
    ctx.fillStyle = d.color.startsWith("rgba") ? palette.muted : d.color;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(d.label, x, y);
    ctx.textAlign = "start";
    ctx.textBaseline = "alphabetic";
  }

  function css(name: string) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }
</script>

<script lang="ts">
  import type Graph from "graphology";
  import type { Lane } from "../model/architecture";
  import type { El } from "../model/graph";
  import { positions } from "../model/layout";
  import { alpha, background, badge, edgeReducer, groupAt, nodeReducer, toGraphology, type Background, type EdgeAttrs,
    type NodeAttrs, type ViewState } from "../model/sigma";

  type Context = { id: string; edge: boolean; source: string; target: string; x: number; y: number };
  let { links, ontap, oncontext, ondrop, onlink }: {
    /** Edges shown; when false only the selected node's are. */
    links: boolean;
    ontap: (d: { id: string; kind: string }) => void;
    oncontext: (d: Context) => void;
    ondrop: (d: { id: string; y: number }) => void;
    onlink: (d: { from: string; to: string }) => void;
  } = $props();

  let el: HTMLElement;
  let sg: Sigma<NodeAttrs, EdgeAttrs>;
  let g: Graph<NodeAttrs, EdgeAttrs>;
  let els: El[] = [];
  let bg: Background = { lanes: [], groups: [] };
  let kindOf: Record<string, string> = {};
  let parentOf: Record<string, string | undefined> = {};
  /** Every drawn node id, compounds included (`pick()`, fading). */
  let known = new Set<string>();
  /** Decorations by id, for the boxes on the background too (most compounds are not graph nodes). */
  let deco: Record<string, { pins?: string; ghost?: boolean; steps?: string }> = {};
  const st: ViewState = { selected: null, hovered: null, show: null, tests: true, links: true, focus: null };
  let painted = false;

  // The model's y points down, sigma's up: flip at the boundary.
  const toView = (x: number, y: number) => sg.graphToViewport({ x, y: -y });
  const fromView = (x: number, y: number) => {
    const p = sg.viewportToGraph({ x, y });
    return { x: p.x, y: -p.y };
  };

  function refresh() {
    if (!sg) return;
    sg.setSetting("nodeReducer", nodeReducer(st) as never);
    sg.setSetting("edgeReducer", edgeReducer(st) as never);
  }

  /** Rows and compound boxes under the edges; Architecture's opaque boxes over them (edges then end
   *  at their border), in viewport pixels. */
  function drawBackground() {
    const [under, over] = [sg.getCanvases().bg.getContext("2d")!, sg.getCanvases().boxes.getContext("2d")!];
    const { width, height } = sg.getDimensions();
    under.clearRect(0, 0, width, height);
    over.clearRect(0, 0, width, height);
    const scale = sg.getGraphToViewportRatio(); // pixels per graph unit at this zoom
    bg.lanes.forEach((l) => {
      const [a, b] = [toView(0, l.y0), toView(0, l.y1)];
      under.fillStyle = alpha(palette.line, 0.45);
      under.fillRect(0, a.y, width, b.y - a.y);
      under.font = `700 ${Math.max(11, Math.min(18, 18 * scale))}px ${FONT}`;
      under.fillStyle = palette.muted;
      under.textBaseline = "middle";
      under.fillText(l.label, Math.max(10, l.x === undefined ? 10 : toView(l.x, 0).x - 40), (a.y + b.y) / 2);
    });
    bg.groups.forEach((b) => {
      const [p, q] = [toView(b.x0, b.y0), toView(b.x1, b.y1)];
      const [w, h] = [q.x - p.x, q.y - p.y];
      if (q.x < 0 || p.x > width || q.y < 0 || p.y > height) return;
      const box = b.kind === "layer" || b.kind === "dir";
      const ctx = box ? over : under;
      const n = deco[b.id];
      const dim = st.show ? (st.show.nodes.has(b.id) ? 1 : 0.15) : st.focus && !st.focus.has(b.id) ? 0.12 : 1;
      const status = palette[b.status] ?? palette.unchanged;
      const border = st.show?.nodes.has(b.id) ? palette.brand : st.selected === b.id ? palette.accent
        : n?.ghost || n?.pins ? palette.accent : b.findings && box ? palette.error : status;
      ctx.globalAlpha = Math.min(dim, b.outer ? 0.55 : n?.ghost ? 0.4 : 1);
      ctx.beginPath();
      ctx.roundRect(p.x, p.y, w, h, Math.min(8, w / 4, h / 4));
      ctx.fillStyle = box ? palette.panel : alpha(palette.panel, 0.5);
      ctx.fill();
      ctx.setLineDash(box && !b.outer && !n?.ghost ? [] : [5, 4]);
      ctx.lineWidth = Math.max(1, Math.min(4, (border === status ? (b.kind === "layer" ? 3 : 2) : 4) * scale * (box ? 1 : 0.75)));
      ctx.strokeStyle = border;
      ctx.stroke();
      ctx.setLineDash([]);
      // Box names scale with the zoom, from 6 px (smaller is noise) up to 26 px.
      const fs = (box ? (b.kind === "layer" ? 15 : 13) : 12) * scale;
      if (fs >= 6) {
        ctx.font = `600 ${Math.min(fs, 26)}px ${FONT}`;
        ctx.fillStyle = n?.pins ? palette.accentText : palette.fg;
        ctx.textAlign = "center";
        ctx.textBaseline = box ? "middle" : "bottom";
        const label = (n?.steps ? n.steps + "  " : "") + b.label + (n?.pins ? "  ◉ " + n.pins : "");
        ctx.fillText(label, p.x + w / 2, box ? p.y + h / 2 : p.y - 3, box ? Math.max(w - 8, 10) : undefined);
        ctx.textAlign = "start";
      }
      ctx.globalAlpha = 1;
    });
    under.textBaseline = over.textBaseline = "alphabetic";
  }

  $effect(() => {
    setPalette(css);
    const s = (sg = new Sigma<NodeAttrs, EdgeAttrs>(toGraphology([]), el, {
      itemSizesReference: "positions",
      zoomToSizeRatioFunction: (r) => r,
      minCameraRatio: 0.02,
      maxCameraRatio: 20,
      renderEdgeLabels: true,
      enableEdgeEvents: true,
      zIndex: true,
      labelRenderedSizeThreshold: 2.5,
      labelDensity: 100, // every candidate: drawLabel keeps the ones that fit
      labelGridCellSize: 90,
      minEdgeThickness: 0.8,
      labelFont: FONT,
      stagePadding: 30,
      defaultEdgeType: "dash",
      nodeProgramClasses: { circle: NodeCircleProgram as never, image: createNodeImageProgram({ size: { mode: "force", value: 96 }, padding: 0.15 }) as never },
      edgeProgramClasses: { dash: EdgeProgramWithDash as never },
      defaultDrawNodeLabel: drawLabel as never,
      defaultDrawNodeHover: drawHover as never,
      defaultDrawEdgeLabel: drawEdgeLabel as never,
      nodeReducer: nodeReducer(st) as never,
      edgeReducer: edgeReducer(st) as never,
    }));
    // Typed without the layer options, which it passes on.
    s.createCanvasContext("bg", { beforeLayer: "edges" } as object);
    s.createCanvasContext("boxes", { afterLayer: "edges" } as object);
    s.resize(true);
    s.on("beforeRender", () => taken.clear());
    s.on("afterRender", () => {
      drawBackground();
      if (!painted && g?.order) {
        painted = true;
        performance.mark("ig:graph-painted");
      }
    });
    (window as unknown as { igSigma: Sigma }).igSigma = s as unknown as Sigma; // for debugging from the console

    const select = (id: string | null) => {
      st.selected = id;
      refresh();
    };
    /** The click that ends a drag or a link is not a tap. */
    let swallow = false;
    s.on("clickNode", ({ node }) => {
      if (swallow) return void (swallow = false);
      const a = g.getNodeAttributes(node);
      if (!a.anchor) select(node);
      ontap({ id: node, kind: a.kind });
    });
    s.on("clickStage", ({ event }) => {
      if (swallow) return void (swallow = false);
      const p = fromView(event.x, event.y);
      const hit = groupAt(bg, p.x, p.y);
      if (hit) return ontap({ id: hit, kind: kindOf[hit] });
      st.focus = null;
      select(null);
    });
    s.on("enterNode", ({ node }) => {
      st.hovered = node;
      el.style.cursor = "pointer";
      refresh();
    });
    s.on("leaveNode", () => {
      st.hovered = null;
      el.style.cursor = "";
      refresh();
    });
    // Planning gestures, turned into notes by <App>: right-click, drop, shift-drag a link.
    const context = (id: string, edge: boolean, o: MouseEvent | TouchEvent) => {
      const m = o as MouseEvent;
      m.preventDefault?.();
      const [source, target] = edge ? [g.source(id), g.target(id)] : ["", ""];
      oncontext({ id, edge, source, target, x: m.clientX, y: m.clientY });
    };
    s.on("rightClickNode", ({ node, event }) => !g.getNodeAttribute(node, "anchor") && context(node, false, event.original));
    s.on("rightClickEdge", ({ edge, event }) => context(edge, true, event.original));
    const noMenu = (e: Event) => e.preventDefault();
    el.addEventListener("contextmenu", noMenu);
    let dragged: string | null = null;
    let moved = false;
    let linkFrom: string | null = null;
    s.on("downNode", ({ node, event }) => {
      const a = g.getNodeAttributes(node);
      swallow = false;
      if ((event.original as MouseEvent).button) return; // only the main button drags or links
      if ((event.original as MouseEvent).shiftKey) linkFrom = node;
      else if (!a.anchor && !["laneadd", "lane", "lanelabel"].includes(a.kind)) [dragged, moved] = [node, false];
    });
    s.on("moveBody", ({ event }) => {
      if (linkFrom) return event.preventSigmaDefault();
      if (!dragged || !g.hasNode(dragged)) return;
      const p = fromView(event.x, event.y);
      g.mergeNodeAttributes(dragged, { x: p.x, y: -p.y });
      const e = els.find((x) => x.data.id === dragged);
      if (e) e.position = p;
      if (parentOf[dragged] || kindOf[dragged] === "layer" || kindOf[dragged] === "dir") bg = background(els);
      moved = true;
      event.preventSigmaDefault();
      event.original.preventDefault();
    });
    const up = (to: string | null) => {
      swallow = moved || (!!linkFrom && to !== linkFrom);
      if (linkFrom && to && to !== linkFrom) onlink({ from: linkFrom, to });
      if (dragged && moved && g.hasNode(dragged)) ondrop({ id: dragged, y: -g.getNodeAttribute(dragged, "y") });
      [linkFrom, dragged, moved] = [null, null, false];
    };
    s.on("upNode", ({ node }) => up(node));
    s.on("upStage", ({ event }) => {
      const p = fromView(event.x, event.y);
      up(groupAt(bg, p.x, p.y));
    });
    // Theme changes (system or data-theme) recolour everything.
    const media = matchMedia("(prefers-color-scheme: dark)");
    const retheme = () => {
      setPalette(css);
      if (els.length) show(els, { fresh: false, preset: true, keepCamera: true });
    };
    media.addEventListener("change", retheme);
    const themeObs = new MutationObserver(retheme);
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      media.removeEventListener("change", retheme);
      themeObs.disconnect();
      el.removeEventListener("contextmenu", noMenu);
      s.kill();
      if ((window as unknown as { igSigma?: Sigma }).igSigma === (s as unknown)) delete (window as unknown as { igSigma?: Sigma }).igSigma;
    };
  });

  $effect(() => {
    st.links = links;
    refresh();
  });

  export function width() {
    return el.clientWidth;
  }

  /** Fits the camera on `ids` (all nodes, boxes and rows when empty), never closer than 1.4x. */
  function fit(ids: string[]) {
    const home = { x: 0.5, y: 0.5, ratio: 1, angle: 0 };
    // The matrix only follows the camera on the next frame: convert against the home state.
    const at = (x: number, y: number) => sg.graphToViewport({ x, y: -y }, { cameraState: home });
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    const add = (x: number, y: number) => ([x0, y0, x1, y1] = [Math.min(x0, x), Math.min(y0, y), Math.max(x1, x), Math.max(y1, y)]);
    const want = new Set(ids);
    g.forEachNode((id, a) => (!want.size || want.has(id)) && (add(a.x - a.size, -a.y - a.size), add(a.x + a.size, -a.y + a.size)));
    bg.groups.forEach((b) => (!want.size || want.has(b.id)) && (add(b.x0, b.y0), add(b.x1, b.y1)));
    if (!want.size) bg.lanes.forEach((l) => (add(l.x ?? x0, l.y0), add(l.x ?? x0, l.y1)));
    if (x0 === Infinity) return;
    const [p, q] = [at(x0, y0), at(x1, y1)];
    const o = at(0, 0);
    const unit = Math.hypot(at(1, 0).x - o.x, at(1, 0).y - o.y); // pixels per graph unit at ratio 1
    const { width: W, height: H } = sg.getDimensions();
    const ratio = Math.max((q.x - p.x) / Math.max(W - 60, 50), (q.y - p.y) / Math.max(H - 60, 50), unit / 1.4);
    const c = sg.viewportToFramedGraph({ x: (p.x + q.x) / 2, y: (p.y + q.y) / 2 }, { cameraState: home });
    sg.getCamera().setState({ x: c.x, y: c.y, ratio, angle: 0 });
  }

  /** Replaces the elements. Unless `fresh`, laid-out nodes keep their positions, so a new depth or a
   *  rebuild doesn't reshuffle the picture. `zoomTo` fits that box and its neighbours. */
  export function show(next: El[], o: { fresh: boolean; preset: boolean; zoomTo?: string; keepCamera?: boolean }) {
    els = next;
    if (!o.preset) {
      const prev: Record<string, { x: number; y: number }> = {};
      if (!o.fresh && g) g.forEachNode((id, a) => !a.anchor && (prev[id] = { x: a.x, y: -a.y }));
      const p = positions(els, 1, prev);
      els.forEach((e) => e.group === "nodes" && p[e.data.id] && (e.position = p[e.data.id]));
    }
    kindOf = {};
    parentOf = {};
    known = new Set();
    els.forEach((e) => {
      if (e.group !== "nodes") return;
      kindOf[e.data.id] = e.data.kind ?? "";
      parentOf[e.data.id] = e.data.parent;
      known.add(e.data.id);
    });
    g = toGraphology(els);
    g.updateEachNodeAttributes((_, a) => ({ ...a, y: -a.y }));
    g.forEachNode((id) => deco[id] && g.mergeNodeAttributes(id, deco[id]));
    bg = background(els);
    let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
    g.forEachNode((_, a) => void ([x0, y0, x1, y1] = [Math.min(x0, a.x), Math.min(y0, a.y), Math.max(x1, a.x), Math.max(y1, a.y)]));
    bg.groups.forEach((b) => void ([x0, y0, x1, y1] = [Math.min(x0, b.x0), Math.min(y0, -b.y1), Math.max(x1, b.x1), Math.max(y1, -b.y0)]));
    bg.lanes.forEach((l) => void ([x0, y0, y1] = [Math.min(x0, (l.x ?? x0) - 60), Math.min(y0, -l.y1), Math.max(y1, -l.y0)]));
    sg.setGraph(g);
    // A fixed frame: dragging a node or a rebuild must not rescale the view.
    sg.setCustomBBox(x0 === Infinity ? null : { x: [x0, x1], y: [y0, y1] });
    if (st.selected && !g.hasNode(st.selected)) st.selected = null;
    st.focus = null;
    sg.refresh();
    if (!o.keepCamera) {
      const t = o.zoomTo && known.has(o.zoomTo) ? o.zoomTo : null;
      fit(t ? [t, ...(g.hasNode(t) ? g.neighbors(t) : [])] : []);
    }
    refresh();
  }

  /** Shows note pins (ids after the label) and ghosts (open moves) on the drawn nodes. */
  export function decorate(pins: Record<string, string[]>, ghosts: Set<string>) {
    known.forEach((id) => (deco[id] = { ...deco[id], pins: pins[id] ? pins[id].join(" ") : undefined, ghost: ghosts.has(id) }));
    g.forEachNode((id) => g.mergeNodeAttributes(id, { pins: deco[id]?.pins, ghost: deco[id]?.ghost }));
    refresh();
  }

  /** Gold on the drawn `nodes` and `edges` ("source|target"), the rest dimmed, step badges on
   *  their targets (node ids or edge keys); null clears it. */
  export function highlight(h: { nodes: Set<string>; edges: Set<string>; steps: Record<string, number[]> } | null) {
    st.show = h ? { nodes: h.nodes, edges: h.edges } : null;
    known.forEach((id) => (deco[id] = { ...deco[id], steps: h && h.steps[id] ? badge(h.steps[id]) : undefined }));
    g.forEachNode((id) => g.setNodeAttribute(id, "steps", deco[id]?.steps));
    g.forEachEdge((id, a) => {
      const k = a.from + "|" + a.to;
      g.setEdgeAttribute(id, "step", h && h.steps[k] ? badge(h.steps[k]) : undefined);
    });
    refresh();
  }

  /** The drawn layer rows (Architecture), for drops. */
  export function lanes(layers: string[]): Lane[] {
    return bg.lanes.map((l) => ({ row: 0, name: l.label, kind: layers.includes(l.label) ? "layer" : "other", top: l.y0, bottom: l.y1 }));
  }

  export function ids(): string[] {
    return els.filter((e) => e.group === "nodes").map((e) => e.data.id);
  }

  /** The first of `candidates` that is on the graph. */
  export function pick(candidates: (string | null)[]): string | null {
    return candidates.find((c) => c && known.has(c)) ?? null;
  }

  export function select(id: string) {
    st.selected = id;
    refresh();
  }

  /** Unselects everything and clears the fading. */
  export function clear() {
    st.selected = null;
    st.focus = null;
    refresh();
  }

  /** The node, its compound and its ancestors, for fading. */
  function withAncestors(ids: Iterable<string>): Set<string> {
    const out = new Set<string>();
    for (let id of ids) {
      out.add(id);
      while (parentOf[id]) out.add((id = parentOf[id]!));
    }
    return out;
  }

  /** Fades everything but `id` (with what it boxes), its neighbours and their parents. */
  export function fadeAround(id: string) {
    const inside = [...known].filter((x) => withAncestors([x]).has(id));
    st.focus = withAncestors([...inside, ...(g.hasNode(id) ? g.neighbors(id) : [])]);
    refresh();
  }

  /** Fades everything but the nodes `keep` accepts (and their parents); nothing if none does. */
  export function fadeExcept(keep: (id: string) => boolean) {
    const xs = [...known].filter(keep);
    st.focus = xs.length ? withAncestors(xs) : null;
    refresh();
  }

  export function center(id: string) {
    let p: { x: number; y: number } | null = null;
    if (g.hasNode(id)) p = { x: g.getNodeAttribute(id, "x"), y: -g.getNodeAttribute(id, "y") };
    const b = bg.groups.find((x) => x.id === id);
    if (!p && b) p = { x: (b.x0 + b.x1) / 2, y: (b.y0 + b.y1) / 2 };
    if (!p) return;
    const v = toView(p.x, p.y);
    const f = sg.viewportToFramedGraph(v);
    sg.getCamera().animate({ x: f.x, y: f.y }, { duration: 200 });
  }
</script>

<div id="cy" role="img" aria-label="Code graph" bind:this={el}></div>
