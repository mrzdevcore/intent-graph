// The board's link to its files, its live check and Claude's turns: autosave after 300 ms of quiet,
// compare-and-swap writes that merge on conflict, one check in flight with the latest winning,
// and outside changes merged through the engine. An unreadable file is never written. With the
// screens (desktop), each screen file is written only when it changed, and images go through
// putAsset (Rust never decodes them; the host does, for the texture).

import type { Design, DesignApi, DesignCheck, Kept, Screen, ScreensApi } from "../api";
import type { ViewNode } from "../data";
import { proposeFile } from "../model/design";
import type { BoardEngine, EngineOut } from "./engine";

/** Agent changes arriving up to this long after a turn ends still belong to that turn. */
export const AGENT_GRACE_MS = 1000;
export interface AgentTurn { id: string; start: number; end: number | null }

/** The turn an agent change at time `at` (the host's clock) belongs to, or null: the engine's
 *  `external(disk, turn)` then joins that turn's undo step. */
export function agentStep(at: number, turns: AgentTurn[]): string | null {
  const t = [...turns].reverse().find((t) => at >= t.start && (t.end === null || at <= t.end + AGENT_GRACE_MS));
  return t ? t.id : null;
}

/** Writes the board; only once the write succeeded is the written text the merge base (an edit
 *  made during the write stays unsaved, so a later merge keeps it). Every write goes through here. */
export async function save(engine: Pick<BoardEngine, "board" | "saved">, put: (text: string) => Promise<void>): Promise<string> {
  const text = engine.board();
  await put(text);
  engine.saved(text);
  return text;
}

export const QUIET_MS = 300;
export const RETRY_MS = 2000;

export interface Clock {
  now(): number;
  /** Runs `f` after `ms`; returns the cancel. */
  after(ms: number, f: () => void): () => void;
}
export const realClock: Clock = {
  now: () => Date.now(),
  after: (ms, f) => {
    const h = setTimeout(f, ms);
    return () => clearTimeout(h);
  },
};

export type StoreEngine = Pick<BoardEngine, "load" | "ops" | "external" | "externalFrom" | "setCheck" | "board" | "saved">
  & Partial<Pick<BoardEngine, "screens" | "setImage" | "importImage" | "dropImage">>;

/** Decodes an image for its texture; null when it can't (a placeholder shows instead). */
export type Decode = (bytes: Uint8Array) => Promise<ImageBitmap | null>;
export const decodeImage: Decode = (bytes) => createImageBitmap(new Blob([bytes as BlobPart])).catch(() => null);

/** The assets a set of screen texts shows: backgrounds and image pieces. */
export function assetsOf(screens: Record<string, string>): string[] {
  const out = new Set<string>();
  for (const t of Object.values(screens)) {
    const s = JSON.parse(t) as Screen;
    if (s.background) out.add(s.background.asset);
    for (const p of s.elements) if (p.props?.asset) out.add(p.props.asset);
  }
  return [...out];
}
export interface StoreHooks {
  /** The Context JSON for `engine.load`. */
  context(): string;
  /** The graph's nodes, for proposing a placed element's file. */
  nodes(): ViewNode[];
  /** Claude's turns, for grouping its edits into one undo step. */
  turns?(): AgentTurn[];
  /** Something the chrome shows changed (status, errors, check). */
  changed?(): void;
  /** Claude's edit was merged: `disk` is the board on disk now, as `text()` gives it (the
   *  outbox's new baseline). */
  agentMerged?(disk: string): void;
}

/** Tauri's invoke rejects with a plain string. */
const msg = (e: unknown) => String((e as Error)?.message ?? e);

const blank = (): Design => ({ version: 1, title: "", scope: { kind: "dir", id: "." }, elements: [], marks: [], links: [], bridges: [] });

const RANK = { note: 0, warning: 1, error: 2 } as const;

/** Check findings as engine targets: per target the worst severity and every reason. */
export function targets(c: DesignCheck): [string, string, string][] {
  const by = new Map<string, { sev: keyof typeof RANK; why: string[] }>();
  for (const f of c.findings) {
    if (!f.target) continue;
    const t = by.get(f.target) ?? { sev: f.severity, why: [] };
    if (RANK[f.severity] > RANK[t.sev]) t.sev = f.severity;
    t.why.push(f.message);
    by.set(f.target, t);
  }
  return [...by].map(([id, t]) => [id, t.sev, t.why.join("; ")]);
}

