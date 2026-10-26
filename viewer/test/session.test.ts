// @vitest-environment happy-dom
import { expect, test } from "vitest";
import type { AgentApi, TrustState } from "../src/api";
import { ChatSession } from "../src/components/chat/session.svelte";
import { Outbox, planPrompt } from "../src/board/outbox";
import { realClock } from "../src/board/store";

/** A fake agent: records starts, loads and prompts; a prompt resolves at once. */
function fake(opts: { trusted?: boolean; failStart?: string; setup?: boolean } = {}) {
  const calls: string[] = [];
  let n = 0;
  const agent: AgentApi = {
    start: async (f) => {
      if (opts.failStart) throw new Error(opts.failStart);
      calls.push("start " + f);
      return "s" + ++n;
    },
    prompt: async (t) => (calls.push("prompt " + t), "end_turn"),
    cancel: async () => {}, answer: async () => {}, reply: async () => {}, setConfig: async () => {},
    list: async () => ({ sessions: [], features: {} }), load: async (id) => void calls.push("load " + id),
    restart: async () => "r", install: async () => {},
    onEvent: () => () => {},
  };
  let trusted = !!opts.trusted;
  const inv = async () => ({ trusted, hash: "h" }) as TrustState;
  const trust = async () => void (trusted = true);
  return { agent, calls, inv, trust, prompts: () => calls.filter((x) => x.startsWith("prompt")) };
}
const tick = () => new Promise((r) => setTimeout(r, 0));
const notices = (c: ChatSession) => c.s.items.flatMap((i) => (i.kind === "error" ? [i.text] : []));

test("plan on an untrusted project sends once after trust", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("patient-export", planPrompt("patient-export"));
  expect(c.trust).not.toBeNull();
  expect(c.held?.text).toBe("/ig:plan plans/patient-export/design.json"); // the trust card shows it
  expect(f.calls).toEqual([]);
  await c.onTrust("h");
  await tick();
  expect(f.calls).toEqual(["start patient-export", "prompt /ig:plan plans/patient-export/design.json"]);
  expect(c.feature).toBe("patient-export");
  expect(c.held).toBeNull();
  await c.start();
  await tick();
  expect(f.prompts().length).toBe(1);
});

test("a declined trust card shows the prompt as not sent", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("f", planPrompt("f"));
  c.decline();
  expect(notices(c)).toEqual(["Not sent: /ig:plan plans/f/design.json"]);
  await c.onTrust("h");
  expect(f.prompts()).toEqual([]);
});

test("another session started or loaded meanwhile drops the held prompt, visibly", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("f", "one");
  await f.trust();
  await c.start(); // the user's own new session, not f's
  await tick();
  expect(notices(c)).toEqual(["Not sent: one"]);
  await c.ask("f", "two");
  await tick();
  expect(f.prompts()).toEqual(["prompt two"]);
  // held for another feature, then the user loads some other session
  c.held = { feature: "g", text: "three" };
  await c.load("s1");
  expect(notices(c)).toEqual(["Not sent: three"]); // in the loaded session's transcript
});

test("a second prompt replaces the held one; an error drops it", async () => {
  const f = fake();
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("f", "first");
  await c.ask("f", "second");
  expect(notices(c)).toEqual(["Not sent: first"]);
  expect(c.held?.text).toBe("second");
  const g = fake({ trusted: true, failStart: "boom" });
  const d = new ChatSession(g.agent, g.inv, g.trust);
  await d.ask("f", "x");
  expect(d.held).toBeNull();
  expect(notices(d)).toEqual(["Not sent: x"]);
});

test("the setup screen keeps the held prompt for Retry", async () => {
  const f = fake({ trusted: true, failStart: "no adapter" });
  const c = new ChatSession(f.agent, f.inv, f.trust);
  const start = f.agent.start;
  f.agent.start = (x) => (c.dispatch({ type: "Setup", instructions: "install", installable: true }), start(x));
  await c.ask("f", "go");
  expect(c.held?.text).toBe("go");
  const ok = fake({ trusted: true });
  (c as unknown as { agent: AgentApi }).agent = ok.agent; // the adapter got installed
  await c.retry();
  await tick();
  expect(ok.calls).toEqual(["start f", "prompt go"]);
});

test("a prompt during a running turn waits for its end; a stop drops it", async () => {
  const f = fake({ trusted: true });
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.ask("f", "first");
  await tick();
  c.dispatch({ type: "Sent", text: "typed" }); // a turn the user started
  const p = c.ask("f", "second");
  await tick();
  expect(f.calls.at(-1)).toBe("prompt first");
  c.dispatch({ type: "TurnEnded", stopReason: "end_turn" });
  await p;
  expect(f.calls.at(-1)).toBe("prompt second");
  c.dispatch({ type: "Sent", text: "typed" });
  const q = c.ask("f", "third");
  c.dispatch({ type: "Stopped", reason: "crashed", stderr: null });
  await q;
  expect(f.calls.at(-1)).toBe("prompt second");
  expect(notices(c)).toContain("Not sent: third");
});

test("board edits wait for the feature's session, then go", async () => {
  const f = fake({ trusted: true });
  const c = new ChatSession(f.agent, f.inv, f.trust);
  const sent: string[] = [];
  const o = new Outbox("f", async (t) => void sent.push(t), () => false, (_a, b) => (b === "1" ? ["added a"] : []), realClock);
  c.outboxes.set("f", o);
  o.baseline("0");
  await c.start(); // a session without a feature
  o.push("1");
  expect(o.pending).toBe(1);
  c.dispatch({ type: "Started", sessionId: "s1" });
  expect(sent).toEqual([]);
  c.features = { f: "s1" };
  c.dispatch({ type: "Info", sessionId: "s1", title: null }); // any event syncs the outboxes
  expect(sent.length).toBe(1);
});

test("opening the board loads the feature's session when the chat has nothing going", async () => {
  const f = fake({ trusted: true });
  f.agent.list = async () => ({ sessions: [], features: { f: "s9" } });
  const c = new ChatSession(f.agent, f.inv, f.trust);
  await c.start(); // the chat's own fresh session: empty, no feature
  await c.adopt("f");
  expect(f.calls).toEqual(["start undefined", "load s9"]);
  expect(c.feature).toBe("f");
  c.dispatch({ type: "Sent", text: "hi" });
  c.dispatch({ type: "TurnEnded", stopReason: "end_turn" });
  await c.adopt("g"); // a conversation is going: left alone
  expect(f.calls.length).toBe(2);
});
