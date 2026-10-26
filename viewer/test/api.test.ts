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
  expect(invoke).toHaveBeenCalledWith("view_data", { plan: "p.md", noPlan: undefined });
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
  let handler: () => void = () => {};
  const listen = vi.fn(async (_n: string, cb: () => void) => ((handler = cb), un));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  const { tauri } = await import("../src/api/tauri");
  const api = tauri();
  expect(await api.viewData()).toEqual(SAMPLE);
  expect(invoke).toHaveBeenCalledWith("view_data", { plan: undefined, noPlan: undefined });
  const cb = vi.fn();
  const off = api.onRebuild(cb);
  await vi.waitFor(() => expect(listen).toHaveBeenCalledWith("rebuild", expect.any(Function)));
  handler();
  expect(cb).toHaveBeenCalled();
  off();
  await vi.waitFor(() => expect(un).toHaveBeenCalled());
  const early = tauri().onRebuild(cb);
  early();
  await vi.waitFor(() => expect(un).toHaveBeenCalledTimes(2));
});
