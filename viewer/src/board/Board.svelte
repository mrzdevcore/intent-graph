<!-- The board host: the engine's canvas, the label layer, the rename overlay and the
     accessibility list. It loads the engine lazily and says plainly when it can't: a failed
     load (with Retry), no WebGL2, or a panic (Retry reloads the page: a trapped wasm instance
     can't be re-initialised, and the board comes back from disk).
     The frame loop runs only while something changed. -->
<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { registerBoard } from "../model/boardselect";
  import type { A11yItem, BoardEngine, Camera, EngineInput, EngineOut, Label } from "./engine";
  import { loadEngine, loadFailed, palette, release, type Importer } from "./host";
  import Labels from "./Labels.svelte";
  import EditOverlay from "./EditOverlay.svelte";
  import A11yList from "./A11yList.svelte";

  let { source, importer, onout, onready, onbeforefree, message = "", reasons = {}, hand = false, reload = () => location.reload() }: {
    /** The board text and context JSON to load: on start and Retry. */
    source: () => { board: string; context: string };
    importer?: Importer;
    onout?: (o: EngineOut, engine: BoardEngine) => void;
    onready?: (engine: BoardEngine) => void;
    /** Unmount, just before the engine is freed: the last chance to read the board. */
    onbeforefree?: (engine: BoardEngine) => void;
    /** Announced politely to screen readers (Claude's edits, check summaries). */
    message?: string;
    /** Retry after a panic (tests replace it). */
    reload?: () => void;
    /** Check reasons by element, link key or context box: the canvas title on hover. */
    reasons?: Record<string, string>;
    /** The screens: handwritten labels. */
    hand?: boolean;
  } = $props();

  let host: HTMLDivElement;
  let canvas = $state<HTMLCanvasElement>();
  let engine: BoardEngine | null = null;
  let failure = $state<{ text: string; retry: boolean } | null>(null);
  let camera = $state<Camera>({ x: 0, y: 0, zoom: 1 });
  let labels = $state.raw<Label[]>([]);
  let items = $state.raw<A11yItem[]>([]);
  let edit = $state<{ id: string; rect: [number, number, number, number]; text: string } | null>(null);
  let cursor = $state("default");
  let status = $state("");
  let width = $state(0);
  let height = $state(0);
  let pending = 0;
  /** Fit the view once the next frame has measured it (start, and a newly loaded board). */
  let fitNext = true;
  let hover = $state<string | null>(null);

  const MODS = (e: KeyboardEvent | MouseEvent) =>
    (e.shiftKey ? 1 : 0) | (e.altKey ? 2 : 0) | (e.ctrlKey ? 4 : 0) | (e.metaKey ? 8 : 0);

  /** Runs an engine call; a panic is already reported through `fail`. */
  function act<T>(f: (e: BoardEngine) => T): T | undefined {
    if (!engine) return undefined;
    try {
      return f(engine);
    } catch (e) {
      if (!failure) status = e instanceof Error ? e.message : String(e);
      return undefined;
    }
  }

  let panicked = false;
  /** Where focus goes back when the rename overlay closes (the canvas or the a11y list). */
  let opener: HTMLElement | null = null;

  function fail(reason: string) {
    panicked = true;
    engine = null;
    edit = null;
    failure = { text: loadFailed(reason), retry: true };
  }

  async function start() {
    failure = null;
    status = "";
    await tick();
    if (!canvas) return;
    const r = await loadEngine(canvas, importer, fail);
    if ("error" in r) {
      failure = { text: r.error, retry: r.retry };
      return;
    }
    engine = r.engine;
    // a lost context comes back by itself, rebuilt from the in-memory scene
    canvas.addEventListener("webglcontextlost", (e) => e.preventDefault());
    canvas.addEventListener("webglcontextrestored", () => (act((e) => e.restore()), schedule()));
    const s = source();
    act((e) => {
      e.load(s.board, s.context);
      e.palette(palette(host));
    });
    resize();
    items = act((e) => e.a11y()) ?? [];
    if (engine) onready?.(engine);
  }

  function schedule() {
    if (pending || !engine) return;
    pending = requestAnimationFrame(() => {
      pending = 0;
      if (act((e) => e.frame())) {
        if (fitNext) {
          fitNext = false;
          act((e) => e.input({ t: "key", key: "!", mods: 1 }));
          return schedule();
        }
        camera = act((e) => e.camera()) ?? camera;
        labels = act((e) => e.labels()) ?? labels;
      }
    });
  }

  // ponytail: the pixel ratio is read on resize; a move to a display of another ratio waits for
  // the next resize (add a matchMedia resolution listener if that shows up blurry)
  function resize() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    act((e) => e.resize(width, height, dpr));
    schedule();
  }

  /** Every engine answer: cursor, rename, errors, a11y, redraw; then the owner's. */
  function handle(o: EngineOut | undefined) {
    if (!o || !engine) return;
    cursor = o.cursor;
    if (o.edit) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      edit = o.edit;
    }
    status = o.error ?? "";
    if (o.changed || o.selection.join("\n") !== items.filter((i) => i.selected).map((i) => i.id).join("\n"))
      items = act((e) => e.a11y()) ?? items;
    if (o.redraw || o.changed) schedule();
    onout?.(o, engine);
  }

  const send = (ev: EngineInput) => handle(act((e) => e.input(ev)));

  /** For the owner (tool rail, inspector, store): an engine call whose effects show on the board.
   *  The answer goes back to the caller, not to `onout`. */
  export function run<T>(f: (e: BoardEngine) => T): T | undefined {
    const r = act(f);
    const o = r as EngineOut | undefined;
    if (o && typeof o === "object" && "redraw" in o) {
      cursor = o.cursor;
      status = o.error ?? "";
    }
    items = act((e) => e.a11y()) ?? items;
    schedule();
    return r;
  }

  /** The next frame fits the view to the board (a newly loaded one). */
  export function fit() {
    fitNext = true;
    schedule();
  }

  /** Board shortcuts from the owner's buttons (undo, redo, fit). */
  export function key(key: string, mods = 0) {
    send({ t: "key", key, mods });
  }

  function onmove(e: PointerEvent) {
    if (edit) return;
    const p = point(e);
    send({ t: "move", ...p });
    hover = act((en) => en.hit(p.x, p.y)) ?? null;
  }

  function point(e: PointerEvent) {
    const r = canvas!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top, mods: MODS(e), pressure: e.pressure || 0.5 };
  }

  function onpointerdown(e: PointerEvent) {
    if (edit) {
      // a click outside the rename overlay commits it (blur), and does nothing else
      (document.activeElement as HTMLElement | null)?.blur();
      return;
    }
    canvas!.focus();
    canvas!.setPointerCapture?.(e.pointerId);
    const p = point(e);
    send({ t: "down", ...p, button: e.button, mods: p.mods | (e.detail >= 2 ? 16 : 0) });
  }

  /** Keys the board owns; the rest (Cmd+Q, Cmd+Enter, Tab…) keep their default. */
  function owned(e: KeyboardEvent): boolean {
    const cmd = e.metaKey || e.ctrlKey;
    if (cmd) return /^[zcvda0]$/i.test(e.key);
    return /^(Arrow|Backspace$|Delete$|Enter$|Escape$|[a-z!]$)/i.test(e.key) || (e.shiftKey && /^[12!@]$/.test(e.key));
  }

  function onkeydown(e: KeyboardEvent) {
    // the rename overlay and the a11y list keep their keys
    if (edit || e.target !== canvas || !owned(e)) return;
    e.preventDefault();
    send({ t: "key", key: e.key, mods: MODS(e) });
  }

  function commit(name: string) {
    const ed = edit!;
    edit = null;
    (opener?.isConnected ? opener : canvas)?.focus();
    if (name === ed.text) return;
    handle(act((e) => e.rename(ed.id, name)));
  }

  function cancel() {
    edit = null;
    (opener?.isConnected ? opener : canvas)?.focus();
  }

  function select(id: string) {
    act((e) => e.select([id]));
    items = act((e) => e.a11y()) ?? items;
    schedule();
  }

  $effect(() => {
    void [width, height];
    resize();
  });

  onMount(() => {
    void start();
    const unregister = registerBoard((ids) => {
      if (!act((e) => e.a11y())?.some((i) => i.id === ids[0])) return false;
      ids.forEach((i) => select(i));
      return true;
    });
    // the wheel must not scroll the page, so it is not passive
    const wheel = (e: WheelEvent) => {
      if (!engine) return;
      e.preventDefault();
      if (edit) return; // the overlay's rect would no longer match
      const r = canvas!.getBoundingClientRect();
      send({ t: "wheel", x: e.clientX - r.left, y: e.clientY - r.top, dx: e.deltaX, dy: e.deltaY, mods: MODS(e) });
    };
    host.addEventListener("wheel", wheel, { passive: false });
    const dark = matchMedia("(prefers-color-scheme: dark)");
    const theme = () => (act((e) => e.palette(palette(host))), schedule());
    dark.addEventListener?.("change", theme);
    return () => {
      unregister();
      host.removeEventListener("wheel", wheel);
      dark.removeEventListener?.("change", theme);
    };
  });

  onDestroy(() => {
    cancelAnimationFrame(pending);
    if (engine) {
      try {
        onbeforefree?.(engine);
      } catch (e) {
        console.error(e); // the engine is freed regardless
      }
    }
    if (engine && canvas) release(canvas, engine);
    engine = null;
  });
