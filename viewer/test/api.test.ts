// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";

const SAMPLE = { nodes: [], root: "/r" };

function page(data: string, live = "/*IG_LIVE*/") {
  document.body.innerHTML = `<script id="ig-data" type="application/json">${data}</script><script id="ig-live" type="application/json">${live}</script>`;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
  vi.resetModules();
  delete (window as any).__TAURI_INTERNALS__;
  document.body.innerHTML = "";
});

test("pickApi: embedded when the page has inline data", async () => {
  page(JSON.stringify(SAMPLE));
  const { pickApi } = await import("../src/api");
  const api = pickApi();
  expect(await api.viewData()).toEqual(SAMPLE);
  expect(api.notes).toBeUndefined();
});

test("pickApi: live pages get notes and fetch /api/view once rebuilt", async () => {
  page(JSON.stringify(SAMPLE), '{"token":"t"}');
  const f = vi.fn(async () => ({ ok: true, json: async () => ({ root: "/new" }) }));
  vi.stubGlobal("fetch", f);
  const { pickApi } = await import("../src/api");
  const api = pickApi();
  expect(api.notes).toBeDefined();
  expect(await api.viewData()).toEqual(SAMPLE);
  expect(await api.viewData()).toEqual({ root: "/new" });
  expect(f).toHaveBeenCalledWith("/api/view");
});

test("pickApi: tauri when __TAURI_INTERNALS__ exists, else http", async () => {
  page("/*IG_DATA*/");
  vi.stubEnv("DEV", false);
  const f = vi.fn(async () => ({ ok: true, json: async () => SAMPLE }));
  vi.stubGlobal("fetch", f);
  const { pickApi } = await import("../src/api");
  expect(await pickApi().viewData()).toEqual(SAMPLE);
  expect(f).toHaveBeenCalledWith("/api/view");
  (window as any).__TAURI_INTERNALS__ = {};
  const invoke = vi.fn(async () => SAMPLE);
  const listen = vi.fn(async () => () => {});
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  vi.resetModules();
  const t = (await import("../src/api")).pickApi();
  expect(await t.viewData({ plan: "p.md" })).toEqual(SAMPLE);
  expect(invoke).toHaveBeenCalledWith("view_data", { plan: "p.md", noPlan: false });
});

test("http: a failed /api/view throws; onRebuild shares one EventSource and closes it", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 500, json: async () => ({ error: "boom" }) })));
  const made: any[] = [];
  class ES {
    listeners: Record<string, (e: unknown) => void> = {};
    close = vi.fn();
    constructor(public url: string) { made.push(this); }
    addEventListener(n: string, cb: (e: unknown) => void) { this.listeners[n] = cb; }
  }
  vi.stubGlobal("EventSource", ES);
  const { http } = await import("../src/api/http");
  const api = http();
  await expect(api.viewData()).rejects.toThrow("boom");
  const a = vi.fn(), b = vi.fn();
  const ua = api.onRebuild(a), ub = api.onRebuild(b);
  expect(made).toHaveLength(1);
  expect(made[0].url).toBe("/api/events");
  made[0].listeners.rebuild({});
  expect(a).toHaveBeenCalled();
  expect(b).toHaveBeenCalled();
  ua();
  expect(made[0].close).not.toHaveBeenCalled();
  ub();
  expect(made[0].close).toHaveBeenCalled();
});

test("tauri: viewData invokes view_data; onRebuild listens to rebuild and unlistens (even if called early)", async () => {
  const invoke = vi.fn(async () => SAMPLE);
  const un = vi.fn();
  let handler: (e: { payload: unknown }) => void = () => {};
  const listen = vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => ((handler = cb), un));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  const { tauri } = await import("../src/api/tauri");
  const api = tauri();
  expect(await api.viewData()).toEqual(SAMPLE);
  expect(invoke).toHaveBeenCalledWith("view_data", { plan: undefined, noPlan: false });
  await api.fileTree!("apps");
  expect(invoke).toHaveBeenCalledWith("file_tree", { dir: "apps" });
  await api.readFile!("a.ts");
  expect(invoke).toHaveBeenCalledWith("read_file", { path: "a.ts" });
  const cb = vi.fn();
  const off = api.onRebuild(cb);
  await vi.waitFor(() => expect(listen).toHaveBeenCalledWith("rebuild", expect.any(Function)));
  handler({ payload: null });
  expect(cb).toHaveBeenCalled();
  off();
  await vi.waitFor(() => expect(un).toHaveBeenCalled());
  const early = tauri().onRebuild(cb);
  early();
  await vi.waitFor(() => expect(un).toHaveBeenCalledTimes(2));
});

