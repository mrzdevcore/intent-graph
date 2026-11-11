// @vitest-environment happy-dom
import { expect, test } from "vitest";
import type { AgentApi, TrustState } from "../src/api";
import { ChatSession } from "../src/components/chat/session.svelte";
import { EditSession, type Ctx } from "../src/map/edit/session.svelte";
import type { Edit, MapBox, View } from "../src/map/types";

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
  await c.ask("patient-export", "/ig:plan plans/patient-export/design.json");
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
  await c.ask("f", "/ig:plan plans/f/design.json");
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

// --- the map's edit session

function editing(boxes: Record<string, string>, opts: { busy?: boolean; arrowKinds?: View["arrowKinds"] } = {}) {
  const sent: Edit[] = [];
  const box = (id: string) => (boxes[id] ? ({ id, kind: { value: boxes[id] }, title: { value: id } } as unknown as MapBox) : undefined);
  const ctx: Ctx = {
    edit: () => ({ apply: async (e: Edit) => (sent.push(e), {} as View) }) as never, box, frame: () => ({ x: 0, y: 0, w: 10, h: 10 }),
    kinds: () => [{ name: "ledger", base: "store", custom: true } as never], arrowKinds: () => opts.arrowKinds,
    cam: () => ({ x: 0, y: 0, k: 1 }), stage: () => document.body, selected: () => null, select: () => {}, tool: () => {}, used: () => {},
    run: (p) => (opts.busy ? false : (p(), true)), arrange: () => {}, drop: () => {}, draftFrame: () => undefined,
  };
  return { s: new EditSession(ctx), sent };
}

test("connect draws the first box to the second with the kind guessed from both ends' bases", async () => {
  const { s, sent } = editing({ api: "service", books: "ledger", web: "app" });
  s.connect("api", "books");
  await tick();
  expect(sent).toEqual([{ op: "arrow", from: "api", to: "books", kind: "writes", label: null }]);
  expect(s.made).toEqual({ from: "api", to: "books", kind: "writes" });
  s.connect("web", "api");
  s.connect("web", "web");
  s.connect("web", "nope");
  await tick();
  expect(sent.slice(1)).toEqual([{ op: "arrow", from: "web", to: "api", kind: "calls", label: null }]);
});

test("connect while an edit is saving shows no chip; a draft at an arrow's end confirms as add and a uses arrow", () => {
  const busy = editing({ api: "service", web: "app" }, { busy: true });
  busy.s.connect("web", "api");
  expect(busy.s.made).toBeNull();
  const { s } = editing({ web: "app" });
  const d = { parent: "system", x: 0, y: 0, w: 240, h: 144, title: "Ledger", drawing: false };
  expect(s.confirm(d, "ledger", "Ledger")).toEqual({ op: "add", parent: "system", kind: "box", id: "ledger", title: "Ledger", at: [], before: null });
  expect(s.confirm({ ...d, from: "web" }, "ledger", "Ledger")).toEqual({ op: "batch", edits: [
    { op: "add", parent: "system", kind: "box", id: "ledger", title: "Ledger", at: [], before: null },
    { op: "arrow", from: "web", to: "ledger", kind: "uses", label: null },
  ] });
});
