<!-- The bridge on the screens: the architecture as a read-only pick board (the same instance
     idle as an overview at the top of the inspector, docked over the right half of the canvas
     in link mode), the picker, the hovered piece's badge and the selected piece's call chain.
     Link mode starts with K or by dragging a piece past the canvas edge; a drop or click on a
     box, or Enter in the picker, links (one undo step); Esc leaves it. -->
<script lang="ts">
  import { untrack } from "svelte";
  import type { Design, Screen } from "../api";
  import type { Index } from "../model/graph";
  import Board from "./Board.svelte";
  import { Bridges, pickerItems, requirement } from "./bridge";
  import BridgeChain from "./BridgeChain.svelte";
  import type { BoardEngine, EngineOut } from "./engine";
  import Picker from "./Picker.svelte";

  let { ix, context, version, engine, screens, selected, hover, ops, openCode, canvas, ondock }: {
    ix: Index;
    /** The architecture's context JSON. */
    context: string;
    /** Bumped when the board changed. */
    version: number;
    engine: () => BoardEngine | null;
    screens: Record<string, Screen>;
    /** The selected piece's key, if one piece is selected. */
    selected: string | null;
    /** What the pointer is over on the screens. */
    hover: string | null;
    ops: (list: object[]) => EngineOut | undefined;
    openCode?: (file: string, line: number) => void;
    /** The screens' canvas cell: where a drag leaves it, and where focus goes back. */
    canvas: () => HTMLElement | undefined;
    ondock: (docked: boolean) => void;
  } = $props();

  const EMPTY_CONTEXT = '{"lanes":[],"boxes":[],"edges":[]}';
  const BLANK = '{"version":1,"title":"","scope":{"kind":"dir","id":"."}}';
  const bridges = new Bridges();
  let mini = $state<ReturnType<typeof Board>>();
  let host = $state<HTMLDivElement>();
  /** Link mode: the piece being linked. */
  let linking = $state<string | null>(null);
  let hint = $state<string | null>(null);
  /** A piece being dragged on the screens (it may leave the canvas). */
  let dragging: string | null = null;

  const design = $derived.by((): Design | null => {
    void version;
    try {
      const e = engine();
      return e ? (JSON.parse(e.board()) as Design) : null;
    } catch {
      return null;
    }
  });
  const isPiece = (k: string | null): k is string => !!k && k.includes("/") && !k.startsWith("nav:");
  const bridgeOf = (k: string | null) => (k && design?.bridges.find((b) => b.from === k)) || null;
  const titleOf = (k: string) => screens[k.split("/")[0]]?.title ?? k.split("/")[0];
  const chainOf = (k: string) => (design ? bridges.chain(ix.D, design, k, titleOf(k)) : null);
  const sel = $derived(bridgeOf(selected));
  const hovered = $derived(hover !== selected ? bridgeOf(hover) : null);
  const badge = $derived.by(() => {
    if (!hovered) return null;
    const c = chainOf(hovered.from);
    return c ? `→ ${c.steps[0].label} · ${c.steps.length} ${c.steps.length === 1 ? "step" : "steps"}` : `→ ${hovered.to} (not in the graph yet)`;
  });
  const describe = (k: string) => {
    const [sid, pid] = k.split("/");
    const p = screens[sid]?.elements.find((x) => x.id === pid);
    return p ? `${p.label} (${screens[sid].title})` : k;
  };

  function dock(k: string) {
    linking = k;
    hint = null;
    ondock(true);
  }

  /** Leaves link mode; focus back on the screens. */
  export function collapse() {
    if (!linking) return;
    linking = null;
    ondock(false);
    canvas()?.querySelector("canvas")?.focus();
  }

  /** K: link mode for the selected piece. */
  export function start() {
    if (isPiece(selected)) dock(selected);
    else hint = "Select a piece, then K links it to the architecture.";
  }

  function link(from: string, to: string) {
    const o = ops([{ op: "bridge", from, to }]);
    hint = o?.error ?? null;
    if (!o?.error) collapse();
  }

  const inside = (r: DOMRect, x: number, y: number) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;

  /** A piece let go outside the screens' view at a client point: linked to the box there. */
  export function dropAt(id: string, cx: number, cy: number) {
    const r = host?.getBoundingClientRect();
    if (!r || !inside(r, cx, cy)) return;
    const t = mini?.run((m) => m.hit(cx - r.left, cy - r.top));
    if (t) link(id, t);
  }

  function miniOut(o: EngineOut) {
    if (!o.picked) return;
    const k = linking ?? (isPiece(selected) ? selected : null);
    if (k) link(k, o.picked);
    else hint = "Select a piece first: a click here links it.";
  }

  // a piece dragged past the canvas edge docks the board, and the box under the pointer lights up
  $effect(() => {
    const down = (e: PointerEvent) => {
      const c = canvas();
      dragging = c?.contains(e.target as Node) && isPiece(hover) ? hover : null;
    };
    const move = (e: PointerEvent) => {
      if (!dragging || !(e.buttons & 1)) return;
      const c = canvas()?.getBoundingClientRect();
      if (!linking && c && !inside(c, e.clientX, e.clientY)) dock(dragging);
      const r = host?.getBoundingClientRect();
      if (linking && r) mini?.run((m) => m.input({ t: "move", x: e.clientX - r.left, y: e.clientY - r.top, mods: 0 }));
    };
    const up = () => (dragging = null);
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
  });

  // a hint is about the selection it was given for
  $effect(() => {
    void selected;
    hint = null;
  });

  // the board follows the design and its context, refitted when what it shows changed
  let shown = "";
  $effect(() => {
    void version;
    const c = context;
    untrack(() => {
      const fresh = mini?.run((m) => {
        const ctx = !!c && c !== shown;
        if (ctx) m.setContext((shown = c));
        const e = engine();
        const o = e ? m.external(e.board(), null) : null;
        m.dismiss(null);
        return ctx || !!o?.changed;
      });
      if (fresh) mini?.fit();
    });
  });

  // docked or not, the board fits its new size
  $effect(() => {
    void linking;
    untrack(() => mini?.fit());
  });
  $effect(() => () => ondock(false));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="dock" class:docked={!!linking} bind:this={host} onkeydown={(e) => e.key === "Escape" && (e.preventDefault(), collapse())}
  title={linking ? undefined : "The architecture: drag a piece here, or select one and click a box, to link it (K: by name)"}>
  <Board bind:this={mini} pick label="The architecture, to link pieces to; the element list follows"
    source={() => ({ board: engine()?.board() ?? BLANK, context: context || EMPTY_CONTEXT })} onout={miniOut} />
  {#if hint}<p class="hint" role="status">{hint}</p>{/if}
  {#if linking && design}
    <Picker piece={describe(linking)} items={(q) => pickerItems(ix.D, { design, screens }, q)} onpick={(id) => link(linking!, id)} onclose={collapse} />
  {/if}
</div>
{#if badge}<div class="badge" aria-hidden="true">{badge}</div>{/if}
{#if sel && design}
  <BridgeChain chain={chainOf(sel.from)} to={sel.to} requirement={requirement({ design, screens }, sel.from)} open={openCode}
    onunlink={() => ops([{ op: "unbridge", key: `${sel.from}>${sel.to}` }])} />
{/if}

<style>
  /* idle: an overview at the top of the inspector; docked: the right half of the canvas */
  .dock { grid-row: 2; grid-column: 3; align-self: start; position: relative; z-index: 2; height: 180px; margin: 10px;
    border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
  .dock.docked { grid-column: 2; justify-self: end; align-self: stretch; width: 50%; height: auto; margin: 0; z-index: 4;
    border-width: 0 0 0 2px; border-color: var(--accent); border-radius: 0; }
  .hint { position: absolute; left: 0; right: 0; bottom: 0; z-index: 5; margin: 0; padding: 4px 8px; font-size: 12px;
    color: var(--fg); background: var(--panel); border-top: 1px solid var(--line); }
  .badge { grid-row: 2; grid-column: 2; align-self: start; justify-self: start; z-index: 3; margin: 8px; padding: 2px 8px;
    pointer-events: none; font-size: 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; color: var(--fg); }
</style>