const KEPT = "ig-unsaved";
/** A recovery file: the board text and the board on disk it was made over, both in `text()`'s form. */
interface KeptCopy { kind: typeof KEPT; version: 2; base: string; board: string }

/** A recovery file's board and base; an older file is the bare board text (base null). */
export function parseKept(text: string): { board: string; base: string | null } {
  try {
    const c = JSON.parse(text) as Partial<KeptCopy>;
    if (c.kind === KEPT && typeof c.board === "string" && typeof c.base === "string") return { board: c.board, base: c.base };
  } catch { /* not JSON: the engine reports it */ }
  return { board: text, base: null };
}

export class BoardStore {
  /** By file path: why it can't be read. */
  errors: Record<string, string> = {};
  /** File paths never written (unreadable on disk). */
  readOnly = new Set<string>();
  check: DesignCheck | null = null;
  checkError: string | null = null;
  saveError: string | null = null;
  saving = false;
  readonly path: string;

  private loaded = false;
  private dirty = false;
  private writing: Promise<void> | null = null;
  /** Disposed: the engine may be freed or showing another feature; it is never touched again. */
  private closed = false;
  private cancelSave: (() => void) | null = null;
  private seq = 0;
  private checking = false;
  private rerun = false;
  /** The last placed element and the file proposed for it: its first rename re-proposes. */
  private proposed: { id: string; name: string; file: string } | null = null;
  /** The screens folder: its key in `errors` and `readOnly` (an invalid screen file). */
  readonly screensPath: string;
  /** By screen id: the screen as on disk (compact JSON), to write only what changed. */
  private disk: Record<string, string> = {};
  /** Images whose texture is loaded (or failed to decode). */
  private images = new Set<string>();
  /** Why the last image was not taken ("Only PNG or JPEG up to 10 MB"). */
  imageError: string | null = null;

  constructor(private api: DesignApi, private engine: StoreEngine, readonly slug: string,
    private clock: Clock = realClock, private hooks: StoreHooks = { context: () => "{}", nodes: () => [] },
    private screensApi: ScreensApi | null = null, private decode: Decode = decodeImage) {
    this.path = `plans/${slug}/design.json`;
    this.screensPath = `plans/${slug}/screens/`;
  }

  /** design.json as last read or written. */
  private diskDesign: Design | null = null;

  /** The board on disk as one text, in `text()`'s form. */
  private diskText(): string {
    if (!this.screensApi) return JSON.stringify(this.diskDesign);
    return JSON.stringify({ design: this.diskDesign, screens: Object.values(this.disk).map((t) => JSON.parse(t) as Screen) });
  }

  /** The board as one text: the design, with every screen when there are screens (the outbox's
   *  and the recovery file's form). */
  text(): string {
    if (!this.screensApi || !this.engine.screens) return this.engine.board();
    const screens = Object.values(this.engine.screens()).map((t) => JSON.parse(t) as Screen);
    return JSON.stringify({ design: JSON.parse(this.engine.board()), screens });
  }

  /** The screens on disk; an invalid file makes them read-only (null). */
  private async listScreens(): Promise<Screen[] | null> {
    if (!this.screensApi) return [];
    try {
      const list = await this.screensApi.list(this.slug);
      delete this.errors[this.screensPath];
      this.readOnly.delete(this.screensPath);
      return list;
    } catch (e) {
      this.errors[this.screensPath] = msg(e);
      this.readOnly.add(this.screensPath);
      return null;
    }
  }

  private setDisk(list: Screen[]) {
    this.disk = Object.fromEntries(list.map((s) => [s.id, JSON.stringify(s)]));
  }

  /** Something unsaved: a change waits for its quiet time or a write is running. */
  get pending(): boolean {
    return this.dirty || this.saving;
  }

