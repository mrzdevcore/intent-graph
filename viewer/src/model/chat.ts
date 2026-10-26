// The chat's state, as a pure reducer over the agent's events (`agent-event`) and the chat's own
// actions (a sent message, an answer, a turn's end, a session switch).

import type { AgentEvent, ConfigOption, PermissionOption, ToolCallInfo } from "../api";

export interface QuestionOption {
  value: string;
  title: string;
  description?: string;
}

/** One `AskUserQuestion` question: `field` is its form key, `custom` its free-text "Other" key. */
export interface Question {
  field: string;
  title?: string;
  description?: string;
  multi: boolean;
  options: QuestionOption[];
  custom?: string;
}

export type Item =
  | { kind: "text"; role: "agent" | "thought" | "user"; text: string }
  | { kind: "tool"; call: ToolCallInfo }
  /** `answered`: the option id, "stopped" or "cancelled"; null while it waits. */
  | { kind: "permission"; requestId: number; toolCall: ToolCallInfo; options: PermissionOption[]; answered: string | null }
  | { kind: "question"; requestId: number; message: string; questions: Question[]; answered: string | null }
  | { kind: "error"; text: string };

export interface Usage {
  used: number | null;
  size: number | null;
  cost: { amount: number; currency: string } | null;
  rateLimit: { status?: string; utilization?: number; [k: string]: unknown } | null;
}

export interface ChatState {
  sessionId: string | null;
  title: string | null;
  items: Item[];
  commands: { name: string; description: string }[];
  config: ConfigOption[];
  usage: Usage | null;
  /** A turn is running (Esc cancels it). */
  running: boolean;
  /** Permission and question cards still waiting. */
  pending: number;
  /** What the live region reads out: the agent's whole reply, once its turn ends (never a replay). */
  live: string;
  /** Where the running turn's items start. */
  turnFrom: number;
  login: boolean;
  setup: { instructions: string; installable: boolean } | null;
  stopped: { reason: string; stderr: string | null } | null;
}

export type ChatAction =
  | AgentEvent
  | { type: "Sent"; text: string }
  | { type: "Answered"; requestId: number; answer: string }
  | { type: "TurnEnded"; stopReason: string }
  | { type: "Failed"; error: string }
  /** Before a start or a load: a blank chat (a load's replay fills it). */
  | { type: "Reset" }
  /** A session is on: its id (a restart keeps the transcript). */
  | { type: "Started"; sessionId: string };

export const initialChat: ChatState = {
  sessionId: null, title: null, items: [], commands: [], config: [], usage: null,
  running: false, pending: 0, live: "", turnFrom: 0, login: false, setup: null, stopped: null,
};

/** The questions of an `AskUserQuestion` form, in field order. */
export function questionsOf(schema: unknown): Question[] {
  const props = ((schema as { properties?: Record<string, Record<string, unknown>> })?.properties) ?? {};
  const opts = (xs: unknown): QuestionOption[] =>
    (Array.isArray(xs) ? xs : []).map((o: { const?: unknown; title?: unknown; description?: unknown }) => ({
      value: String(o.const ?? ""),
      title: String(o.title ?? o.const ?? ""),
      ...(typeof o.description === "string" ? { description: o.description } : {}),
    }));
  return Object.entries(props)
    .filter(([, p]) => p.oneOf || (p.items as { anyOf?: unknown })?.anyOf)
    .map(([field, p]) => {
      const multi = !p.oneOf;
      const q: Question = { field, multi, options: opts(multi ? (p.items as { anyOf: unknown }).anyOf : p.oneOf) };
      if (typeof p.title === "string") q.title = p.title;
      if (typeof p.description === "string") q.description = p.description;
      if (props[field + "_custom"]) q.custom = field + "_custom";
      return q;
    });
}

/** The commands a `/` prefix completes to (before any space), ig's first. */
export function completions(commands: ChatState["commands"], text: string) {
  const m = /^\/(\S*)$/.exec(text);
  if (!m) return [];
  const ig = (c: { name: string }) => (c.name.startsWith("ig:") ? 0 : 1);
  return commands
    .filter((c) => c.name.startsWith(m[1]))
    .sort((a, b) => ig(a) - ig(b) || a.name.localeCompare(b.name));
}

