<!-- The app: header (status, plan, modes, search, counts, breadcrumb, hint), and the state the tree,
     graph and side panel share: mode, Architecture scope, focus, journey. -->
<script module lang="ts">
  import type { Mode } from "../model/show";

  const STATE_LABEL = { planned: "Planning", applying: "Applying", done: "Done", plain: "Plan" };

  const MODES: [Mode, string, string][] = [
    ["changed", "Changed", "Changed nodes and their direct neighbours"],
    ["architecture", "Architecture", "Layers, then directories, modules and members. Click a box to open it."],
    ["focus", "Focus", "The neighbourhood of the selected node"],
    ["blueprint", "Blueprint", "A page and everything behind it: front, API, use cases, adapters, systems"],
  ];

  const LINKS_KEY = "ig.links";
  const TESTS_KEY = "ig.tests";

  function readTests(): boolean {
    try {
      return localStorage.getItem(TESTS_KEY) === "1";
    } catch {
      return false;
    }
  }

  function readLinks(): boolean {
    try {
      return localStorage.getItem(LINKS_KEY) !== "0";
    } catch {
      return true;
    }
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { NoteInput, Note, ViewData } from "../data";
  import type { IgApi } from "../api";
  import { coalesce } from "../model/coalesce";
  import { dropNote, ghosts, pins } from "../model/notes";
  import { architecture, initialScope, rep } from "../model/architecture";
  import { blueprint, journeyName, unitOf } from "../model/blueprint";
  import { changedIds, exact } from "../model/changed";
  import { focusIds } from "../model/focus";
  import { withTests } from "../model/tests";
  import { ancestors, findNode, index, topMember, type Group } from "../model/graph";
  import { beforeAfter, nodeNamed } from "../model/code";
  import { applyShow, commonScope, parseShow, showReducer, type Show, type ShowState } from "../model/show";
  import ShowBanner from "./ShowBanner.svelte";
  import Chat from "./chat/Chat.svelte";
  import { ChatSession } from "./chat/session.svelte";
  import { nodeMatcher } from "../model/markdown";
  import CodeView from "./CodeView.svelte";
  import Files from "./Files.svelte";
  import Graph from "./Graph.svelte";
  import SidePanel, { type SideView } from "./SidePanel.svelte";
  import Tree from "./Tree.svelte";
  import { loadFailed } from "../board/messages";

  /** The desktop app's Design view: the architecture board, a lazy chunk; `ig view` builds
   *  (`__BOARDS__` false) drop it with the engine. */
  const DesignView = __BOARDS__ ? () => import("../board/ArchBoard.svelte") : null;
  /** The IG_DESIGN dev hook (`?design=<slug>`): the Design view open on that feature. */
  const initialDesign = __BOARDS__ ? new URLSearchParams(location.search).get("design") : null;
  let designOpen = $state(!!initialDesign);
  /** Bumped by Retry: the chunk import runs again. */
  let designTry = $state(0);
  /** "Design around this": the node, numbered so the same one asks again. */
  let around = $state.raw<{ id: string; n: number } | null>(null);

  /** The first data (the host reads it before mounting) and the host's API. */
  let { data: first, api }: { data: ViewData; api: IgApi } = $props();

  // Rebuilds swap the data in place; everything below follows `D`.
  let data = $state.raw(untrack(() => first));
  let tests = $state(readTests());
  const D = $derived(data);
  /** What the graph and tree see: without test files unless the Tests toggle is on. */
  const ix = $derived(index(withTests(D, tests)));
  /** Extra start nodes from `D.focus`; cleared when the user focuses on one node. */
  let focusRoots: string[] = untrack(() => (D.focus ?? []).filter((id) => ix.byId[id]));
  let mode = $state<Mode>(untrack(() => (focusRoots.length ? "focus" : D.mode)));
  let scope = $state.raw<Group>(untrack(() => initialScope(ix)));
  let focusId = $state<string | null>(focusRoots[0] ?? null);
  let focusDepth = $state(1);
  let journey = $state<string | null>(null);
  let uiOnly = $state(false);
  let side = $state.raw<SideView>({ kind: "overview" });
  let hint = $state("");
  let screenIds = $state.raw<string[]>([]);
  let links = $state(readLinks());
  let notes = $state.raw<Note[]>([]);
  /** The open right-click menu: a node, or an edge between two nodes. */
  let menu = $state.raw<{ x: number; y: number; target: string; to?: string } | null>(null);
  let graph: ReturnType<typeof Graph>;
  let tree: ReturnType<typeof Tree>;
  let lastMode: Mode | null = null;
  /** Claude's `ig_show`: the answer (re-applied to every new index) and the view Back returns to. */
  let showSt = $state.raw<ShowState>({ current: null, previous: null });
  const shown = $derived(showSt.current ? applyShow(ix, showSt.current) : null);
  let showSeq = 0;

  /** Back to the data's own start state: first paint, and after a rebuild swaps the data. */
  function reset() {
    focusRoots = (D.focus ?? []).filter((id) => ix.byId[id]);
    mode = focusRoots.length ? "focus" : D.mode;
    scope = initialScope(ix);
    focusId = focusRoots[0] ?? null;
    focusDepth = 1;
    journey = null;
    uiOnly = false;
    lastMode = null;
    menu = null;
    showView();
    draw();
    side = { kind: "overview" };
    refreshNotes();
  }

  /** Tauri's invoke rejects with a plain string. */
  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  onMount(() => {
    reset();
  });

  // A swapped `data` starts over (the first run is onMount's).
  let seen = untrack(() => data);
  $effect(() => {
    if (data === seen) return;
    seen = data;
    untrack(reset);
  });

  // The host's events; the cleanup closes its stream.
  $effect(() => {
    const offs = [
      // One view_data build at a time, plus one pending for the events that came meanwhile.
      api.onRebuild(coalesce(() => api.viewData().then((d) => (data = d), (e: unknown) => (hint = msg(e))))),
      api.on?.("note", () => refreshNotes()),
      api.on?.("focus", (ids) => focusAll(ids as string[])),
      api.on?.("show", (raw) => {
        const s = parseShow(raw);
        if (s) showAnswer(s);
      }),
    ];
    return () => offs.forEach((off) => off?.());
  });

  export function refreshNotes() {
    api.notes?.notes().then((ns) => {
      notes = ns;
      graph.decorate(pins(ns), ghosts(ns));
    }, (e: unknown) => (hint = msg(e)));
  }

  /** From `ig_focus`: Focus mode on these nodes. */
  export function focusAll(ids: string[]) {
    const known = ids.filter((id) => ix.byId[id]);
    if (!known.length) return;
    focusRoots = known;
    focusId = known[0];
    mode = "focus";
    draw();
  }

  /** From `ig_show` or a chat chip: the answer in gold, in its view; Back returns to the view before the first show. */
  export function showAnswer(s: Show) {
    const view = { mode, selection: side.kind === "node" ? [side.id] : [], scope: scope.id, focus: focusRoots.length ? focusRoots : focusId ? [focusId] : [] };
    showSt = showReducer(showSt, { t: "show", s: { ...s, seq: ++showSeq }, view }).state;
    showView();
    draw();
    const first = showSt.current?.mode === "chain" && [...shown!.nodes][0];
    if (first) details(first); // the side panel shows its calls down
  }

  /** The show's view: Focus on its nodes, or Architecture at their common scope. Draws nothing. */
  function showView() {
    const ids = shown ? [...shown.nodes] : [];
    if (!ids.length) return;
    if (shown!.mode === "architecture") scope = commonScope(ix, ids);
    else [focusRoots, focusId] = [ids, ids[0]];
    mode = shown!.mode;
  }

  /** The drawn element that stands for node `id` in this mode. */
  function onGraph(id: string): string | null {
    const n = ix.byId[id];
    if (!n) return null;
    return mode === "architecture" ? rep(ix, scope, id) : mode === "blueprint" ? unitOf(ix, id) : graph.pick([id, topMember(ix, n).id, n.file]);
  }

  function highlightShow() {
    if (!shown) return graph.highlight(null);
    const key = (t: string) => t.split("|").map(onGraph).join("|");
    const steps: Record<string, number[]> = {};
    shown.steps.forEach((st) => (steps[key(st.target)] ??= []).push(st.n));
    const nodes = new Set([...shown.nodes].map(onGraph).filter((x): x is string => !!x));
    graph.highlight({ nodes, edges: new Set([...shown.edges].map(key)), steps });
  }

  function clearShow() {
    showSt = showReducer(showSt, { t: "clear" }).state;
    highlightShow();
  }

  function backShow() {
    const r = showReducer(showSt, { t: "back" });
    showSt = r.state;
    const p = r.restore;
    if (!p) return highlightShow();
    scope = (p.scope && ix.groups[p.scope]) || (p.scope === ix.root.id ? ix.root : initialScope(ix));
    [focusRoots, focusId] = [p.focus, p.focus[0] ?? null];
    mode = p.mode === "focus" && !focusId ? D.mode : p.mode;
    draw();
    const sel = p.selection.find((id) => ix.byId[id]);
    if (sel) onTreeSelect(sel);
    else side = { kind: "overview" };
  }

  /** A step: the first click selects its node (an edge's source), the second opens its code. */
  function onStep(target: string, again: boolean) {
    const id = target.split("|")[0];
    if (again && sideActions.openFile) return sideActions.openFile(id);
    onTreeSelect(id);
  }

  /** Gestures make notes only on a checked plan that is not being applied, in `ig serve`. */
  const planning = () => !!api.notes && D.state === "planned";

  function addNote(n: NoteInput) {
    api.notes?.add(n).then(() => {
      hint = "Note added; the agent will pick it up.";
      refreshNotes();
    }, (e: unknown) => (hint = msg(e)));
  }

  function onContext(d: { id: string; edge: boolean; source: string; target: string; x: number; y: number }) {
    const by = ix.byId;
    if (!planning()) return;
    if (d.edge ? by[d.source] && by[d.target] : by[d.id])
      menu = d.edge ? { x: d.x, y: d.y, target: d.source, to: d.target } : { x: d.x, y: d.y, target: d.id };
  }

  function menuAction(kind: "comment" | "remove") {
    const m = menu!;
    menu = null;
    const what = m.to ? m.target + " → " + m.to : m.target;
    if (kind === "remove") return addNote({ kind, target: m.target, to: m.to });
    const text = window.prompt("Comment on " + what);
    if (text && text.trim()) addNote({ kind, target: m.target, to: m.to, text });
  }

  function onDrop(d: { id: string; y: number }) {
    const n = ix.byId[d.id];
    if (!planning() || mode !== "architecture" || !n) return;
    const note = dropNote(graph.lanes(D.layers), n.id, d.y, n.layer);
    if (note) addNote(note);
  }

  function onLink({ from, to }: { from: string; to: string }) {
    if (!planning() || !ix.byId[from] || !ix.byId[to]) return;
    const kind = window.prompt("Link " + from + " → " + to + ": kind", "calls");
    if (kind && kind.trim()) addNote({ kind: "link", target: from, to, text: kind.trim() });
  }

  function addBox(layer: string) {
    const v = window.prompt("Add to layer " + layer + ": <kind> <name>", "module ");
    const m = v && /^\s*(\S+)\s+(\S+)\s*$/.exec(v);
    if (m) addNote({ kind: "add", text: m[1], target: m[2], layer });
    else if (v) hint = "Expected <kind> <name>, like: module cache";
  }

  /** Re-renders the graph for the current mode. `zoomTo` fits that box. */
  function draw(zoomTo?: string) {
    const els =
      mode === "architecture" ? architecture(ix, scope, graph.width() || 1200)
      : mode === "focus" ? exact(ix, focusIds(ix, focusRoots.length ? focusRoots : focusId!, focusDepth))
      : mode === "blueprint" ? blueprint(ix, journey, uiOnly)
      : exact(ix, changedIds(ix));
    if (mode === "architecture" && planning())
      els.filter((e) => e.data.kind === "lanelabel" && D.layers.includes(e.data.label!)).forEach((l) =>
        els.push({ group: "nodes", data: { id: "laneadd:" + l.data.label, label: "+", kind: "laneadd" },
          position: { x: l.position!.x, y: l.position!.y + 28 }, grabbable: false }));
    const fresh = mode !== lastMode;
    lastMode = mode;
    graph.show(els, { fresh, preset: mode === "blueprint" || mode === "architecture", zoomTo });
    graph.decorate(pins(notes), ghosts(notes));
    highlightShow();
    if (mode === "focus" && focusId) graph.select(focusId);
    if (mode === "blueprint") side = { kind: "journeys" };
    screenIds = graph.ids();
    const focused = focusId && ix.byId[focusId];
    hint =
      mode === "blueprint"
        ? journey
          ? "Journey " + journeyName(journey).route + ": the page and everything behind it, by lane. Dashed blue: HTTP calls."
          : "Pick a page on the right."
        : mode === "architecture" ? "Click a box to open it. Dimmed boxes are outside, connected context."
        : mode === "focus" ? "Neighbourhood of " + (focused ? focused.name : "") + " (depth " + focusDepth + ")"
        : screenIds.length ? "" : "No changes against the base.";
  }

  function setMode(m: Mode) {
    if (m === "focus" && !focusId) return;
    mode = m;
    draw();
  }

  function toggleLinks() {
    links = !links;
    try {
      localStorage.setItem(LINKS_KEY, links ? "1" : "0");
    } catch {
      // storage blocked: the choice lasts for this page only
    }
  }

  function toggleTests(on = !tests) {
    tests = on;
    try {
      localStorage.setItem(TESTS_KEY, tests ? "1" : "0");
    } catch {
      // storage blocked: the choice lasts for this page only
    }
    scope = ix.groups[scope.id] ?? ix.root; // ix is new: re-find the scope in it
    if (side.kind === "node" && !ix.byId[side.id]) side = { kind: "overview" };
    draw();
  }

  function setScope(t: Group | undefined) {
    scope = t || ix.root;
    mode = "architecture";
    draw();
  }

  function focusOn(id: string) {
    if (!ix.byId[id] && D.nodes.some((n) => n.id === id && n.test)) toggleTests(true); // a finding on a test
    if (!ix.byId[id]) return;
    focusId = id;
    focusRoots = [];
    mode = "focus";
    draw();
    details(id);
  }

  function details(id: string, fromTree = false) {
    if (!ix.byId[id]) {
      side = ix.groups[id] ? { kind: "group", id } : { kind: "overview" };
      return;
    }
    if (!fromTree) tree.reveal(id);
    side = { kind: "node", id };
  }

  function onTap({ id, kind }: { id: string; kind: string }) {
    if (kind === "laneadd") return addBox(id.slice("laneadd:".length));
    if (mode === "architecture" && (kind === "layer" || kind === "dir")) {
      setScope(ix.groups[id]);
      side = { kind: "group", id };
      return;
    }
    details(id);
    graph.fadeAround(id);
  }

  /** Tree → graph: selects the element that stands for `id` in this mode, or says it is not shown. */
  function onTreeSelect(id: string) {
    const n = ix.byId[id];
    details(id, true);
    if (!n) return;
    const candidates =
      mode === "architecture" ? [rep(ix, scope, id)]
      : mode === "blueprint" ? [unitOf(ix, id)]
      : [id, topMember(ix, n).id, n.file];
    const x = graph.pick(candidates);
    graph.clear();
    if (x) {
      graph.select(x);
      graph.fadeAround(x);
      graph.center(x);
    } else {
      hint = "Not in this view; double-click it in the tree (or press Focus) to see its neighbourhood.";
    }
  }

  function onTreeFolder(key: string) {
    const prefix = key + "/";
    graph.fadeExcept((id) => {
      const b = ix.byId[id];
      return !!b && !!b.file && b.file.indexOf(prefix) === 0;
    });
  }

  function onSearch(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    const hit = findNode(ix, (e.target as HTMLInputElement).value);
    if (hit) focusOn(hit);
  }

  /** The file of the selected node, for the Files panel. */
  const selectedFile = $derived(side.kind === "node" ? (ix.byId[side.id]?.file ?? null) : null);

  /** The code view over the graph: a file's text, or a node's before and after. */
  let code = $state.raw<{ path: string; text?: string; line?: number; merge?: { before: string; after: string } } | null>(null);

  let opening = 0;

  async function openFile(path: string, line?: number) {
    const mine = ++opening;
    try {
      const f = await api.readFile!(path);
      if (mine === opening) code = { path, text: f.text, line };
    } catch (e) {
      if (mine !== opening) return;
      hint = String((e as Error)?.message ?? e);
    }
  }

  /** A file chosen in Files: shown in the code view, and selected in the graph when it has a module. */
  function onFile(path: string) {
    const m = D.nodes.find((n) => n.kind === "module" && n.file === path);
    if (m) onTreeSelect(m.id);
    else if (!api.readFile) hint = path + " is not in the graph.";
    if (api.readFile) openFile(path);
  }

  /** A word clicked in the code view: the graph node of this file with that name, if any. */
  function onWord(word: string, line: number) {
    const n = code && nodeNamed(D.nodes, code.path, word, line);
    if (n) {
      onTreeSelect(n.id);
      code = null;
    }
  }

  /** The chat (desktop app only), docked on the right; "Chat" in the header hides it. */
  let chatOpen = $state(true);
  const nodeMatch = $derived(nodeMatcher(Object.values(ix.byId), D.root));
  const chatApi = untrack(() => (api.agent && api.trustInventory && api.trustProject ? api : null));
  /** One session for the chat panel and the board. */
  const chat = chatApi ? new ChatSession(chatApi.agent!, chatApi.trustInventory!.bind(chatApi), chatApi.trustProject!.bind(chatApi)) : null;
  // a held prompt (Plan, Apply behind the trust card or setup) opens a collapsed chat
  $effect(() => {
    if (chat?.held) chatOpen = true;
  });

  /** What "Attach selection" and "Plan this" take: the selected node, else the file in the code view. */
  const selection = $derived(
    side.kind === "node" && ix.byId[side.id] ? { ids: [side.id], files: ix.byId[side.id].file ? [ix.byId[side.id].file] : [] }
    : code && !code.merge ? { ids: [], files: [code.path] }
    : null);

  const crumbs = $derived(mode === "architecture" ? [ix.root, ...ancestors(ix, scope)] : []);

  const sideActions = {
    focus: focusOn,
    overview: () => {
      graph.clear();
      side = { kind: "overview" };
    },
    depth: (id: string) => {
      focusDepth = 3 - focusDepth;
      draw();
      details(id);
    },
    arch: (id: string) => {
      const n = ix.byId[id];
      setScope(ix.treeOf[n.file]);
      graph.select(n.file);
    },
    journey: (id: string) => {
      journey = id;
      draw();
    },
    uiOnly: (on: boolean) => {
      uiOnly = on;
      if (journey) draw();
    },
    compare: (id: string) => {
      const n = ix.byId[id];
      if (n?.diff) code = { path: n.file, merge: beforeAfter(n.diff) };
    },
    design: __BOARDS__ && untrack(() => api.design) ? (id: string) => {
      around = { id, n: (around?.n ?? 0) + 1 };
      designOpen = true;
    } : undefined,
    openFile: untrack(() => api.readFile) ? (id: string) => openFile(ix.byId[id].file, ix.byId[id].line) : undefined,
  };
</script>

<!-- Any click or Escape closes the right-click menu (its buttons act first). -->
<svelte:window onclick={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} />

<header>
  <h1><svg class="mark" viewBox="0 0 120 120" width="22" height="22" fill="none" stroke="currentColor" stroke-width="5" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"><path d="M30 42 Q30 22 60 22 Q90 22 90 42 V86 Q90 104 60 104 Q30 104 30 86 Z"/><path d="M35 28 L30 13 L46 23"/><path d="M85 28 L90 13 L74 23"/><path d="M56 62 L60 68 L64 62"/><circle cx="47" cy="48" r="11"/><circle cx="73" cy="48" r="11"/><circle cx="47" cy="48" r="4.5" fill="#E8A317" stroke="none"/><circle cx="73" cy="48" r="4.5" fill="#E8A317" stroke="none"/></svg>ig</h1>
  <span id="status" class="status {D.blocking ? 'block' : 'ok'}">{D.blocking ? "blocking" : "not blocking"}</span>
  <span id="plan" class="status">{D.plan ? STATE_LABEL[D.state] + " · " + D.plan.title : ""}</span>
  <div class="modes" role="group" aria-label="View mode">
    {#each MODES as [m, label, title]}<button data-mode={m} {title} aria-pressed={m === mode ? "true" : "false"}
      disabled={m === "focus" && !focusId} onclick={() => setMode(m)}>{label}</button>{/each}
  </div>
  <button id="links" class="toggle" aria-pressed={links ? "true" : "false"} title="Show or hide the links (the selected node's stay)"
    onclick={toggleLinks}>Links</button>
  <button id="tests" class="toggle" aria-pressed={tests ? "true" : "false"} title="Show or hide test and spec files"
    onclick={() => toggleTests()}>Tests</button>
  <input id="search" type="search" placeholder="Find a node…" aria-label="Find a node" list="ids" autocomplete="off" onkeydown={onSearch}>
  <datalist id="ids">{#each D.nodes.filter((n) => n.kind !== "test" && (tests || !n.test)) as n}<option value={n.id}></option>{/each}</datalist>
  <span id="counts" class="counts">{D.counts.added} added · {D.counts.changed} changed · {D.counts.removed} removed</span>
  <!-- svelte-ignore a11y_invalid_attribute -->
  <span id="crumbs" class="crumbs">{#each crumbs as t, i}{i ? " › " : ""}{#if t === scope}<strong>{i ? t.label : "All"}</strong>{:else}<a href="#" onclick={(e) => { e.preventDefault(); setScope(t); side = { kind: "group", id: t.id }; }}>{i ? t.label : "All"}</a>{/if}{/each}</span>
  <span id="hint" class="counts" title={hint}>{hint}</span>
  {#if __BOARDS__ && api.design}<button id="design-toggle" class="toggle" aria-pressed={designOpen ? "true" : "false"} title="Draw a feature over the real system"
    onclick={() => (designOpen = !designOpen)}>Design</button>{/if}
  {#if chatApi}<button id="chat-toggle" class="toggle" aria-pressed={chatOpen ? "true" : "false"} onclick={() => (chatOpen = !chatOpen)}>Chat</button>{/if}
</header>
<!-- The Design view: the board with the chat docked on its right ("Chat" collapses it). -->
<main class:with-chat={chatApi && chatOpen} class:designing={__BOARDS__ && api.design && designOpen}>
  <div id="tree" aria-label="Files" role="navigation">
    <Tree bind:this={tree} {ix} {mode} {screenIds} onselect={onTreeSelect} onfolder={onTreeFolder} onfocus={focusOn} />
    {#if api.fileTree}<Files list={api.fileTree.bind(api)} path={selectedFile} version={D} onselect={onFile} />{/if}
  </div>
  <div id="center">
    <Graph bind:this={graph} {links} ontap={onTap} oncontext={onContext} ondrop={onDrop} onlink={onLink} />
    {#if showSt.current && shown}<ShowBanner title={showSt.current.title} missing={shown.missing.length} steps={shown.steps}
      onclear={clearShow} onback={backShow} onstep={onStep} />{/if}
    {#if __BOARDS__ && api.design && designOpen}{#key designTry}{#await DesignView!() then V}<V.default {ix} api={api.design} graphScope={scope} {around} initial={initialDesign}
      beforeClose={api.beforeClose?.bind(api)} {chat} screensApi={api.screens ?? null}
      initialSurface={initialDesign && new URLSearchParams(location.search).get("surface")} />{:catch e}<div role="alert" style="position:absolute;inset:0;z-index:2;padding:15vh 16px 0;background:var(--bg)"><p>{loadFailed(e instanceof Error ? e.message : String(e))}</p><button onclick={() => designTry++}>Retry</button></div>{/await}{/key}{/if}
    {#if code}<CodeView path={code.path} text={code.text} merge={code.merge} line={code.line} onword={onWord} onclose={() => (code = null)} />{/if}
  </div>
  <div id="side" role="complementary">
    <SidePanel {ix} view={side} {mode} {focusId} {focusDepth} {journey} {uiOnly} {notes} on={sideActions} />
  </div>
  {#if chatApi}
    <div id="chat" hidden={!chatOpen}>
      <Chat chat={chat!}
        openUrl={chatApi.openUrl ? (u) => chatApi.openUrl!(u).catch((e: unknown) => (hint = msg(e))) : undefined}
        root={D.root} {selection} onopendiff={(path, before, after) => (code = { path, merge: { before, after } })}
        match={nodeMatch} onnode={focusOn} onshow={showAnswer} />
    </div>
  {/if}
</main>
{#if menu}
  <div class="ctx-menu" role="menu" style="left:{menu.x}px;top:{menu.y}px">
    <button role="menuitem" onclick={() => menuAction("comment")}>Comment</button>
    <button role="menuitem" onclick={() => menuAction("remove")}>Remove</button>
  </div>
{/if}
