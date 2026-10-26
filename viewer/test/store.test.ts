import { expect, test } from "vitest";
import type { Design, DesignApi, DesignCheck } from "../src/api";
import type { ViewNode } from "../src/data";
import type { EngineOut } from "../src/board/engine";
import { agentStep, BoardStore, save, type Clock, type StoreEngine } from "../src/board/store";

test("agentStep groups arrivals up to 1 s after the turn ends", () => {
  const turns = [{ id: "t1", start: 100, end: 500 }, { id: "t2", start: 5000, end: null }];
  expect(agentStep(300, turns)).toBe("t1");
  expect(agentStep(1500, turns)).toBe("t1");
  expect(agentStep(1501, turns)).toBeNull();
  expect(agentStep(9000, turns)).toBe("t2");
  expect(agentStep(50, turns)).toBeNull();
});

test("save makes the written text the merge base, only after the write succeeds", async () => {
  let board = "v1";
  const saved: string[] = [];
  const engine = { board: () => board, saved: (t: string) => void saved.push(t) };
  // an edit lands while the write is in flight: the base is still what was written
  await save(engine, async () => void (board = "v2"));
  expect(saved).toEqual(["v1"]);
  await expect(save(engine, () => Promise.reject(new Error("disk full")))).rejects.toThrow("disk full");
  expect(saved).toEqual(["v1"]);
});

const OUT = (o: Partial<EngineOut> = {}): EngineOut =>
  ({ redraw: false, changed: false, meaning: false, selection: [], edit: null, placed: null, cursor: "default", error: null, ...o });

const design = (els: Design["elements"] = []): Design =>
  ({ version: 1, title: "T", scope: { kind: "dir", id: "." }, elements: els, marks: [], links: [], bridges: [] });

/** Timers run only on `advance`. */
function fakeClock() {
  let t = 0;
  let timers: { at: number; f: () => void }[] = [];
  const clock: Clock = {
    now: () => t,
    after: (ms, f) => {
      const x = { at: t + ms, f };
      timers.push(x);
      return () => (timers = timers.filter((y) => y !== x));
    },
  };
  const advance = async (ms: number) => {
    t += ms;
    const due = timers.filter((x) => x.at <= t);
    timers = timers.filter((x) => x.at > t);
    due.forEach((x) => x.f());
    await settle();
  };
  return { clock, advance, at: (v: number) => (t = v) };
}

const settle = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

/** A fake engine: the board is a design object; every call is recorded. */
function fakeEngine(d = design()) {
  const calls: [string, unknown[]][] = [];
  let board = d;
  const engine: StoreEngine = {
    load: (b, c) => void calls.push(["load", [b, c]]),
    ops: (o, s) => {
      calls.push(["ops", [JSON.parse(o), s]]);
      return OUT({ changed: true });
    },
    external: (disk, turn) => {
      calls.push(["external", [JSON.parse(disk), turn]]);
      return OUT({ changed: true, meaning: true });
    },
    setCheck: (t) => void calls.push(["setCheck", [JSON.parse(t)]]),
    board: () => JSON.stringify(board),
    saved: (w) => void calls.push(["saved", [w]]),
  };
  const of = (m: string) => calls.filter((c) => c[0] === m).map((c) => c[1]);
  return { engine, calls, of, set: (x: Design) => (board = x) };
}

type Deferred<T> = { promise: Promise<T>; resolve: (v: T) => void; reject: (e: unknown) => void };
function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void, reject!: (e: unknown) => void;
  const promise = new Promise<T>((a, b) => ((resolve = a), (reject = b)));
  return { promise, resolve, reject };
}

const result = (seq: number, findings: DesignCheck["findings"] = []): DesignCheck =>
  ({ ok: true, ms: 12, seq, volume: {} as DesignCheck["volume"], findings });

function fakeApi(disk: () => Design | Error = () => design()) {
  const writes: Design[] = [];
  const checks: { seq: number; d: Deferred<DesignCheck> }[] = [];
  let writeResult: (() => Promise<void>) | null = null;
  const api: DesignApi = {
    features: async () => [],
    create: async () => { throw new Error("unused"); },
    read: async () => {
      const d = disk();
      if (d instanceof Error) throw d;
      return d;
    },
    write: async (_slug, d) => {
      writes.push(d);
      if (writeResult) return writeResult();
    },
    check: (_slug, _d, seq) => {
      const d = deferred<DesignCheck>();
      checks.push({ seq: seq!, d });
      return d.promise;
    },
    onChanged: () => () => {},
  };
  return { api, writes, checks, failWrites: (f: (() => Promise<void>) | null) => (writeResult = f) };
}

const NODES: ViewNode[] = [{ id: "server/api/a.ts", file: "server/api/a.ts", module: true, layer: "api" } as ViewNode];

function setup(opts: { disk?: () => Design | Error; board?: Design; turns?: () => { id: string; start: number; end: number | null }[] } = {}) {
  const c = fakeClock(), e = fakeEngine(opts.board), a = fakeApi(opts.disk);
  const store = new BoardStore(a.api, e.engine, "export", c.clock, { context: () => "{}", nodes: () => NODES, turns: opts.turns });
  return { ...c, ...e, ...a, store };
}

test("three quick changes save once after 300 ms", async () => {
  const s = setup();
  await s.store.load();
  for (let i = 0; i < 3; i++) {
    if (i) await s.advance(100);
    s.store.onOut(OUT({ changed: true }));
  }
  expect(s.writes).toHaveLength(0);
  await s.advance(299);
  expect(s.writes).toHaveLength(0);
  await s.advance(1);
  expect(s.writes).toHaveLength(1);
  expect(s.of("saved")).toHaveLength(1);
});