  /** Reads the file into the engine; an unreadable one leaves a blank board, read-only. */
  async load(): Promise<void> {
    let d: Design;
    try {
      d = await this.api.read(this.slug);
    } catch (e) {
      if (this.closed) return;
      this.engine.load(JSON.stringify(blank()), this.hooks.context());
      this.fail(e);
      return;
    }
    if (this.closed) return;
    this.ok();
    const screens = this.screensApi ? await this.listScreens() : null;
    if (this.closed) return;
    this.engine.load(JSON.stringify(screens ? { design: d, screens } : d), this.hooks.context());
    if (screens) this.setDisk(screens);
    this.diskDesign = d;
    this.loaded = true;
    this.recheck();
    this.notify();
    void this.loadImages();
  }

  /** Textures for the images the screens show, each read and decoded once (`asset-changed`
   *  calls this again: idempotent). One that can't be read or decoded stays a placeholder. */
  async loadImages(changed?: string): Promise<void> {
    const api = this.screensApi, e = this.engine;
    if (!api || !e.screens || !e.setImage || this.closed) return;
    const shown = assetsOf(e.screens());
    // a replaced file is read again; one no screen shows any more gives its texture back
    for (const name of [...this.images].filter((n) => n === changed || !shown.includes(n))) {
      this.images.delete(name);
      e.dropImage?.(name);
    }
    for (const name of shown.filter((n) => !this.images.has(n))) {
      this.images.add(name);
      const bytes = await api.readAsset(this.slug, name).catch((err: unknown) => (console.error(err), null));
      const bitmap = bytes && (await this.decode(bytes));
      if (bitmap && !this.closed) e.setImage(name, bitmap);
    }
  }

  /** A pasted, dropped or picked image: stored, then a screen's background (one undo step). A
   *  refusal (not PNG or JPEG, over 10 MB) changes nothing and says why. */
  async importImage(bytes: Uint8Array): Promise<void> {
    const api = this.screensApi, e = this.engine;
    if (!api || !e.importImage || this.closed) return;
    let name: string;
    try {
      name = await api.putAsset(this.slug, bytes);
    } catch (err) {
      this.imageError = msg(err);
      this.notify();
      return;
    }
    this.imageError = null;
    this.images.add(name);
    const bitmap = await this.decode(bytes);
    if (this.closed) return;
    if (bitmap) e.setImage?.(name, bitmap);
    this.onOut(e.importImage(name));
  }

  /** A screen file changed outside: every screen is read again and merged in (one undo step,
   *  Claude's turn when it is one); an invalid file is reported, and no screen is written. */
  async externalScreens(): Promise<boolean> {
    if (!this.screensApi) return true;
    const list = await this.listScreens();
    if (this.closed) return false;
    if (!list || !this.loaded) {
      this.notify();
      return false;
    }
    const turn = agentStep(this.clock.now(), this.hooks.turns?.() ?? []);
    this.onOut(this.engine.external(JSON.stringify({ screens: list }), turn));
    this.setDisk(list);
    if (turn) this.hooks.agentMerged?.(this.diskText());
    this.notify();
    void this.loadImages();
    return true;
  }

  /** Every engine answer: save on change, check on a change of meaning, propose a placed file. */
  onOut(o: EngineOut): void {
    if (o.changed) {
      this.queueSave();
      this.follow();
      if (this.screensApi) void this.loadImages();
    }
    if (o.placed) this.place(o.placed.id, o.placed.layer);
    if (o.meaning) this.recheck();
    this.notify();
  }

  /** Writes now (feature switch, app quit): after any write in flight has fully finished, so its
   *  merge base and conflict merge land before the caller moves on. */
  async flush(): Promise<void> {
    this.cancelSave?.();
    this.cancelSave = null;
    while (this.writing) await this.writing;
    if (this.closed || this.readOnly.has(this.path) || !this.loaded || !this.dirty) return;
    this.writing = this.write().finally(() => {
      this.writing = null;
      this.saving = false;
      this.notify();
    });
    await this.writing;
  }

  /** Writes until nothing is unsaved (feature switch, quit): edits made during a write are
   *  written too. What can't be written (a failing write, an unreadable file) is kept as the
   *  recovery copy, never dropped. */
  async finish(): Promise<void> {
    for (let i = 0; i < 5 && !this.closed && (this.dirty || this.writing); i++) {
      await this.flush();
      if (this.saveError || this.readOnly.has(this.path)) break;
    }
    if (this.loaded && !this.closed && this.dirty) await this.keep(this.text(), this.saveError);
  }

