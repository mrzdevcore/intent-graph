<!-- The Design view's screens: the tool rail with the UI pieces palette, and the inspector
     (a screen's title, states and background; a piece's label and props; a nav's label). The
     engine draws the paper-sketch screens on the same board as the architecture (one history,
     one store). Images come in by paste, drop or I: stored through putAsset, then decoded here
     for their texture. Screen and image changes on disk merge in while the board is open. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { AppWindow, ArrowUpRight, ChevronDown, CreditCard, Heading, Image, ImagePlus, Link, List, Monitor, MousePointer2,
    PanelTop, PenLine, RectangleHorizontal, Shapes, Smile, SquareCheck, Table, TextCursorInput, Type } from "lucide-static";
  import type { Design, PieceKind, Screen, ScreenPiece, ScreensApi } from "../api";
  import type { Index } from "../model/graph";
  import Board from "./Board.svelte";
  import { bridgeChain, pickerItems, requirement } from "./bridge";
  import BridgeChain from "./BridgeChain.svelte";
  import type { BoardEngine, EngineOut, Tool } from "./engine";
  import Picker from "./Picker.svelte";
  import type { BoardStore } from "./store";

  let { active, slug, store, api, engine, act, tool, selection, version, onpick, ix, context, hover = null, openCode }: {
    /** The screens surface is the one shown. */
    active: boolean;
    slug: string | null;
    store: BoardStore | null;
    api: ScreensApi | null;
    engine: () => BoardEngine | null;
    /** One engine call that shows on the board and is saved. */
    act: (f: (e: BoardEngine) => EngineOut) => EngineOut | undefined;
    tool: Tool;
    selection: string[];
    /** Bumped when the board or the store's state changed. */
    version: number;
    onpick: (t: Tool) => void;
    ix: Index;
    /** The architecture's context JSON: the mini-board draws it. */
    context: string;
    /** What the pointer is over on the screens. */
    hover?: string | null;
    openCode?: (file: string, line: number) => void;
  } = $props();

  const TOOLS: [Tool, string, string, string][] = [
    ["select", "V", "Select", MousePointer2], ["screen", "S", "Screen frame", Monitor], ["pieces", "U", "UI pieces", Shapes],
    ["pen", "P", "Pen", PenLine], ["arrow", "A", "Arrow (onto another screen: a nav)", ArrowUpRight], ["piece:text", "X", "Text", Type],
    ["import", "I", "Import image (PNG or JPEG; or paste, or drop)", ImagePlus], ["bridge", "K", "Link to the architecture", Link],
  ];
  const PIECES: [PieceKind, string, string, string][] = [
    ["button", "B", "Button", RectangleHorizontal], ["input", "N", "Input", TextCursorInput], ["select", "O", "Select", ChevronDown],
    ["checkbox", "C", "Checkbox", SquareCheck], ["table", "T", "Table", Table], ["list", "L", "List", List],
    ["card", "D", "Card", CreditCard], ["tabs", "J", "Tabs", PanelTop], ["modal", "M", "Modal", AppWindow],
    ["heading", "H", "Heading", Heading], ["image", "G", "Image", Image], ["icon", "Y", "Icon", Smile],
  ];
  type Field = "text" | "list" | "bool" | "num" | "level";
  const FIELDS: Partial<Record<PieceKind, [keyof NonNullable<ScreenPiece["props"]>, Field][]>> = {
    button: [["text", "text"], ["primary", "bool"]], input: [["placeholder", "text"]], select: [["options", "list"]],
    checkbox: [["checked", "bool"]], table: [["columns", "list"], ["rows", "num"]], list: [["items", "num"]],
    tabs: [["tabs", "list"], ["active", "num"]], modal: [["title", "text"]], text: [["text", "text"]],
    heading: [["text", "text"], ["level", "level"]], icon: [["name", "text"]], note: [["text", "text"]],
  };
  const STATES = ["loading", "error", "empty"];

  let picker = $state<HTMLInputElement>();
  let custom = $state("");

  const screens = $derived.by((): Record<string, Screen> => {
    void version;
    try {
      const e = engine();
      return e ? Object.fromEntries(Object.entries(e.screens()).map(([id, t]) => [id, JSON.parse(t) as Screen])) : {};
    } catch {
      return {};
    }
  });
  const sel = $derived(selection.length === 1 ? selection[0] : null);
  const nav = $derived.by(() => {
    const m = sel?.match(/^nav:([^/]+)\/([^>]+)>(.+)$/);
    const s = m && screens[m[1]];
    const n = s?.navs.find((x) => x.from === m![2] && x.to === m![3]);
    return s && n ? { screen: s, n } : null;
  });
  const piece = $derived.by(() => {
    if (!sel || nav || !sel.includes("/")) return null;
    const [sid, pid] = sel.split("/");
    const p = screens[sid]?.elements.find((x) => x.id === pid);
    return p ? { screen: sid, p } : null;
  });
  const screen = $derived(sel && !nav && !piece ? screens[sel] : undefined);
  const palette = $derived(tool === "pieces" || (tool.startsWith("piece:") && tool !== "piece:text"));
  const readOnly = $derived.by(() => {
    void version;
    return store ? store.errors[store.screensPath] ?? null : null;
  });
  const imageError = $derived.by(() => {
    void version;
    return store?.imageError ?? null;
  });

  const ops = (list: object[]) => act((e) => e.ops(JSON.stringify(list), "user"));

  // The bridge: a piece linked to the architecture, by a drag onto the mini-board, a click in
  // it, or K and the picker; each is one undo step.
  const EMPTY_CONTEXT = '{"lanes":[],"boxes":[],"edges":[]}';
  const BLANK = '{"version":1,"title":"","scope":{"kind":"dir","id":"."}}';
  let mini = $state<ReturnType<typeof Board>>();
  let miniHost = $state<HTMLDivElement>();
  /** The piece the picker links, while open. */
  let picking = $state<string | null>(null);
  let hint = $state<string | null>(null);
  const design = $derived.by((): Design | null => {
    void version;
    try {
      const e = engine();
      return e ? (JSON.parse(e.board()) as Design) : null;
    } catch {
      return null;
    }
  });
  const board = $derived(design ? { design, screens } : null);
  const linked = (k: string | null) => (k && design?.bridges.find((b) => b.from === k)) || null;
  /** The linked piece hovered, else the one selected. */
  const bridge = $derived(linked(hover) ?? linked(piece ? sel : null));
  const describe = (k: string) => {
    const [sid, pid] = k.split("/");
    const p = screens[sid]?.elements.find((x) => x.id === pid);
    return p ? `${p.label} (${screens[sid].title})` : k;
  };

  function link(from: string, to: string) {
    hint = null;
    picking = null;
    const o = ops([{ op: "bridge", from, to }]);
    if (o?.error) hint = o.error;
  }

  /** A piece let go outside the screens at a client point: linked to what the mini-board has there. */
  export function dropAt(id: string, cx: number, cy: number) {
    const r = miniHost?.getBoundingClientRect();
    if (!r || cx < r.left || cx > r.right || cy < r.top || cy > r.bottom) return;
    const t = mini?.run((m) => m.hit(cx - r.left, cy - r.top));
    if (t) link(id, t);
  }

  function miniOut(o: EngineOut) {
    if (!o.picked) return;
    if (piece && sel) link(sel, o.picked);
    else hint = "Select a piece first: a click here links it.";
  }

  // a hint is about the selection it was given for
  $effect(() => {
    void sel;
    hint = null;
  });

  // the mini-board follows the board and its context
  let miniContext = "";
  $effect(() => {
    void version;
    const c = context;
    untrack(() => {
      const fresh = mini?.run((m) => {
        const ctx = !!c && c !== miniContext;
        if (ctx) m.setContext((miniContext = c));
        const e = engine();
        const o = e ? m.external(e.board(), null) : null;
        m.dismiss(null);
        return ctx || !!o?.changed;
      });
      // an overview: fitted again whenever what it shows changed
      if (fresh) mini?.fit();
    });
  });

  // K: the picker for the selected piece
  $effect(() => {
    if (active && tool === "bridge") untrack(() => {
      onpick("select");
      if (piece && sel) picking = sel;
      else hint = "Select a piece, then K links it to the architecture.";
    });
  });
  const setPiece = (screen: string, id: string, set: object) => ops([{ op: "screen_update", screen, id, set }]);
  const setScreen = (id: string, set: object) => ops([{ op: "screen_set", id, set }]);

  function prop(screen: string, p: ScreenPiece, name: string, f: Field, el: HTMLInputElement | HTMLSelectElement) {
    const raw = el instanceof HTMLInputElement && f === "bool" ? el.checked : el.value;
    const v = f === "list" ? String(raw).split(",").map((x) => x.trim()).filter(Boolean)
      : f === "num" || f === "level" ? (String(raw) === "" ? null : Math.max(0, Math.min(255, Math.round(Number(raw)))))
      : f === "bool" ? raw || null : String(raw) === "" ? null : raw;
    setPiece(screen, p.id, { props: { [name]: v } });
  }

  async function take(f: File) {
    await store?.importImage(new Uint8Array(await f.arrayBuffer()));
  }

  // I (or the rail's button): the file picker, then back to select
  $effect(() => {
    if (active && tool === "import") untrack(() => {
      picker?.click();
      onpick("select");
    });
  });

  // paste or drop an image while the screens are shown
  $effect(() => {
    if (!active || readOnly) return;
    const typing = (t: EventTarget | null) => !!(t as HTMLElement | null)?.closest?.("input, textarea, select, [contenteditable]");
    const paste = (e: ClipboardEvent) => {
      const f = e.clipboardData?.files?.[0];
      if (!f || typing(e.target)) return;
      e.preventDefault();
      void take(f);
    };
    const over = (e: DragEvent) => e.dataTransfer?.types.includes("Files") && e.preventDefault();
    const drop = (e: DragEvent) => {
      const f = e.dataTransfer?.files?.[0];
      if (!f) return;
      e.preventDefault();
      void take(f);
    };
    window.addEventListener("paste", paste);
    window.addEventListener("dragover", over);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("paste", paste);
      window.removeEventListener("dragover", over);
      window.removeEventListener("drop", drop);
    };
  });

  // screens and images changed on disk (Claude's design_edit, git, an editor): merged in
  $effect(() => {
    const s = slug, a = api;
    if (!s || !a) return;
    const offScreens = a.onScreenChanged((x) => x === s && store?.slug === s && void store.externalScreens());
    const offAssets = a.onAssetChanged((x) => x === s && store?.slug === s && void store.loadImages());
    return () => {
      offScreens();
      offAssets();
    };
  });
