// @vitest-environment happy-dom
import { expect, test } from "vitest";
import type { AgentApi, AgentEvent, TrustState } from "../src/api";
import { ChatSession } from "../src/components/chat/session.svelte";
import { planPrompt } from "../src/board/outbox";

/** A fake agent: records starts and prompts; a prompt resolves at once. */
function fake() {
  const calls: string[] = [];
  let emit: (e: AgentEvent) => void = () => {};
  const agent: AgentApi = {
    start: async (f) => (calls.push("start " + f), "s1"),
    prompt: async (t) => (calls.push("prompt " + t), "end_turn"),
    cancel: async () => {}, answer: async () => {}, reply: async () => {}, setConfig: async () => {},
    list: async () => ({ sessions: [], features: {} }), load: async (id) => void calls.push("load " + id),
    restart: async () => "s2", install: async () => {},
    onEvent: (cb) => ((emit = cb), () => {}),
  };
  let trusted = false;
  const inv = async () => ({ trusted, hash: "h" }) as TrustState;
  const trust = async () => void (trusted = true);
  return { agent, calls, inv, trust, emit: (e: AgentEvent) => emit(e) };
}

test("plan on an untrusted project sends once after trust", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  let started = 0;
  await c.ask("patient-export", planPrompt("patient-export"), () => started++);
  expect(c.trust).not.toBeNull();
  expect(f.calls).toEqual([]);
  await c.onTrust("h");
  await new Promise((r) => setTimeout(r, 0));
  expect(f.calls).toEqual(["start patient-export", "prompt /ig:plan plans/patient-export/design.json"]);
  expect(started).toBe(1);
  expect(c.feature).toBe("patient-export");
  await c.start();
  await new Promise((r) => setTimeout(r, 0));
  expect(f.calls.filter((x) => x.startsWith("prompt")).length).toBe(1);
});

test("a declined trust card shows the prompt as not sent", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("f", planPrompt("f"));
  c.decline();
  expect(c.s.items.at(-1)).toEqual({ kind: "error", text: "Not sent: /ig:plan plans/f/design.json" });
  await c.onTrust("h");
  expect(f.calls.filter((x) => x.startsWith("prompt"))).toEqual([]);
});

test("a prompt during a running turn waits for its end", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await f.trust();
  await c.ask("f", "first");
  c.dispatch({ type: "Sent", text: "typed" }); // a turn the user started
  const p = c.ask("f", "second");
  await new Promise((r) => setTimeout(r, 0));
  expect(f.calls.at(-1)).toBe("prompt first");
  c.dispatch({ type: "TurnEnded", stopReason: "end_turn" });
  await p;
  expect(f.calls.at(-1)).toBe("prompt second");
});