  /** The engine is about to be freed: the board text is taken now, synchronously, and written
   *  once any write in flight is done, if it still needs writing; if that fails it is kept as
   *  the recovery copy. The store is disposed. */
  close(): Promise<void> {
    const text = this.loaded ? this.engine.board() : null;
    const all = this.loaded ? this.text() : null;
    const screens = this.loaded && this.screensApi ? this.engine.screens?.() : undefined;
    const invalid = this.readOnly.has(this.path);
    this.dispose();
    return (async () => {
      while (this.writing) await this.writing;
      if (!text || !this.dirty) return;
      if (invalid) return this.keep(all!, this.errors[this.path]);
      this.dirty = false;
      // no merge on a conflict here (the engine is gone): the text is kept instead
      try {
        await this.api.write(this.slug, JSON.parse(text) as Design);
        if (screens) await this.writeScreens(screens, () => {});
      } catch (e) {
        this.dirty = true;
        this.saveError = msg(e);
        await this.keep(all!, e);
      }
    })();
  }

  /** Board text that could not be written, kept with the disk it was made over (its merge base):
   *  plans/<slug>/design.unsaved.json, or the next free design.unsaved.<n>.json. */
  private async keep(text: string, why: unknown) {
    console.error(`${this.path}: not written (${msg(why)}); keeping the edits in design.unsaved.json`);
    const copy: KeptCopy = { kind: KEPT, version: 2, base: this.diskText(), board: text };
    await this.api.keepUnsaved(this.slug, JSON.stringify(copy)).catch((e: unknown) => console.error(e));
  }

  /** A recovery copy, merged into the board over its own base as one outside step (later work
   *  on disk stays), then deleted. An older copy (the bare board) adds and changes only. */
  async restore(k: Pick<Kept, "name" | "text">): Promise<void> {
    if (this.closed) return;
    const c = parseKept(k.text);
    this.onOut(this.engine.externalFrom(c.board, c.base));
    await this.api.discardUnsaved(this.slug, k.name);
  }

  discard(name: string): Promise<void> {
    return this.api.discardUnsaved(this.slug, name);
  }

  /** The file changed outside: merged in (one undo step, Claude's turn when it is one), or, when
   *  invalid, reported and left alone. */
  async external(): Promise<boolean> {
    let d: Design;
    try {
      d = await this.api.read(this.slug);
    } catch (e) {
      if (!this.closed) this.fail(e);
      return false;
    }
    if (this.closed) return false;
    this.ok();
    const text = JSON.stringify(d);
    this.diskDesign = d;
    if (!this.loaded) {
      const screens = this.screensApi ? await this.listScreens() : null;
      if (this.closed) return false;
      this.engine.load(JSON.stringify(screens ? { design: d, screens } : d), this.hooks.context());
      if (screens) this.setDisk(screens);
      this.loaded = true;
      this.dirty = false; // the blank board's state is gone
      this.recheck();
    } else {
      const turn = agentStep(this.clock.now(), this.hooks.turns?.() ?? []);
      const o = this.engine.external(text, turn);
      this.onOut(o);
      if (turn) this.hooks.agentMerged?.(this.diskText());
    }
    this.notify();
    return true;
  }

  /** Stops the timers (the owner flushes first). */
  dispose(): void {
    this.closed = true;
    this.cancelSave?.();
    this.cancelSave = null;
    this.seq++;
  }

  private notify() {
    this.hooks.changed?.();
  }

  private ok() {
    delete this.errors[this.path];
    this.readOnly.delete(this.path);
  }

  private fail(e: unknown) {
    this.errors[this.path] = msg(e);
    this.readOnly.add(this.path);
    this.cancelSave?.();
    this.cancelSave = null;
    this.notify();
  }

  private queueSave() {
    this.dirty = true; // also while read-only: never written there, but kept on finish or close
    if (this.readOnly.has(this.path)) return;
    this.cancelSave?.();
    this.cancelSave = this.clock.after(QUIET_MS, () => void this.flush());
  }