</script>

<input type="file" accept="image/png,image/jpeg" hidden bind:this={picker}
  onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) void take(f); e.currentTarget.value = ""; }}>

{#if active}
  {#if picking && board}
    <Picker piece={describe(picking)} items={(q) => pickerItems(ix.D, board, q)} onpick={(id) => link(picking!, id)} onclose={() => (picking = null)} />
  {/if}
  {#if bridge && board}
    <BridgeChain chain={bridgeChain(ix.D, board, bridge.from)} to={bridge.to} requirement={requirement(board, bridge.from)} open={openCode}
      onunlink={() => ops([{ op: "unbridge", key: `${bridge.from}>${bridge.to}` }])} />
  {/if}
  <div class="rail" role="toolbar" aria-label="Screen tools" aria-orientation="vertical">
    {#each TOOLS as [t, key, name, icon] (t)}
      <button class="icon" title="{name} ({key})" aria-label="{name} ({key})" aria-pressed={tool === t || (t === "pieces" && palette) ? "true" : "false"}
        disabled={!store || !!readOnly} onclick={() => onpick(t === "pieces" && palette ? "select" : t)}>{@html icon}<kbd>{key}</kbd></button>
    {/each}
    {#if palette}
      <div class="palette" role="toolbar" aria-label="UI pieces" aria-orientation="vertical">
        {#each PIECES as [k, key, name, icon] (k)}
          <button title="{name} ({key})" aria-pressed={tool === `piece:${k}` ? "true" : "false"} onclick={() => onpick(`piece:${k}`)}>
            {@html icon}<span>{name}</span><kbd>{key}</kbd></button>
        {/each}
      </div>
    {/if}
  </div>

  <aside class="inspector" aria-label="Screen inspector">
    <div class="mini" bind:this={miniHost} title="The architecture: drag a piece here, or select one and click a box, to link it (K: by name)">
      <Board bind:this={mini} pick label="The architecture, to link pieces to; the element list follows"
        source={() => ({ board: engine()?.board() ?? BLANK, context: context || EMPTY_CONTEXT })} onout={miniOut} />
    </div>
    {#if hint}<p class="muted" role="status">{hint}</p>{/if}
    {#if readOnly}<p class="problem" role="alert">Screens read-only: {readOnly}. No screen file is written until it is fixed.</p>{/if}
    {#if imageError}<p class="problem" role="alert">{imageError}</p>{/if}
    {#if screen}
      {#key screen.id}
        <h3>Screen {#if screen.state}<span class="muted">· {screen.state}</span>{/if}</h3>
        {#if screen.variantOf}<p class="muted">A state of {screens[screen.variantOf]?.title ?? screen.variantOf}</p>{/if}
        <label>Title <input value={screen.title} onchange={(e) => setScreen(screen.id, { title: e.currentTarget.value })}></label>
        <div class="row">
          <label>W <input type="number" value={screen.w} onchange={(e) => setScreen(screen.id, { w: +e.currentTarget.value })}></label>
          <label>H <input type="number" value={screen.h} onchange={(e) => setScreen(screen.id, { h: +e.currentTarget.value })}></label>
        </div>
        <p class="muted">Add a state, a copy next to it:</p>
        <div class="states">
          {#each STATES as st (st)}<button onclick={() => act((e) => e.variant(screen.id, st))}>{st}</button>{/each}
        </div>
        <form class="row" onsubmit={(e) => { e.preventDefault(); if (custom.trim()) act((en) => en.variant(screen.id, custom)); custom = ""; }}>
          <input bind:value={custom} placeholder="another state" aria-label="Another state"><button>Add</button>
        </form>
        {#if screen.background}
          <p class="muted">Background: <code>{screen.background.asset}</code> (locked)
            <button onclick={() => setScreen(screen.id, { background: null })}>Remove</button></p>
        {:else}
          <button onclick={() => picker?.click()}>Import image… (I)</button>
        {/if}
      {/key}
    {:else if piece}
      {#key sel}
        <h3>{piece.p.kind[0].toUpperCase() + piece.p.kind.slice(1)} <span class="muted">{piece.screen}</span></h3>
        <label>Label <input value={piece.p.label} onchange={(e) => setPiece(piece.screen, piece.p.id, { label: e.currentTarget.value })}></label>
        {#each FIELDS[piece.p.kind] ?? [] as [name, f] (name)}
          {@const v = piece.p.props?.[name]}
          {#if f === "bool"}
            <label class="check"><input type="checkbox" checked={!!v} onchange={(e) => prop(piece.screen, piece.p, name, f, e.currentTarget)}> {name}</label>
          {:else if f === "level"}
            <label>{name} <select value={String(v ?? 1)} onchange={(e) => prop(piece.screen, piece.p, name, f, e.currentTarget)}>
              {#each [1, 2, 3] as l (l)}<option value={String(l)}>{l}</option>{/each}</select></label>
          {:else}
            <label>{name}{f === "list" ? " (comma separated)" : ""}
              <input type={f === "num" ? "number" : "text"} value={Array.isArray(v) ? v.join(", ") : (v ?? "")}
                onchange={(e) => prop(piece.screen, piece.p, name, f, e.currentTarget)}></label>
          {/if}
        {/each}
        {#if piece.p.kind === "image" && piece.p.props?.asset}<p class="muted">Image: <code>{piece.p.props.asset}</code></p>{/if}
        <div class="row">
          <label>W <input type="number" value={piece.p.w} onchange={(e) => setPiece(piece.screen, piece.p.id, { w: +e.currentTarget.value })}></label>
          <label>H <input type="number" value={piece.p.h} onchange={(e) => setPiece(piece.screen, piece.p.id, { h: +e.currentTarget.value })}></label>
        </div>
      {/key}
    {:else if nav}
      <h3>Nav</h3>
      <p><code>{nav.n.from}</code> → {screens[nav.n.to]?.title ?? nav.n.to}</p>
      <label>Label <input value={nav.n.label} onchange={(e) => ops([{ op: "nav", screen: nav.screen.id, from: nav.n.from, to: nav.n.to, label: e.currentTarget.value }])}></label>
      <button onclick={() => ops([{ op: "unnav", screen: nav.screen.id, from: nav.n.from, to: nav.n.to }])}>Remove</button>
    {:else if selection.length > 1}
      <h3>{selection.length} selected</h3>
      <p class="muted">Arrows nudge (Shift: 40 px), Delete removes; Cmd+D duplicates pieces.</p>
    {:else}
      <h3>Screens</h3>
      <p class="muted">S draws a screen; U opens the UI pieces (B, N, O, C, T, L, D, J, M, H, G, Y), then click in a screen.
        P draws, A draws an arrow (from a piece onto another screen: a nav), X writes. Paste or drop a PNG or JPEG for a
        locked background. Double-click or Enter renames.</p>
    {/if}
  </aside>
{/if}

<style>
  .rail { grid-row: 2; grid-column: 1; position: relative; display: flex; flex-direction: column; gap: 4px; padding: 6px;
    border-right: 1px solid var(--line); background: var(--panel); }
  .icon { position: relative; display: grid; place-items: center; width: 34px; height: 34px; padding: 0; border-radius: 6px; }
  .rail :global(svg) { width: 18px; height: 18px; flex: none; }
  .icon[aria-pressed="true"], .palette button[aria-pressed="true"] { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
  kbd { position: absolute; right: 2px; bottom: 0; font-size: 9px; color: inherit; opacity: 0.7; font-family: inherit; }
  .palette { position: absolute; left: 100%; top: 6px; z-index: 4; display: flex; flex-direction: column; gap: 2px; padding: 6px;
    background: var(--panel); border: 1px solid var(--line); border-radius: 6px; max-height: calc(100% - 12px); overflow: auto; }
  .palette button { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: 4px; text-align: left; font-size: 13px; }
  .palette span { flex: 1; }
  .palette kbd { position: static; opacity: 0.7; }
  .inspector { grid-row: 2; grid-column: 3; border-left: 1px solid var(--line); background: var(--panel); padding: 10px; overflow: auto; font-size: 13px; }
  .inspector h3 { margin: 0 0 8px; font-size: 14px; }
  .inspector label { display: flex; flex-direction: column; gap: 2px; margin-bottom: 8px; color: var(--muted); }
  .inspector label.check { flex-direction: row; align-items: center; gap: 6px; color: var(--fg); }
  .inspector input:not([type="checkbox"]), .inspector select { color: var(--fg); background: var(--bg); border: 1px solid var(--line);
    border-radius: 4px; padding: 3px 6px; font: inherit; min-width: 0; }
  .row { display: flex; gap: 6px; align-items: flex-end; margin-bottom: 8px; }
  .row label { flex: 1; margin: 0; min-width: 0; }
  .row input { width: 100%; box-sizing: border-box; }
  .states { display: flex; gap: 6px; margin-bottom: 8px; flex-wrap: wrap; }
  .muted { color: var(--muted); }
  .mini { position: relative; height: 180px; margin: -4px -4px 8px; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
  .problem { color: var(--error); }
</style>
