// @vitest-environment happy-dom
import { afterEach, expect, test, vi } from "vitest";

afterEach(() => {
  vi.resetModules();
  delete (window as any).__TAURI_INTERNALS__;
});

test("pickApi: no host outside the desktop app", async () => {
  const { pickApi } = await import("../src/api");
  expect(pickApi()).toEqual({});
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
