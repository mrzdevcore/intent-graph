// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";

afterEach(() => {
  vi.resetModules();
  delete (window as any).__TAURI_INTERNALS__;
});

test("pickApi: outside the desktop app, only the embedded view", async () => {
  document.body.innerHTML = '<script id="ig-data" type="application/json">{"version":1}</script>';
  const { pickApi } = await import("../src/api");
  const api = pickApi();
  expect(Object.keys(api)).toEqual(["view", "shown", "diff"]);
  expect(await api.view!()).toEqual({ version: 1 });
  expect(api.shown!()).toBeNull();
});

test("embedded: the page's answer is read once, and a broken one is none", async () => {
  document.body.innerHTML = '<script id="ig-show" type="application/json">{"seq":3,"title":"t","nodes":["a"]}</script>';
  const { pickApi } = await import("../src/api");
  expect(pickApi().shown!()).toMatchObject({ title: "t", nodes: ["a"], seq: 3 });
  document.body.innerHTML = '<script id="ig-show" type="application/json">/*IG_SHOW*/</script>';
  expect(pickApi().shown!()).toBeNull();
});

test("tauri onShow hands over a parsed show, and drops a bad one", async () => {
  (window as any).__TAURI_INTERNALS__ = {};
  const handlers: Record<string, (e: { payload: unknown }) => void> = {};
  vi.doMock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (n: string, cb: (e: { payload: unknown }) => void) => ((handlers[n] = cb), () => {})) }));
  const { pickApi } = await import("../src/api");
  const got: unknown[] = [];
  pickApi().onShow!((s) => got.push(s));
  handlers.show({ payload: { version: 1, seq: 7, title: "t", nodes: ["a"], edges: [["a", "b"]], steps: [], mode: "focus" } });
  handlers.show({ payload: { title: "no nodes" } });
  expect(got).toMatchObject([{ seq: 7, title: "t", edges: [["a", "b"]] }]);
});

test("tauri agent: commands carry their arguments; onEvent hands over agent-event payloads", async () => {
  (window as any).__TAURI_INTERNALS__ = {};
  const invoke = vi.fn(async () => "id");
  let handler: (e: { payload: unknown }) => void = () => {};
  const listen = vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => ((handler = cb), () => {}));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  const { pickApi } = await import("../src/api");
  const api = pickApi();
  const agent = api.agent!;
  expect(await agent.start("login")).toBe("id");
  expect(invoke).toHaveBeenCalledWith("agent_start", { feature: "login" });
  await agent.prompt("hi");
  expect(invoke).toHaveBeenCalledWith("agent_prompt", { text: "hi", attachments: [] });
  await agent.answer(3, "reject");
  expect(invoke).toHaveBeenCalledWith("agent_answer", { requestId: 3, optionId: "reject" });
  await api.trustInventory!();
  expect(invoke).toHaveBeenCalledWith("trust_inventory");
  const cb = vi.fn();
  agent.onEvent(cb);
  await vi.waitFor(() => expect(listen).toHaveBeenCalledWith("agent-event", expect.any(Function)));
  const e = { type: "Stopped", reason: "exit 1", stderr: null };
  handler({ payload: e });
  expect(cb).toHaveBeenCalledWith(e);
});

test("tauri: a quit is acked at once (nothing to flush)", async () => {
  (window as any).__TAURI_INTERNALS__ = {};
  const invoke = vi.fn(async () => {});
  const listeners: Record<string, (e: { payload: unknown }) => void> = {};
  const listen = vi.fn(async (n: string, cb: (e: { payload: unknown }) => void) => ((listeners[n] = cb), () => {}));
  vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
  vi.doMock("@tauri-apps/api/event", () => ({ listen }));
  const { pickApi } = await import("../src/api");
  pickApi();
  await vi.waitFor(() => expect(listeners["flush-before-exit"]).toBeDefined());
  listeners["flush-before-exit"]({ payload: null });
  expect(invoke).toHaveBeenCalledWith("exit_flushed");
});