</script>

<div class="board" bind:this={host} bind:clientWidth={width} bind:clientHeight={height} role="presentation" {onkeydown}>
  {#if failure}
    <div class="failure" role="alert">
      <p>{failure.text}</p>
      {#if failure.retry}<button onclick={() => (panicked ? reload() : start())}>Retry</button>{/if}
    </div>
  {:else}
    <canvas bind:this={canvas} tabindex="0" aria-label="Board canvas; the element list follows" style="cursor:{cursor}"
      {onpointerdown}
      title={hover ? reasons[hover] : undefined}
      onpointermove={onmove}
      onpointerup={(e) => edit || send({ t: "up", ...point(e) })}
      oncontextmenu={(e) => e.preventDefault()}
      onmousedown={(e) => e.preventDefault() /* focus is set in pointerdown; the default would take it from the rename overlay */}></canvas>
    <Labels {camera} {labels} {hand} />
    {#if edit}<EditOverlay rect={edit.rect} text={edit.text} oncommit={commit} oncancel={cancel} />{/if}
    <A11yList {items} {message} onselect={select} onkey={(key) => send({ t: "key", key, mods: 0 })} />
    {#if status}<div class="status" role="status">{status}</div>{/if}
  {/if}
</div>

<style>
  /* the screens' paper and ink: the same in both themes */
  .board { position: relative; width: 100%; height: 100%; overflow: hidden; background: var(--bg); --paper: #f4f1ea; --ink: #2b2b2b; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; outline: none; touch-action: none; }
  .failure { max-width: 34em; margin: 15vh auto 0; padding: 16px; color: var(--fg); }
  .status { position: absolute; left: 8px; bottom: 8px; padding: 2px 8px; border-radius: 4px; font-size: 12px;
    color: var(--error); background: var(--panel); border: 1px solid var(--line); }
</style>
