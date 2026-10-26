// The chat reducer over the fake agent's recorded `tour` (crates/ig-desktop record_chat_fixture).
import { expect, test } from "vitest";
import type { AgentEvent } from "../src/api";
import { chatReducer, completions, initialChat, type ChatAction, type ChatState } from "../src/model/chat";
import events from "./fixtures/agent-events.json";

const recorded = events as AgentEvent[];
const play = (as: ChatAction[], s: ChatState = initialChat) => as.reduce(chatReducer, s);
const upTo = (type: string) => recorded.slice(0, recorded.findIndex((e) => e.type === type) + 1);

test("chunks concatenate into one agent message", () => {
  const s = play(recorded);
  const texts = s.items.filter((i) => i.kind === "text");
  expect(texts[0]).toMatchObject({ role: "agent", text: "Here is **the plan**:\n\n- one\n- two\n" });
  expect(s.title).toBe("Tour");
});

test("tool updates merge into the call by id", () => {
  const s = play(recorded);
  const tools = s.items.filter((i) => i.kind === "tool");
  expect(tools).toHaveLength(1);
  const call = tools[0].kind === "tool" && tools[0].call;
  expect(call).toMatchObject({ toolCallId: "a", title: "Write a.txt", kind: "edit", status: "failed" });
  expect((call as { content: unknown[] }).content[0]).toMatchObject({ type: "diff", path: "/p/a.txt", newText: "hi\n" });
});

test("a permission request becomes a pending card, and a reject resolves it", () => {
  let s = play(upTo("Permission"));
  const card = s.items.at(-1)!;
  expect(card).toMatchObject({ kind: "permission", requestId: 1, answered: null });
  expect(card.kind === "permission" && card.options.map((o) => o.name)).toEqual(["Yes", "Yes, allow all edits during this session", "No"]);
  expect(s.pending).toBe(1);
  s = chatReducer(s, { type: "Answered", requestId: 1, answer: "reject" });
  expect(s.items.at(-1)).toMatchObject({ kind: "permission", answered: "reject" });
  expect(s.pending).toBe(0);
  // Answered once: a second answer changes nothing.
  expect(chatReducer(s, { type: "Answered", requestId: 1, answer: "allow-once" })).toBe(s);
});

test("a question becomes option buttons; usage, commands and config arrive", () => {
  const s = play(recorded);
  const q = s.items.find((i) => i.kind === "question")!;
  expect(q).toMatchObject({ message: "How big?", answered: null });
  expect(q.kind === "question" && q.questions).toEqual([
    { field: "question_0", title: "Scope", multi: false, custom: "question_0_custom",
      options: [{ value: "Small", title: "Small", description: "One module" }, { value: "Large", title: "Large" }] },
  ]);
  expect(s.usage).toMatchObject({ used: 31639, size: 1000000, cost: { amount: 0.16, currency: "USD" }, rateLimit: { status: "allowed", utilization: 0.42 } });
  expect(s.config.map((c) => [c.id, c.currentValue])).toEqual([["mode", "default"], ["model", "default"]]);
  expect(completions(s.commands, "/").map((c) => c.name)).toEqual(["ig:explore", "ig:plan", "compact"]);
  expect(completions(s.commands, "/ig:p").map((c) => c.name)).toEqual(["ig:plan"]);
  expect(completions(s.commands, "/ig:plan x")).toEqual([]);
});

test("a turn: the user's text, running until it ends; a stop keeps the transcript and closes cards", () => {
  let s = chatReducer(initialChat, { type: "Sent", text: "hi" });
  expect(s.items).toEqual([{ kind: "text", role: "user", text: "hi" }]);
  expect(s.running).toBe(true);
  s = play(upTo("Permission"), s);
  s = chatReducer(s, { type: "Stopped", reason: "the agent stopped (exit 3)", stderr: "boom" });
  expect(s.stopped).toEqual({ reason: "the agent stopped (exit 3)", stderr: "boom" });
  expect(s.running).toBe(false);
  expect(s.pending).toBe(0);
  expect(s.items.find((i) => i.kind === "permission")).toMatchObject({ answered: "stopped" });
  expect(s.items[0]).toEqual({ kind: "text", role: "user", text: "hi" });
  s = chatReducer(s, { type: "TurnEnded", stopReason: "cancelled" });
  expect(s.running).toBe(false);
});

test("a session switch starts over; auth and setup are screens", () => {
  let s = play(recorded);
  s = chatReducer(chatReducer(s, { type: "Reset" }), { type: "Started", sessionId: "s2" });
  expect(s).toMatchObject({ sessionId: "s2", items: [], title: null, usage: null });
  expect(s.commands).toEqual([]);
  s = chatReducer(s, { type: "Auth", needed: true, label: null });
  expect(s.login).toBe(true);
  s = chatReducer(s, { type: "Setup", instructions: "npm install x", installable: true });
  expect(s.setup).toEqual({ instructions: "npm install x", installable: true });
});
