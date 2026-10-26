<!-- The Design view's architecture board: the feature switcher and scope picker, the tool rail,
     the engine's board over the faded real system, the inspector, the action bar (readiness,
     Plan, Apply) and the checks bar. The store autosaves to plans/<slug>/design.json and
     re-checks on every change of meaning. While the chat's session is the feature's, the
     outbox tells Claude what the user changed on the board. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { Box, Cloud, Frame, Maximize, MousePointer2, Redo2, Route, SquareFunction, Spline, StickyNote, Undo2 } from "lucide-static";
  import type { BoardStatus, Design, DesignApi, DesignScope, ScreensApi } from "../api";
  import type { ChatSession } from "../components/chat/session.svelte";
  import { openQuestions } from "../model/chat";
  import type { Group, Index } from "../model/graph";
  import Board from "./Board.svelte";
  import Checks from "./Checks.svelte";
  import Features from "./Features.svelte";
  import Inspector from "./Inspector.svelte";
  import { buildContext, scopeGroup } from "./context";
  import type { A11yItem, BoardEngine, EngineOut, Surface, Tool } from "./engine";
  import ScreensBoard from "./ScreensBoard.svelte";
  import type { Importer } from "./host";
  import { BoardStore, realClock, targets, type StoreEngine } from "./store";
  import { Outbox, applyPrompt, planPrompt } from "./outbox";
  import { applyState, noteChips, openNotes, readiness } from "./ready";

  let { ix, api, graphScope, around = null, initial = null, beforeClose, importer, chat = null, screensApi = null, initialSurface = null, openCode, initialLink = false }: {
    ix: Index;
    /** Opens a file in the Code view (the bridge's call chain). */
    openCode?: (file: string, line: number) => void;
    api: DesignApi;
    /** The Graph's Architecture scope: offered as a "folder" scope. */
    graphScope: Group;
    /** "Design around this" from the Graph: a node id, with a counter so the same id asks again. */
    around?: { id: string; n: number } | null;
    /** The feature to open first (the IG_DESIGN dev hook), before the remembered one. */
    initial?: string | null;
    /** Runs `cb` (awaited) before the app window closes; returns the unsubscribe. */
    beforeClose?: (cb: () => Promise<void>) => () => void;
    /** Tests: a fake engine. */
    importer?: Importer;
    /** The chat's session (desktop app): Plan, Apply and the board updates go through it. */
    chat?: ChatSession | null;
    /** The feature's screens and images (desktop app): the Screens surface. */
    screensApi?: ScreensApi | null;
    /** The surface to show first (the IG_SURFACE dev hook). */
    initialSurface?: string | null;
    /** Link mode open on the first piece (the IG_LINK dev hook). */
    initialLink?: boolean;
  } = $props();

  const scopeKey = (s: DesignScope) => JSON.stringify(s);
  const FEATURE_KEY = "ig.design.feature";
  const WIDTH = 1200;
  const TOOLS: [Tool, string, string, string][] = [
    ["select", "V", "Select", MousePointer2], ["module", "M", "Module", Box], ["function", "F", "Function", SquareFunction],
    ["route", "R", "Route", Route], ["system", "E", "External system", Cloud], ["link", "L", "Link", Spline],
    ["note", "N", "Sticky note", StickyNote], ["frame", "G", "Frame", Frame],
  ];
  const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
  const CMD = isMac ? "Cmd" : "Ctrl";
  const CMD_BIT = isMac ? 8 : 4;
  const BLANK = JSON.stringify({ version: 1, title: "", scope: { kind: "dir", id: "." } });

  let board = $state<ReturnType<typeof Board>>();
  let features = $state<ReturnType<typeof Features>>();
  let engine: BoardEngine | null = null;
  let store = $state.raw<BoardStore | null>(null);
  let slug = $state<string | null>(null);
  let doc = $state.raw<Design | null>(null);
  let selection = $state.raw<string[]>([]);
  let tool = $state<Tool>("select");
  /** Bumped when the store's status, errors or check change. */
  let version = $state(0);
  let boxes = $state.raw<Map<string, { id: string; label: string; kind: string }>>(new Map());

  const scope = $derived<DesignScope>(doc?.scope ?? { kind: "dir", id: "." });

  const remember = (s: string) => {
    try { localStorage.setItem(FEATURE_KEY, s); } catch { /* storage blocked: for this page only */ }
  };
  const remembered = (): string | null => {
    try { return localStorage.getItem(FEATURE_KEY); } catch { return null; } // storage blocked
  };

  /** The context for a scope, also kept for the inspector's box lookups. */
  function context(s: DesignScope): string {
    const json = buildContext(ix, s, WIDTH);
    boxes = new Map((JSON.parse(json).boxes as { id: string; label: string; kind: string }[]).map((b) => [b.id, b]));
    contextJson = json;
    return json;
  }

  function readDoc() {
    try {
      doc = engine ? (JSON.parse(engine.board()) as Design) : null;
      items = engine?.a11y() ?? [];
    } catch {
      doc = null;
    }
  }

  const NOOP: EngineOut = { redraw: false, changed: false, meaning: false, selection: [], edit: null, placed: null, cursor: "default", error: null };
  /** The store's engine: calls that change what is drawn go through the board, so they show. */
  const adapter: StoreEngine = {
    load: (b, c) => {
      board?.run((e) => e.load(b, c));
      board?.fit();
    },
    ops: (o, s) => board?.run((e) => e.ops(o, s)) ?? NOOP,
    external: (d, t) => {
      const o = board?.run((e) => e.external(d, t)) ?? NOOP;
      // an outside edit with nothing queued: not news for Claude (its own edits: agentMerged)
      if (!t && o.changed && outbox && !outbox.pending) rebase();
      return o;
    },
    setCheck: (t) => void board?.run((e) => e.setCheck(t)),
    board: () => engine!.board(),
    saved: (w) => engine!.saved(w),
    screens: () => engine!.screens(),
    setImage: (a, b) => void board?.run((e) => e.setImage(a, b)),
    importImage: (a) => board?.run((e) => e.importImage(a)) ?? NOOP,
    dropImage: (a) => void board?.run((e) => e.dropImage(a)),
  };

  async function open(next: string) {
    if (!engine || next === store?.slug) return;
    const old = store;
    if (old) {
      await old.finish();
      old.dispose();
    }
    kept = null;
    slug = next;
    remember(next);
    selection = [];
    planStatus = null;
    if (outbox) outbox.onchange = () => {};
    outbox = null;
    const s = new BoardStore(api, adapter, next, realClock, {
      context: () => context(scope),
      nodes: () => ix.D.nodes,
      changed: () => {
        readDoc();
        version++;
      },
      turns: () => chat?.turns.filter((t) => t.feature === next) ?? [],
      agentMerged: (d) => engine && outbox?.agent(d, s.text()),
    }, screensApi);
    store = s;
    // the context follows the file's scope, which is only known once it is read
    await s.load();
    if (store !== s) return;
    attach(next);
    void refreshStatus();
    void chat?.adopt(next);
    const u = await api.unsaved(next).catch((e: unknown) => (console.error(e), null));
    if (store === s) kept = u;
    if (around && around.n !== usedAround) {
      usedAround = around.n;
      setScope({ kind: "around", id: around.id, depth: 1 });
    }
  }

  async function start() {
    const list = (await features?.refresh()) ?? [];
    const want = initial ?? remembered();
    const first = list.find((f) => f.slug === want && f.design) ?? list.find((f) => f.design && !f.done) ?? list.find((f) => f.design);
    if (first) await open(first.slug);
  }

  function ready(e: BoardEngine) {
    engine = e;
    if (initialSurface === "screens" && screensApi) surface = e.surface("screens");
    void start();
  }

  /** The architecture context drawn now (the screens' mini-board shows it too). */
  let contextJson = $state("");
  /** What the pointer is over on the board, and the screens panel (a piece dragged out of it). */
  let hover = $state<string | null>(null);
  let screens = $state<ReturnType<typeof ScreensBoard>>();
  let canvasEl = $state<HTMLDivElement>();
  /** Link mode: the architecture is docked over the canvas's right half. */
  let docked = $state(false);

  function onout(o: EngineOut, e: BoardEngine) {
    if (o.dropped && canvasEl) {
      const r = canvasEl.getBoundingClientRect();
      screens?.dropAt(o.dropped.id, r.left + o.dropped.x, r.top + o.dropped.y);
    }
    selection = o.selection;
    tool = e.tool();
    store?.onOut(o);
    if (!o.changed) readDoc();
    else queue();
  }

  /** One engine step that shows on the board; the store saves and checks it. */
  function act(f: (e: BoardEngine) => EngineOut) {
    const o = board?.run(f);
    if (o) store?.onOut(o);
    if (o?.changed) queue();
    return o;
  }
  const ops = (list: object[]) => act((e) => e.ops(JSON.stringify(list), "user"));

  /** The architecture or the screens: one board, one history. */
  let surface = $state<Surface>("architecture");
  function setSurface(s: Surface) {
    surface = board?.run((e) => e.surface(s)) ?? s;
    tool = "select";
    board?.fit();
  }

  // Plan, Apply and the board updates to Claude.
  let planStatus = $state.raw<BoardStatus | null>(null);
  let items = $state.raw<A11yItem[]>([]);
  let outbox: Outbox | null = null;
  /** Change lines waiting for the quiet period, the turn's end or the feature's session. */
  let queued = $state(0);
  /** The chat's session is this feature's: board edits reach Claude. */
  const live = $derived(!!chat && !!slug && chat.feature === slug);

  /** The feature's outbox, kept by the chat session: edits queued before the board closed (or
   *  before the feature had a session) are still there; the delta comes from this engine now. */
  function attach(s: string) {
    const c = chat, e = engine;
    if (!c || !e) return;
    let o = c.outboxes.get(s);
    if (!o) {
      // a microtask later: the send never runs inside the session's own dispatch
      o = new Outbox(s, (t) => Promise.resolve().then(() => c.post(t)), () => false, () => [], realClock);
      c.outboxes.set(s, o);
    }
    const box = o;
    o.hasPlan = () => !!planStatus?.exists;
    o.delta = (a, b) => e.delta(a, b);
    o.onchange = () => (outbox === box ? (queued = box.pending) : undefined);
    outbox = o;
    if (!o.pending) rebase();
    queued = o.pending;
    c.sync();
  }

  /** A user edit, after the store's follow-ups (a placed element's file); queued whether or
   *  not the chat is in the feature's session (only the send waits for that). */
  function queue() {
    if (outbox && engine && store) outbox.push(store.text());
  }

  /** What Claude knows now: the board as it is, nothing queued. */
  function rebase() {
    if (engine && outbox && store) outbox.baseline(store.text());
  }

  // the chat's "Edited the board" cards: Show selects and fits, Undo reverts Claude's turn
  $effect(() => {
    const c = chat, f = slug;
    if (!c || !f) return;
    c.board = {
      feature: f,
      show: (ids) => (board?.run((e) => e.select(ids)), (surface = engine?.surface() ?? surface), board?.key("2", 1)),
      undo: (step) => {
        const o = board?.run((e) => e.revert(step));
        if (!o?.changed) return false;
        store?.onOut(o);
        queue();
        return true;
      },
    };
    return () => c.board?.feature === f && (c.board = null);
  });

  /** "open <feature>'s session to send". */
  function openSession() {
    const id = slug && chat?.features[slug];
    if (id && !chat!.s.running) void chat!.load(id);
  }

  async function refreshStatus() {
    const s = slug;
    if (!s) return;
    try {
      const st = await api.planStatus(s);
      if (slug === s) planStatus = st;
    } catch (e) {
      console.error(e);
    }
  }

  /** Claude reads the design file: written first, and what it reads is the new baseline (no
   *  separate update for edits made before Plan). */
  async function plan() {
    const s = slug, st = store;
    if (!s || !st || !chat) return;
    await st.flush();
    if (store === st) rebase();
    await chat.ask(s, planPrompt(s));
  }

  async function applyPlan() {
    const s = slug, st = store;
    if (!s || !st || !chat || !applyState(planStatus).enabled) return;
    await st.flush();
    await chat.ask(s, applyPrompt(s));
  }

  // a turn's end: the plan may have changed (the outboxes follow the session by themselves)
  $effect(() => {
    const r = live && !!chat?.s.running;
    if (!r) untrack(() => void refreshStatus());
  });
  // the selected sticky notes go along with the next chat message
  $effect(() => {
    const sel = selection;
    untrack(() => chat && (chat.chips = noteChips(items, sel)));
    return () => chat && (chat.chips = []);
  });
  const readyLine = $derived.by(() => {
    void version;
    return readiness(chat ? openQuestions(chat.s) : 0, openNotes(items), store?.check ?? null);
  });
  const applyable = $derived(applyState(planStatus));

  function onkey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (!(e.metaKey || e.ctrlKey) || e.key !== "Enter" || t?.closest("input, textarea, select, [contenteditable]")) return;
    e.preventDefault();
    void plan();
  }

  function pickTool(t: Tool) {
    board?.run((e) => e.input({ t: "tool", tool: t }));
    tool = t;
  }

  function setScope(s: DesignScope) {
    if (!store || JSON.stringify(s) === JSON.stringify(scope)) return;
    ops([{ op: "scope", scope: s }]);
  }

  // The context follows the board's scope however it changed (picker, undo, redo, a change on
  // disk); a new scope is fitted. A rebuilt graph redraws it and re-checks; the board stays.
  const scopeK = $derived(scopeKey(scope));
  let fittedScope = "";
  $effect(() => {
    void ix;
    const k = scopeK;
    untrack(() => {
      if (!store) return;
      board?.run((e) => e.setContext(context(scope)));
      if (k !== fittedScope) {
        fittedScope = k;
        board?.fit();
      }
    });
  });
  $effect(() => {
    void ix;
    untrack(() => {
      store?.recheck();
      void refreshStatus();
    });
  });

  /** The last "Design around this" applied (a store that loads later applies a newer one). */
  let usedAround = 0;
  $effect(() => {
    const a = around;
    if (!a) return;
    untrack(() => {
      if (!store || a.n === usedAround) return;
      usedAround = a.n;
      setScope({ kind: "around", id: a.id, depth: 1 });
    });
  });

  $effect(() => {
    const s = slug;
    if (!s) return;
    return api.onChanged((changed) => changed === s && store?.slug === s && void store.external());
  });

  // the board closes: the outbox stays with the session (its edits still go), unhooked from here
  $effect(() => () => {
    if (outbox) outbox.onchange = () => {};
  });

  // App quit: the window waits for the last edit to be written
  $effect(() => beforeClose?.(async () => {
    await store?.finish();
  }));

  /** The board is about to free the engine (the view closes): the store takes the text now. */
  function beforeFree() {
    if (!store) return;
    const p = store.close();
    // a window close or quit right after this waits for that write too
    const off = beforeClose?.(() => p);
    void p.finally(() => off?.());
  }

  /** Edits a failed write kept in design.unsaved.json, offered back on open. */
  let kept = $state.raw<{ text: string; at: number } | null>(null);
  async function restoreKept(restore: boolean) {
    const k = kept, s = store;
    if (!k || !s) return;
    kept = null;
    try {
      await (restore ? s.restore(k.text) : s.discard());
    } catch (e) {
      console.error(e);
    }
  }

  // the scope picker: whole system, each layer, the Graph's folder, and the current one
  const layers = $derived(Object.values(ix.root.children).filter((g) => g.layer).map((g) => g.label));
  const scopeOptions = $derived.by(() => {
    const opts: [DesignScope, string][] = [[{ kind: "dir", id: "." }, "Whole system"]];
    layers.forEach((l) => opts.push([{ kind: "layer", name: l }, "Layer " + l]));
    if (graphScope !== ix.root) opts.push([{ kind: "dir", id: graphScope.id }, "Folder " + graphScope.label]);
    if (!opts.some(([s]) => scopeKey(s) === scopeKey(scope))) {
      const label = scope.kind === "around" ? "Around " + (ix.byId[scope.id]?.name ?? scope.id)
        : scope.kind === "layer" ? "Layer " + scope.name : "Folder " + scopeGroup(ix, scope).label;
      opts.push([scope, label]);
    }
    return opts;
  });

  const reasons = $derived.by(() => {
    void version;
    const c = store?.check;
    return c ? Object.fromEntries(targets(c).map(([id, , why]) => [id, why])) : {};
  });

  const status = $derived.by(() => {
    void version;
    const s = store;
    if (!s) return { text: "", failed: false };
    const err = s.errors[s.path];
    if (err) return { text: `Read-only: ${err}. It is never overwritten; fix it and the board reloads.${s.pending ? " Unsaved changes stay on screen until then." : ""}`, failed: true };
    if (s.saveError) return { text: `Not saved: ${s.saveError}. Retrying…`, failed: true };
    if (s.pending) return { text: "saving…", failed: false };
    return { text: `Saved to ${s.path}`, failed: false };
  });
  const readOnly = $derived.by(() => {
    void version;
    return !!store && store.readOnly.has(store.path);
  });
  const checkError = $derived.by(() => {
    void version;
    return store?.checkError ?? null;
  });