/** No input yet: absent, or the `{}` the adapter sends before the input streams in. */
const noInput = (v: unknown) => v === undefined || v === null || (typeof v === "object" && !Object.keys(v as object).length);

/** A tool call update over the call: fields it sends replace the old ones (an empty input never replaces one). */
function merge(call: ToolCallInfo, update: ToolCallInfo): ToolCallInfo {
  const out = { ...call };
  for (const [k, v] of Object.entries(update))
    if (v !== null && v !== undefined && k !== "sessionUpdate" && !(k === "rawInput" && noInput(v))) out[k] = v;
  return out;
}

/** Every waiting card closed with `why`. */
function closeAll(s: ChatState, why: string): ChatState {
  if (!s.pending) return s;
  const items = s.items.map((i) =>
    (i.kind === "permission" || i.kind === "question") && i.answered === null ? { ...i, answered: why } : i);
  return { ...s, items, pending: 0 };
}

/** The agent's text of the running turn. */
function reply(s: ChatState): string {
  return s.items.slice(s.turnFrom).flatMap((i) => (i.kind === "text" && i.role === "agent" ? [i.text] : [])).join("\n");
}

export function chatReducer(s: ChatState, a: ChatAction): ChatState {
  switch (a.type) {
    case "MessageChunk": {
      const text = a.content?.text;
      if (typeof text !== "string") return s;
      const last = s.items.at(-1);
      const items = last?.kind === "text" && last.role === a.role
        ? [...s.items.slice(0, -1), { ...last, text: last.text + text }]
        : [...s.items, { kind: "text" as const, role: a.role, text }];
      return { ...s, items };
    }
    case "ToolCall":
    case "ToolCallUpdate": {
      const u = a.type === "ToolCall" ? a.call : a.update;
      const at = s.items.findIndex((i) => i.kind === "tool" && i.call.toolCallId === u.toolCallId);
      if (at < 0) return { ...s, items: [...s.items, { kind: "tool", call: merge({ toolCallId: u.toolCallId }, u) }] };
      const old = s.items[at] as { kind: "tool"; call: ToolCallInfo };
      return { ...s, items: s.items.map((i, n) => (n === at ? { kind: "tool", call: merge(old.call, u) } : i)) };
    }
    case "Permission": {
      // The request carries the tool's input: a card still without one takes it (the "Show in graph" chip needs it).
      const items = s.items.map((i) =>
        i.kind === "tool" && i.call.toolCallId === a.toolCall.toolCallId && noInput(i.call.rawInput) && !noInput(a.toolCall.rawInput)
          ? { ...i, call: { ...i.call, rawInput: a.toolCall.rawInput } } : i);
      return {
        ...s, pending: s.pending + 1,
        items: [...items, { kind: "permission", requestId: a.requestId, toolCall: a.toolCall, options: a.options, answered: null }],
      };
    }
    case "Question":
      return {
        ...s, pending: s.pending + 1,
        items: [...s.items, { kind: "question", requestId: a.requestId, message: a.message, questions: questionsOf(a.schema), answered: null }],
      };
    case "Answered": {
      const at = s.items.findIndex((i) =>
        (i.kind === "permission" || i.kind === "question") && i.requestId === a.requestId && i.answered === null);
      if (at < 0) return s;
      const items = s.items.map((i, n) => (n === at ? { ...i, answered: a.answer } as Item : i));
      return { ...s, items, pending: s.pending - 1 };
    }
    case "Usage":
      return { ...s, usage: { used: a.used, size: a.size, cost: a.cost, rateLimit: a.rateLimit as Usage["rateLimit"] } };
    case "Info":
      return { ...s, title: a.title };
    case "Commands":
      return { ...s, commands: a.commands };
    case "Config":
      return { ...s, config: Array.isArray(a.options) ? a.options : [] };
    case "Plan":
      return s;
    case "Auth":
      return { ...s, login: a.needed };
    case "Setup":
      return { ...s, setup: { instructions: a.instructions, installable: a.installable } };
    case "Stopped":
      return { ...closeAll(s, "stopped"), running: false, stopped: { reason: a.reason, stderr: a.stderr } };
    case "Sent":
      return { ...s, running: true, live: "", stopped: null, turnFrom: s.items.length + 1, items: [...s.items, { kind: "text", role: "user", text: a.text }] };
    case "TurnEnded":
      return { ...closeAll(s, "cancelled"), running: false, live: s.running ? reply(s) : s.live };
    case "Failed":
      return { ...closeAll(s, "cancelled"), running: false, items: [...s.items, { kind: "error", text: a.error }] };
    case "Reset":
      return initialChat;
    case "Started":
      return { ...s, sessionId: a.sessionId, stopped: null, login: false, setup: null };
  }
}