test("tauri agent: commands carry their arguments; onEvent hands over agent-event payloads", async () => {
  const invoke = vi.fn(async () => "id");
  let handler: (e: { payload: unknown }) => void = () => {};
  const listen = vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => ((handler = cb), () => {}));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  vi.resetModules();
  const { tauri } = await import("../src/api/tauri");
  const agent = tauri().agent!;
  expect(await agent.start("login")).toBe("id");
  expect(invoke).toHaveBeenCalledWith("agent_start", { feature: "login" });
  await agent.prompt("hi");
  expect(invoke).toHaveBeenCalledWith("agent_prompt", { text: "hi", attachments: [] });
  await agent.answer(3, "reject");
  expect(invoke).toHaveBeenCalledWith("agent_answer", { requestId: 3, optionId: "reject" });
  const cb = vi.fn();
  agent.onEvent(cb);
  await vi.waitFor(() => expect(listen).toHaveBeenCalledWith("agent-event", expect.any(Function)));
  const e = { type: "Stopped", reason: "exit 1", stderr: null };
  handler({ payload: e });
  expect(cb).toHaveBeenCalledWith(e);
});

test("tauri design api invokes the commands", async () => {
  const invoke = vi.fn(async () => []);
  let handler: (e: { payload: unknown }) => void = () => {};
  const listen = vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => ((handler = cb), () => {}));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  const { tauri } = await import("../src/api/tauri");
  const d = tauri().design!;
  await d.features();
  expect(invoke).toHaveBeenCalledWith("features");
  await d.create("Patient export");
  expect(invoke).toHaveBeenCalledWith("create_feature", { title: "Patient export" });
  await d.read("a");
  expect(invoke).toHaveBeenCalledWith("read_design", { slug: "a" });
  const doc = { version: 1, title: "A", scope: { kind: "dir", id: "." }, elements: [], marks: [], links: [], bridges: [] } as any;
  await d.write("a", doc);
  expect(invoke).toHaveBeenCalledWith("write_design", { slug: "a", design: doc });
  const cb = vi.fn();
  d.onChanged(cb);
  await vi.waitFor(() => expect(listen).toHaveBeenCalledWith("design-changed", expect.any(Function)));
  handler({ payload: { slug: "a" } });
  expect(cb).toHaveBeenCalledWith("a");
});

test("on quit every flush finishes before the ack, even when one fails", async () => {
  const { flushThenAck } = await import("../src/api/tauri");
  const log: string[] = [];
  const slow = () => new Promise<void>((r) => setTimeout(() => (log.push("slow"), r()), 10));
  const bad = () => Promise.reject(new Error("disk full"));
  const err = vi.spyOn(console, "error").mockImplementation(() => {});
  await flushThenAck([slow, bad], async () => void log.push("ack"));
  expect(log).toEqual(["slow", "ack"]);
  await flushThenAck([], async () => void log.push("ack"));
  expect(log).toEqual(["slow", "ack", "ack"]);
  err.mockRestore();
});

test("a flush that never ends holds the quit only until the bound", async () => {
  const { flushThenAck } = await import("../src/api/tauri");
  let acked = false;
  await flushThenAck([() => new Promise(() => {})], async () => void (acked = true), 20);
  expect(acked).toBe(true);
});

test("tauri screens: commands, base64 of a big image, events", async () => {
  (window as any).__TAURI_INTERNALS__ = {};
  const invoke = vi.fn(async (c: string) => (c === "read_asset" ? new Uint8Array([1, 2]).buffer : "0123456789ab.png"));
  const listeners: Record<string, (e: { payload: unknown }) => void> = {};
  const listen = vi.fn(async (n: string, cb: (e: { payload: unknown }) => void) => ((listeners[n] = cb), () => {}));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  vi.doMock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ onCloseRequested: async () => {} }) }));
  const { tauri, toBase64 } = await import("../src/api/tauri");
  const big = new Uint8Array(200_000).fill(65);
  expect(atob(toBase64(big))).toBe("A".repeat(200_000));
  const s = tauri().screens!;
  expect(await s.putAsset("f", new Uint8Array([104, 105]))).toBe("0123456789ab.png");
  expect(invoke).toHaveBeenCalledWith("put_asset", { slug: "f", base64: "aGk=" });
  expect(Array.from(await s.readAsset("f", "0123456789ab.png"))).toEqual([1, 2]);
  const seen = vi.fn();
  s.onScreenChanged(seen);
  await Promise.resolve();
  listeners["screen-changed"]({ payload: { slug: "f", id: "home" } });
  expect(seen).toHaveBeenCalledWith("f", "home");
});
