// The chat reducer over the fake agent's recorded `tour` (crates/ig-desktop record_chat_fixture).
import { expect, test } from "vitest";
import type { AgentEvent, ToolCallInfo } from "../src/api";
import { chatReducer, completions, openQuestions, controls, fastNext, fastOn, groupTools, initialChat, shortCommand, type ChatAction, type ChatState, type Item } from "../src/model/chat";
import chat from "../src/components/chat/Chat.svelte?raw";
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

test("the live region gets the turn's whole reply at its end, never replayed history", () => {
  const chunk = (text: string): ChatAction => ({ type: "MessageChunk", sessionId: "s", role: "agent", content: { type: "text", text } });
  let s = play([chunk("old "), chunk("history")]);
  expect(s.live).toBe("");
  s = play([{ type: "Sent", text: "hi" }, chunk("Hel"), chunk("lo")], s);
  expect(s.live).toBe("");
  s = chatReducer(s, { type: "TurnEnded", stopReason: "end_turn" });
  expect(s.live).toBe("Hello");
});

test("the live region is not part of the copyable transcript", () => {
  // Before the header, outside the transcript: a selection of the messages never reaches it.
  expect(chat.indexOf('aria-live="polite"')).toBeLessThan(chat.indexOf('<header class="chat-head">'));
  expect(chat).toContain('<div class="sr-only" aria-live="polite">'); // .sr-only: user-select none (app.css)
});

test("consecutive tool calls of one kind group, each keeping its status", () => {
  type St = ToolCallInfo["status"];
  const call = (id: string, kind: string, status: St = "completed"): Item => ({ kind: "tool", call: { toolCallId: id, kind, status } });
  const rows = groupTools([
    call("a", "execute"), call("b", "execute", "failed"), call("c", "execute"),
    call("d", "read"),
    { kind: "text", role: "agent", text: "x" },
    call("e", "read"), call("f", "read", "in_progress"),
  ]);
  expect(rows.map((r) => (r.kind === "group" ? r.label : r.kind))).toEqual(["Ran 3 commands", "tool", "text", "Read 2 files"]);
  const [g, , , h] = rows as Extract<(typeof rows)[number], { kind: "group" }>[];
  expect(g.calls.map((c) => c.status)).toEqual(["completed", "failed", "completed"]);
  expect([g.status, h.status]).toEqual(["failed", "in_progress"]);
});

test("commands drop a leading cd to the project root and show the root as .", () => {
  const root = "/Users/me/p/front";
  expect(shortCommand("cd /Users/me/p/front && grep -rn home /Users/me/p/front/core", root)).toBe("grep -rn home ./core");
  expect(shortCommand('cd "/Users/me/p/front" && ls', root)).toBe("ls");
  expect(shortCommand("ls /Users/me/p/front", root)).toBe("ls .");
  expect(shortCommand("ls /Users/me/p/frontend", root)).toBe("ls /Users/me/p/frontend");
  expect(shortCommand("cd /Users/me/p/front/core && ls", root)).toBe("cd ./core && ls");
});

test("agent controls: mode and model as chips, fast mode as a toggle, the rest in the menu", () => {
  const opt = (id: string, category: string | undefined, values: string[], current = values[0]) => ({
    id, name: id, category, type: "select", currentValue: current, options: values.map((v) => ({ value: v, name: v })),
  });
  const c = controls([
    opt("model", "model", ["default", "opus"]), opt("mode", "mode", ["default", "bypassPermissions"]),
    opt("effort", "thought_level", ["low", "high"]), opt("fast_mode", undefined, ["off", "on"]),
    opt("agent", undefined, ["default", "planner"]),
  ]);
  expect([c.mode?.id, c.model?.id, c.fast?.id, c.more.map((o) => o.id)]).toEqual(["mode", "model", "fast_mode", ["effort", "agent"]]);
  expect([fastOn(c.fast!), fastNext(c.fast!)]).toEqual([false, "on"]);
  expect(fastOn({ ...c.fast!, currentValue: "on" })).toBe(true);
});

test("a tool card without input takes the permission request's input; an empty update keeps it", () => {
  const input = { title: "t", nodes: ["a"] };
  const s = play([
    { type: "ToolCall", sessionId: "s", call: { toolCallId: "x", title: "mcp__plugin_ig_ig__ig_show", rawInput: {} } },
    { type: "Permission", requestId: 1, sessionId: "s", toolCall: { toolCallId: "x", rawInput: input }, options: [] },
    { type: "ToolCallUpdate", sessionId: "s", update: { toolCallId: "x", rawInput: {}, status: "completed" } },
  ]);
  const tool = s.items.find((i) => i.kind === "tool");
  expect(tool?.kind === "tool" && tool.call.rawInput).toEqual(input);
});

test("the setup card says Installing only while an install runs", () => {
  expect(chat).toContain('{c.installing ? "Installing…" : "Install"}');
});

test("openQuestions counts only unanswered question items", () => {
  const q = (requestId: number): ChatAction => ({ type: "Question", requestId, sessionId: "s", message: "?", schema: {} });
  const perm: ChatAction = { type: "Permission", requestId: 9, sessionId: "s", toolCall: { toolCallId: "t" }, options: [] };
  const s = play([{ type: "Sent", text: "hi" }, q(1), q(2), perm, { type: "Answered", requestId: 1, answer: "a" }]);
  expect(openQuestions(s)).toBe(1);
  expect(openQuestions(play([{ type: "TurnEnded", stopReason: "end_turn" }], s))).toBe(0);
});