/** A transcript row: an item, or consecutive tool calls of one kind in one card. */
export type Row = Item | { kind: "group"; label: string; status: string; calls: ToolCallInfo[] };

const KINDS: Record<string, [string, string]> = {
  execute: ["Ran", "commands"], read: ["Read", "files"], edit: ["Edited", "files"],
  search: ["Ran", "searches"], fetch: ["Fetched", "pages"], delete: ["Deleted", "files"], move: ["Moved", "files"],
};

/** The worst status first: failed, then still running, else completed. */
function groupStatus(calls: ToolCallInfo[]): string {
  const st = calls.map((c) => c.status ?? "pending");
  return st.includes("failed") ? "failed" : st.find((x) => x !== "completed") ?? "completed";
}

export function groupTools(items: Item[]): Row[] {
  const out: Row[] = [];
  for (let i = 0; i < items.length; ) {
    const it = items[i];
    const kind = it.kind === "tool" ? it.call.kind : undefined;
    let j = i + 1;
    while (kind && j < items.length && items[j].kind === "tool" && (items[j] as { call: ToolCallInfo }).call.kind === kind) j++;
    if (j - i < 2) {
      out.push(it);
      i++;
      continue;
    }
    const calls = items.slice(i, j).map((x) => (x as { call: ToolCallInfo }).call);
    const [verb, noun] = KINDS[kind!] ?? ["", kind + " calls"];
    out.push({ kind: "group", label: `${verb} ${calls.length} ${noun}`.trim(), status: groupStatus(calls), calls });
    i = j;
  }
  return out;
}

/** A command without a leading `cd <root> &&`, the root itself shown as `.`. */
export function shortCommand(title: string, root: string): string {
  const r = root.replace(/\/$/, "");
  if (!r) return title;
  const esc = r.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return title
    .replace(new RegExp(`^cd (["']?)${esc}\\1 && `), "")
    .replace(new RegExp(`${esc}(?=[/\\s"';|&)]|$)`, "g"), ".");
}

/** Where the agent's config options go: mode and model as chips, fast mode as a toggle (when it
 *  has two values), the rest (effort, agent, ...) in the "⋯" menu. */
export function controls(config: ConfigOption[]) {
  const is = (o: ConfigOption, k: string) => o.category === k || o.id === k;
  const mode = config.find((o) => is(o, "mode"));
  const model = config.find((o) => is(o, "model"));
  const fast = config.find((o) => /fast/i.test(o.id + " " + o.name) && o.options.length === 2);
  return { mode, model, fast, more: config.filter((o) => o !== mode && o !== model && o !== fast) };
}

const OFF = /^(off|false|disabled?|no)$/i;
/** A two-value option is on unless its current value (or its name) reads as off. */
export function fastOn(o: ConfigOption): boolean {
  const cur = o.options.find((x) => x.value === o.currentValue);
  return !OFF.test(o.currentValue) && !OFF.test(cur?.name ?? "");
}
export const fastNext = (o: ConfigOption) => o.options.find((x) => x.value !== o.currentValue)!.value;