test("only meaning changes re-check", async () => {
  const s = setup();
  await s.store.load();
  s.checks[0].d.resolve(result(s.checks[0].seq));
  await settle();
  const n = s.checks.length;
  s.store.onOut(OUT({ changed: true }));
  expect(s.checks).toHaveLength(n);
  s.store.onOut(OUT({ changed: true, meaning: true }));
  expect(s.checks).toHaveLength(n + 1);
});

test("an edit during a check reruns once; a stale response is ignored", async () => {
  const s = setup();
  await s.store.load();
  expect(s.checks).toHaveLength(1);
  // three edits while the first check runs: one rerun, not three
  for (let i = 0; i < 3; i++) s.store.onOut(OUT({ changed: true, meaning: true }));
  expect(s.checks).toHaveLength(1);
  const finding = { ruleId: "layer", severity: "error" as const, message: "api must not call ui", tool: "ig", file: "", line: 0, fingerprint: "x", target: "fn-a" };
  s.checks[0].d.resolve(result(s.checks[0].seq, [finding]));
  await settle();
  expect(s.of("setCheck")).toHaveLength(0); // stale: an edit came after it was sent
  expect(s.store.check).toBeNull();
  expect(s.checks).toHaveLength(2);
  s.checks[1].d.resolve(result(s.checks[1].seq, [finding]));
  await settle();
  expect(s.of("setCheck")).toEqual([[[["fn-a", "error", "api must not call ui"]]]]);
  expect(s.store.check?.seq).toBe(s.checks[1].seq);
  expect(s.checks).toHaveLength(2);
  // a response with an older seq never wins
  s.store.onOut(OUT({ meaning: true, changed: true }));
  s.checks[2].d.resolve(result(s.checks[1].seq));
  await settle();
  expect(s.of("setCheck")).toHaveLength(1);
});

test("flush writes immediately", async () => {
  const s = setup();
  await s.store.load();
  s.store.onOut(OUT({ changed: true }));
  await s.store.flush();
  expect(s.writes).toHaveLength(1);
  await s.advance(1000);
  expect(s.writes).toHaveLength(1); // the timer found nothing left to save
});

test("external valid change goes through engine.external", async () => {
  const turns = () => [{ id: "turn-1", start: 0, end: 1000 }];
  const s = setup({ turns });
  await s.store.load();
  s.at(1500);
  await s.store.external();
  expect(s.of("external")).toEqual([[design(), "turn-1"]]);
  s.at(5000);
  await s.store.external();
  expect(s.of("external")[1][1]).toBeNull();
  expect(s.of("load")).toHaveLength(1);
});

test("a write conflict merges then retries", async () => {
  const s = setup();
  await s.store.load();
  let first = true;
  s.failWrites(async () => {
    if (first) {
      first = false;
      throw "conflict"; // Tauri rejects with a plain string
    }
  });
  s.store.onOut(OUT({ changed: true }));
  await s.store.flush();
  await settle();
  expect(s.of("external")).toHaveLength(1);
  expect(s.writes).toHaveLength(2);
  expect(s.of("saved")).toHaveLength(1);
  expect(s.store.saveError).toBeNull();
});

test("invalid file sets errors and readOnly and never writes it", async () => {
  let disk: Design | Error = new Error("plans/export/design.json: unsupported version 2");
  const s = setup({ disk: () => disk });
  await s.store.load();
  const path = "plans/export/design.json";
  expect(s.store.errors[path]).toContain("unsupported version 2");
  expect(s.store.readOnly.has(path)).toBe(true);
  s.store.onOut(OUT({ changed: true }));
  await s.advance(1000);
  await s.store.flush();
  expect(s.writes).toHaveLength(0);
  // fixed on disk: it loads and is writable again
  disk = design();
  await s.store.external();
  expect(s.store.readOnly.has(path)).toBe(false);
  expect(s.store.errors[path]).toBeUndefined();
  expect(s.of("load")).toHaveLength(2);
  // broken again from outside after loading: never written over
  disk = new Error("bad json");
  await s.store.external();
  s.store.onOut(OUT({ changed: true }));
  await s.advance(1000);
  expect(s.writes).toHaveLength(0);
});

test("a failed write keeps the board and retries", async () => {
  const s = setup();
  await s.store.load();
  s.failWrites(() => Promise.reject(new Error("disk full")));
  s.store.onOut(OUT({ changed: true }));
  await s.advance(300);
  expect(s.writes).toHaveLength(1);
  expect(s.store.saveError).toBe("disk full");
  expect(s.of("saved")).toHaveLength(0);
  expect(s.of("load")).toHaveLength(1); // the board on screen is untouched
  s.failWrites(null);
  await s.advance(2000);
  expect(s.writes).toHaveLength(2);
  expect(s.store.saveError).toBeNull();
  expect(s.of("saved")).toHaveLength(1);
});

test("placed fills the proposed file in one op", async () => {
  const s = setup({ board: design([{ id: "fn-function", kind: "function", name: "Function", layer: "api", x: 0, y: 0 }]) });
  await s.store.load();
  s.store.onOut(OUT({ changed: true, meaning: true, placed: { id: "fn-function", layer: "api" } }));
  expect(s.of("ops")).toEqual([[[{ op: "update", id: "fn-function", set: { file: "server/api/function.ts" } }], "user"]]);
  // the first rename follows the proposal while the file is still the proposed one
  s.set(design([{ id: "fn-function", kind: "function", name: "Export patients", layer: "api", file: "server/api/function.ts", x: 0, y: 0 }]));
  s.store.onOut(OUT({ changed: true, meaning: true }));
  expect(s.of("ops")[1][0]).toEqual([{ op: "update", id: "fn-function", set: { file: "server/api/export-patients.ts" } }]);
});
