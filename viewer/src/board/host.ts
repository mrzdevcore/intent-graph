// Loads the board engine (crates/ig-board as wasm, lazily) onto a canvas and adapts its JSON
// strings to `BoardEngine`. Desktop app only: `ig view` builds never import this file.

import type { A11yItem, BoardEngine, Camera, EngineOut, Label } from "./engine";

import { loadFailed, noWebgl2 } from "./messages";

export { loadFailed, noWebgl2 };

/** What host.ts uses of the generated glue (./pkg/ig_board.js). */
export interface RawEngine {
  attach(canvas: HTMLCanvasElement): void;
  load(board: string, context: string): void;
  input(ev: string): string;
  ops(ops: string, step: string): string;
  external(disk: string, turn?: string | null): string;
  setCheck(targets: string): void;
  dismiss(id?: string | null): void;
  highlighted(): string;
  select(ids: string): void;
  undo(): string;
  redo(): string;
  board(): string;
  saved(written: string): void;
  palette(json: string): void;
  resize(w: number, h: number, dpr: number): void;
  frame(): boolean;
  restore(): void;
  camera(): string;
  labels(): string;
  a11y(): string;
  setImage(asset: string, bitmap: ImageBitmap): void;
  free(): void;
}
export interface Pkg {
  BoardEngine: new () => RawEngine;
  lastPanic(): string | undefined;
}
export type Importer = () => Promise<Pkg>;

type Glue = Pkg & { default(init: { module_or_path: string }): Promise<unknown> };
// Globs, so a build without pkg/ still compiles and then says to rebuild; each is a lazy chunk.
const glue = import.meta.glob<Glue>("./pkg/ig_board.js");
const wasm = import.meta.glob<string>("./pkg/ig_board_bg.wasm", { query: "?url", import: "default" });

/** The real import: the glue, then the .wasm by URL. */
export const importPkg: Importer = async () => {
  const js = glue["./pkg/ig_board.js"], url = wasm["./pkg/ig_board_bg.wasm"];
  if (!js || !url) throw new Error("viewer/src/board/pkg is missing");
  const [m, u] = await Promise.all([js(), url()]);
  await m.default({ module_or_path: u });
  return m;
};

const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** A Rust panic traps the wasm: the engine is dead, `onFail` gets the panic's message. A
 *  returned `Err` (bad JSON from the host) is thrown as an Error and the engine lives on. */
export function wrap(raw: RawEngine, pkg: Pkg, onFail: (reason: string) => void): BoardEngine {
  let dead = false;
  const g = <T>(f: () => T): T => {
    if (dead) throw new Error("the board engine stopped");
    try {
      return f();
    } catch (e) {
      if (!(e instanceof WebAssembly.RuntimeError)) throw new Error(msg(e));
      dead = true;
      const reason = pkg.lastPanic() ?? msg(e);
      onFail(reason);
      throw new Error(reason);
    }
  };
  const out = (s: string) => JSON.parse(s) as EngineOut;
  return {
    load: (b, c) => g(() => raw.load(b, c)),
    input: (ev) => g(() => out(raw.input(JSON.stringify(ev)))),
    ops: (o, step) => g(() => out(raw.ops(o, JSON.stringify(step)))),
    external: (d, turn) => g(() => out(raw.external(d, turn))),
    setCheck: (t) => g(() => raw.setCheck(t)),
    dismiss: (id) => g(() => raw.dismiss(id)),
    highlighted: () => g(() => JSON.parse(raw.highlighted()) as string[]),
    select: (ids) => g(() => raw.select(JSON.stringify(ids))),
    undo: () => g(() => out(raw.undo())),
    redo: () => g(() => out(raw.redo())),
    board: () => g(() => raw.board()),
    saved: (w) => g(() => raw.saved(w)),
    palette: (p) => g(() => raw.palette(JSON.stringify(p))),
    resize: (w, h, dpr) => g(() => raw.resize(w, h, dpr)),
    frame: () => g(() => raw.frame()),
    restore: () => g(() => raw.restore()),
    camera: () => g(() => JSON.parse(raw.camera()) as Camera),
    labels: () => g(() => JSON.parse(raw.labels()) as Label[]),
    a11y: () => g(() => JSON.parse(raw.a11y()) as A11yItem[]),
    setImage: (a, b) => g(() => raw.setImage(a, b)),
    free: () => raw.free(),
  };
}

/** Dev flag (Web Inspector console): `localStorage.setItem("ig.dev.noWebgl2", "1")`. */
function webgl2Off(): boolean {
  try {
    return localStorage.getItem("ig.dev.noWebgl2") === "1";
  } catch {
    return false;
  }
}

export type Loaded = { engine: BoardEngine } | { error: string; retry: boolean };

/** Imports the engine and attaches it to `canvas`. A failed import is a rebuild message (with
 *  Retry); no WebGL2 is a message of its own. `onFail` reports a later panic. */
export async function loadEngine(canvas: HTMLCanvasElement, importer: Importer = importPkg,
  onFail: (reason: string) => void = () => {}): Promise<Loaded> {
  let pkg: Pkg;
  try {
    pkg = await importer();
  } catch (e) {
    return { error: loadFailed(msg(e)), retry: true };
  }
  let why = "";
  const note = (e: Event) => (why = (e as WebGLContextEvent).statusMessage || why);
  canvas.addEventListener("webglcontextcreationerror", note);
  const raw = new pkg.BoardEngine();
  try {
    if (webgl2Off()) throw new Error("turned off by ig.dev.noWebgl2");
    raw.attach(canvas);
  } catch (e) {
    raw.free();
    return { error: noWebgl2(why || msg(e)), retry: false };
  } finally {
    canvas.removeEventListener("webglcontextcreationerror", note);
  }
  return { engine: wrap(raw, pkg, onFail) };
}

/** Unmount: the context is given back now (browsers cap live WebGL contexts), then the engine
 *  frees its GL objects and memory. */
export function release(canvas: HTMLCanvasElement, engine: BoardEngine): void {
  try {
    canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
  } finally {
    engine.free();
  }
}

/** Paint names the renderer knows, from the theme tokens. */
const TOKENS: Record<string, [string, number]> = {
  lane: ["--code", 0.7], context: ["--panel", 0.6], contextBorder: ["--line", 1], contextEdge: ["--unchanged", 0.5],
  surface: ["--panel", 1], link: ["--muted", 1], selected: ["--note", 1], highlight: ["--accent", 1],
  error: ["--error", 1], warning: ["--warning", 1], note: ["--note", 1],
  "mark.changed": ["--changed", 1], "mark.remove": ["--removed", 1],
  "element.module": ["--fg", 0.7], "element.function": ["--note", 1], "element.route": ["--added", 1],
  "element.system": ["--muted", 1], "element.note": ["--changed", 1], "element.frame": ["--muted", 1],
};

/** `#rgb` / `#rrggbb` to 0..1 RGBA; anything else grey. */
export function rgba(hex: string, a = 1): [number, number, number, number] {
  let h = hex.trim().replace(/^#/, "");
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  const n = /^[0-9a-f]{6}$/i.test(h) ? parseInt(h, 16) : 0x808080;
  return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, a];
}

export function palette(el: Element): Record<string, [number, number, number, number]> {
  const css = getComputedStyle(el);
  return Object.fromEntries(Object.entries(TOKENS).map(([k, [v, a]]) => [k, rgba(css.getPropertyValue(v), a)]));
}