  private async write(): Promise<void> {
    this.saving = true;
    this.notify();
    for (let attempt = 0; ; attempt++) {
      this.dirty = false;
      try {
        // a write that ends after dispose never sets the merge base of what the engine shows now
        const engine = { board: () => this.engine.board(), saved: (t: string) => this.closed || this.engine.saved(t) };
        this.diskDesign = JSON.parse(await save(engine, (t) => this.api.write(this.slug, JSON.parse(t) as Design))) as Design;
        const now = this.screensApi ? this.engine.screens?.() : undefined;
        if (now) await this.writeScreens(now, (p) => this.closed || this.engine.saved(JSON.stringify({ screens: p })));
        this.saveError = null;
        return;
      } catch (e) {
        this.dirty = true;
        // someone else wrote: merge theirs in, then write the result once more
        if (this.closed) return;
        if (msg(e) === "conflict" && attempt === 0 && (await this.external()) && (await this.externalScreens())) continue;
        if (this.closed || this.readOnly.has(this.path)) return;
        this.saveError = msg(e);
        this.cancelSave?.();
        this.cancelSave = this.clock.after(RETRY_MS, () => void this.flush());
        return;
      }
    }
  }

  /** Writes the screens that differ from disk and removes the ones gone; each written one
   *  becomes the merge base (`saved`). Never while a screen file is invalid. */
  private async writeScreens(now: Record<string, string>, saved: (p: Record<string, Screen | null>) => void) {
    const api = this.screensApi;
    if (!api || this.readOnly.has(this.screensPath)) return;
    for (const [id, text] of Object.entries(now)) {
      const s = JSON.parse(text) as Screen;
      if (this.disk[id] === JSON.stringify(s)) continue;
      await api.write(this.slug, s);
      this.disk[id] = JSON.stringify(s);
      saved({ [id]: s });
    }
    for (const id of Object.keys(this.disk).filter((id) => !(id in now))) {
      await api.remove(this.slug, id);
      delete this.disk[id];
      saved({ [id]: null });
    }
  }

  /** Checks the board again (a change of meaning, or the code graph was rebuilt). */
  recheck() {
    if (this.closed) return;
    if (this.checking) {
      this.rerun = true;
      return;
    }
    void this.runCheck();
  }

  private async runCheck() {
    this.checking = true;
    const seq = ++this.seq;
    try {
      const r = await this.api.check(this.slug, JSON.parse(this.engine.board()) as Design, seq);
      // the latest wins: a response for an older board, or one an edit overtook, is dropped
      if (r.seq === this.seq && !this.rerun) {
        this.check = r;
        this.checkError = null;
        this.engine.setCheck(JSON.stringify(targets(r)));
      }
    } catch (e) {
      if (seq === this.seq) this.checkError = msg(e);
    } finally {
      this.checking = false;
      this.notify();
      if (this.rerun) {
        this.rerun = false;
        this.recheck();
      }
    }
  }

  private element(id: string) {
    return (JSON.parse(this.engine.board()) as Design).elements.find((e) => e.id === id);
  }

  private setFile(id: string, file: string) {
    // joined to the step that placed or renamed it: one undo takes both back
    this.onOut(this.engine.ops(JSON.stringify([{ op: "update", id, set: { file } }]), "amend"));
  }

  /** A placed element gets the file its layer's files suggest. */
  private place(id: string, layer: string | null) {
    const e = this.element(id);
    const file = e && !e.file && proposeFile(this.hooks.nodes(), layer, e.kind, e.name);
    if (!e || !file) return;
    this.setFile(id, file);
    this.proposed = { id, name: e.name, file };
  }

  /** The placed element's first rename: the proposal follows the name while the file is untouched. */
  private follow() {
    const p = this.proposed;
    if (!p) return;
    const e = this.element(p.id);
    if (!e || e.file !== p.file) return void (this.proposed = null);
    if (e.name === p.name) return;
    this.proposed = null;
    const file = proposeFile(this.hooks.nodes(), e.layer ?? null, e.kind, e.name);
    if (file && file !== e.file) this.setFile(e.id, file);
  }
}
