// The board's link to its file, its live check and Claude's turns: autosave after 300 ms of quiet,
// compare-and-swap writes that merge on conflict, one check in flight with the latest winning,
// and outside changes merged through the engine. An unreadable file is never written.

import type { Design, DesignApi, DesignCheck } from "../api";
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

export type StoreEngine = Pick<BoardEngine, "load" | "ops" | "external" | "setCheck" | "board" | "saved">;
export interface StoreHooks {
  /** The Context JSON for `engine.load`. */
  context(): string;
  /** The graph's nodes, for proposing a placed element's file. */
  nodes(): ViewNode[];
  /** Claude's turns, for grouping its edits into one undo step. */
  turns?(): AgentTurn[];
  /** Something the chrome shows changed (status, errors, check). */
  changed?(): void;
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

  constructor(private api: DesignApi, private engine: StoreEngine, readonly slug: string,
    private clock: Clock = realClock, private hooks: StoreHooks = { context: () => "{}", nodes: () => [] }) {
    this.path = `plans/${slug}/design.json`;
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
    this.engine.load(JSON.stringify(d), this.hooks.context());
    this.loaded = true;
    this.recheck();
    this.notify();
  }

  /** Every engine answer: save on change, check on a change of meaning, propose a placed file. */
  onOut(o: EngineOut): void {
    if (o.changed) {
      this.queueSave();
      this.follow();
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
    if (this.loaded && !this.closed && this.dirty) await this.keep(this.engine.board(), this.saveError);
  }

  /** The engine is about to be freed: the board text is taken now, synchronously, and written
   *  once any write in flight is done, if it still needs writing; if that fails it is kept as
   *  the recovery copy. The store is disposed. */
  close(): Promise<void> {
    const text = this.loaded ? this.engine.board() : null;
    const invalid = this.readOnly.has(this.path);
    this.dispose();
    return (async () => {
      while (this.writing) await this.writing;
      if (!text || !this.dirty) return;
      if (invalid) return this.keep(text, this.errors[this.path]);
      this.dirty = false;
      // no merge on a conflict here (the engine is gone): the text is kept instead
      await this.api.write(this.slug, JSON.parse(text) as Design).catch((e) => {
        this.dirty = true;
        this.saveError = msg(e);
        return this.keep(text, e);
      });
    })();
  }

  /** Board text that could not be written: plans/<slug>/design.unsaved.json. */
  private async keep(text: string, why: unknown) {
    console.error(`${this.path}: not written (${msg(why)}); keeping the edits in design.unsaved.json`);
    await this.api.keepUnsaved(this.slug, text).catch((e: unknown) => console.error(e));
  }

  /** The recovery copy, merged into the board as one outside step, then deleted. */
  async restore(text: string): Promise<void> {
    if (this.closed) return;
    this.onOut(this.engine.external(text, null));
    await this.api.discardUnsaved(this.slug);
  }

  discard(): Promise<void> {
    return this.api.discardUnsaved(this.slug);
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
    if (!this.loaded) {
      this.engine.load(text, this.hooks.context());
      this.loaded = true;
      this.dirty = false; // the blank board's state is gone
      this.recheck();
    } else {
      this.onOut(this.engine.external(text, agentStep(this.clock.now(), this.hooks.turns?.() ?? [])));
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
        await save(engine, (t) => this.api.write(this.slug, JSON.parse(t) as Design));
        this.saveError = null;
        return;
      } catch (e) {
        this.dirty = true;
        // someone else wrote: merge theirs in, then write the result once more
        if (this.closed) return;
        if (msg(e) === "conflict" && attempt === 0 && (await this.external())) continue;
        if (this.closed || this.readOnly.has(this.path)) return;
        this.saveError = msg(e);
        this.cancelSave?.();
        this.cancelSave = this.clock.after(RETRY_MS, () => void this.flush());
        return;
      }
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