</script>

<svelte:window onkeydown={onkey} />

<div class="arch">
  <div class="bar">
    <Features bind:this={features} {api} {slug} onpick={(s) => void open(s)} />
    {#if screensApi}<div class="tabs" role="tablist" aria-label="Surface">{#each [["architecture", "Architecture"], ["screens", "Screens"]] as const as [s, l] (s)}
      <button role="tab" aria-selected={surface === s} disabled={!store} onclick={() => setSurface(s)}>{l}</button>{/each}</div>{/if}
    <label class="scope">Context
      <select value={scopeKey(scope)} disabled={!store || readOnly} onchange={(e) => setScope(JSON.parse(e.currentTarget.value))}>
        {#each scopeOptions as [s, label] (scopeKey(s))}<option value={scopeKey(s)}>{label}</option>{/each}
      </select>
    </label>
    <span class="spacer"></span>
    <button class="icon" title="Undo ({CMD}+Z)" aria-label="Undo" disabled={!store || readOnly} onclick={() => board?.key("z", CMD_BIT)}>{@html Undo2}</button>
    <button class="icon" title="Redo ({CMD}+Shift+Z)" aria-label="Redo" disabled={!store || readOnly} onclick={() => board?.key("z", CMD_BIT | 1)}>{@html Redo2}</button>
    <button class="icon" title="Fit (Shift+1)" aria-label="Fit" disabled={!store} onclick={() => board?.fit()}>{@html Maximize}</button>
  </div>

  {#if surface === "architecture"}<div class="rail" role="toolbar" aria-label="Tools" aria-orientation="vertical">
    {#each TOOLS as [t, key, name, icon] (t)}
      <button class="icon" title="{name} ({key})" aria-label="{name} ({key})" aria-pressed={tool === t ? "true" : "false"}
        disabled={!store || readOnly} onclick={() => pickTool(t)}>{@html icon}<kbd>{key}</kbd></button>
    {/each}
  </div>{/if}

  <div class="canvas" class:docked inert={readOnly} bind:this={canvasEl}>
    <Board bind:this={board} source={() => ({ board: BLANK, context: '{"lanes":[],"boxes":[],"edges":[]}' })} {importer} onready={ready} {onout} {reasons}
      onbeforefree={beforeFree} hand={surface === "screens"} onhover={(h) => (hover = h)} />
    {#if kept}
      <div class="kept" role="alert">
        Unsaved changes from {new Date(kept.at).toLocaleString()} were kept (design.unsaved.json).
        <button onclick={() => restoreKept(true)}>Restore</button>
        <button onclick={() => restoreKept(false)}>Discard</button>
      </div>
    {/if}
    {#if !slug}
      <div class="empty">
        <p>No feature yet. <strong>New feature</strong> above starts one: <code>plans/&lt;slug&gt;/design.json</code>, drawn over the real system.</p>
      </div>
    {/if}
  </div>

  <ScreensBoard bind:this={screens} active={surface === "screens"} {slug} {store} api={screensApi} engine={() => engine} {act} {tool} {selection} {version}
    onpick={pickTool} {ix} context={contextJson} {hover} {openCode} canvas={() => canvasEl} ondock={(d) => (docked = d)}
    {initialLink} />
  {#if surface === "architecture"}<Inspector {ix} {doc} {selection} {boxes} {reasons} {ops} cmd={CMD} />{/if}

  <div class="foot">
    {#if chat}
      <div class="actions" aria-label="Plan and apply">
        <span class="ready" class:clear={readyLine.clear} role="status">{readyLine.text}</span>
        {#if queued && live}<span class="muted">Chat · {queued} {queued === 1 ? "note" : "notes"} queued</span>
        {:else if queued && slug && chat.features[slug]}<button class="link" disabled={chat.s.running} onclick={openSession}>Chat · {queued}
          {queued === 1 ? "edit" : "edits"} queued — open {slug}'s session to send</button>
        {:else if queued}<span class="muted" title="Plan starts the feature's session with the design as it is">Chat · {queued} {queued === 1 ? "edit" : "edits"} queued</span>{/if}
        <span class="spacer"></span>
        {#if applyable.reason}<span class="muted">{applyable.reason}</span>{/if}
        <button disabled={!store || !applyable.enabled} title={applyable.reason ?? "Build the plan: /ig:apply"} onclick={applyPlan}>Apply</button>
        <button class="plan" disabled={!store} title="Plan from this design ({CMD}+Enter)" onclick={plan}>Plan <kbd>{isMac ? "⌘↵" : "Ctrl+↵"}</kbd></button>
      </div>
    {/if}
    <Checks check={store?.check ?? null} error={checkError} status={status.text} failed={status.failed} />
  </div>
</div>

<style>
  .arch { position: absolute; inset: 0; z-index: 2; display: grid; grid-template-columns: 48px 1fr 260px;
    grid-template-rows: auto 1fr auto auto; background: var(--bg); color: var(--fg); }
  .bar { grid-column: 1 / -1; display: flex; gap: 8px; align-items: center; padding: 6px 10px; border-bottom: 1px solid var(--line);
    background: var(--panel); font-size: 13px; }
  .spacer { flex: 1; }
  .scope { display: flex; gap: 6px; align-items: center; color: var(--muted); }
  .rail { display: flex; flex-direction: column; gap: 4px; padding: 6px; border-right: 1px solid var(--line); background: var(--panel); }
  .icon { position: relative; display: grid; place-items: center; width: 34px; height: 34px; padding: 0; border-radius: 6px; }
  .icon :global(svg) { width: 18px; height: 18px; }
  .icon[aria-pressed="true"] { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
  kbd { position: absolute; right: 2px; bottom: 0; font-size: 9px; color: inherit; opacity: 0.7; font-family: inherit; }
  .canvas { grid-row: 2; grid-column: 2; position: relative; min-width: 0; min-height: 0; }
  .canvas.docked { margin-right: 50%; }
  .tabs { display: flex; gap: 2px; } .tabs [aria-selected="true"] { background: var(--accent); color: var(--on-accent); }
  .kept { position: absolute; top: 8px; left: 50%; transform: translateX(-50%); z-index: 3; display: flex; gap: 8px;
    align-items: center; padding: 6px 10px; font-size: 13px; background: var(--panel); border: 1px solid var(--warning);
    border-radius: 6px; color: var(--fg); }
  .empty { position: absolute; inset: 0; background: var(--bg); padding: 15vh 16px 0; color: var(--muted); }
  .empty p { max-width: 34em; margin: 0 auto; }
  .muted { color: var(--muted); }
  .foot { grid-row: 4; grid-column: 1 / -1; }
  .actions { display: flex; gap: 10px; align-items: center; padding: 6px 10px; border-top: 1px solid var(--line);
    background: var(--panel); font-size: 13px; }
  .ready { color: var(--warning); }
  .ready::before { content: "● "; }
  .ready.clear { color: var(--added); }
  .actions .link { border: 0; background: none; padding: 0; color: var(--accent-text); font: inherit; cursor: pointer; }
  .plan { background: var(--accent); border-color: var(--accent); color: var(--on-accent); font-weight: 600; }
  .plan kbd { position: static; font-size: 11px; margin-left: 4px; }
</style>
