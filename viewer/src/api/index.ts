// How the viewer reaches its host. Two hosts: `ig view`'s page, which has only the embedded map,
// and the desktop app (Tauri): the trust gate, the agent and links.

import type { Edit, View } from "../map/types";
import type { Show } from "../model/show";
import { embedded } from "./embedded";
import { tauri } from "./tauri";

/** What a Claude Code session in the project would run, and whether the user trusted exactly that. */
export interface TrustState {
  hooks: { event: string; when: string; command: string }[];
  instructions: string[];
  mcp: string[];
  unreadable: string[];
  notes: string[];
  any_files: boolean;
  ig_plugin: boolean;
  app_plugin: string;
  hash: string;
  /** Why `.claude/` could not be fully checked: trust is not possible. */
  incomplete: string | null;
  trusted: boolean;
}

/** A tool call or its update, as the adapter sends it; `content` may hold `{type: "diff", path, oldText, newText}`. */
export interface ToolCallInfo {
  toolCallId: string;
  title?: string;
  kind?: string;
  status?: "pending" | "in_progress" | "completed" | "failed";
  content?: unknown[];
  locations?: { path: string }[];
  rawInput?: unknown;
  rawOutput?: unknown;
  [key: string]: unknown;
}

/** One permission choice; answer with its `optionId` (the adapter's own, e.g. "allow-once", "reject"). */
export interface PermissionOption {
  optionId: string;
  name: string;
  kind: "allow_once" | "allow_always" | "reject_once" | "reject_always";
}

/** One session setting (ACP `configOptions`), e.g. `mode` or `model`; set it to one of `options`' values. */
export interface ConfigOption {
  id: string;
  name: string;
  category?: string;
  type: string;
  currentValue: string;
  options: { value: string; name: string; description?: string }[];
}

/** The desktop app's `agent-event`s (crates/ig-desktop/src/events.rs). */
export type AgentEvent =
  | { type: "MessageChunk"; sessionId: string; role: "agent" | "thought" | "user"; content: { type: string; text?: string } }
  | { type: "ToolCall"; sessionId: string; call: ToolCallInfo }
  | { type: "ToolCallUpdate"; sessionId: string; update: ToolCallInfo }
  | { type: "Usage"; sessionId: string; used: number | null; size: number | null; cost: { amount: number; currency: string } | null; rateLimit: unknown }
  | { type: "Info"; sessionId: string; title: string | null }
  | { type: "Commands"; sessionId: string; commands: { name: string; description: string }[] }
  | { type: "Plan"; sessionId: string; entries: unknown }
  | { type: "Permission"; requestId: number; sessionId: string; toolCall: ToolCallInfo; options: PermissionOption[] }
  /** `AskUserQuestion`: a form `schema` (`properties.question_<n>` with `oneOf` or `items.anyOf` options). Answer with `reply`. */
  | { type: "Question"; requestId: number; sessionId: string; message: string; schema: unknown }
  /** The session's `configOptions` (mode, model, ...); change one with `setConfig`. */
  | { type: "Config"; sessionId: string; options: ConfigOption[] }
  /** Not logged in: show "Run `claude` and `/login` in a terminal, then retry". */
  | { type: "Auth"; needed: boolean; label: string | null }
  /** The adapter exited: keep the transcript, offer `agentRestart`. */
  | { type: "Stopped"; reason: string; stderr: string | null }
  /** A short notice for the transcript. */
  | { type: "Notice"; text: string }
  /** No node or no adapter: show `instructions`; `installable`: offer `agentInstall` (after the user confirms). */
  | { type: "Setup"; instructions: string; installable: boolean };

/** The agent (ACP adapter) of the open project. Every call that starts or drives a session needs the project trusted. */
export interface AgentApi {
  /** A new session (for a feature slug, remembered in `.ig/sessions.json`); its id. */
  start(feature?: string): Promise<string>;
  /** One turn; resolves with the stop reason ("end_turn", "cancelled", ...). Attachments: project-relative paths. */
  prompt(text: string, attachments?: string[]): Promise<string>;
  cancel(): Promise<void>;
  /** Answers a `Permission` event, exactly once. */
  answer(requestId: number, optionId: string): Promise<void>;
  /** Answers a `Question` event once: `{field: label | labels}`, or null to skip it. */
  reply(requestId: number, content: Record<string, string | string[]> | null): Promise<void>;
  /** Sets a `ConfigOption` (mode, model, ...); a `Config` event follows. */
  setConfig(configId: string, value: string): Promise<void>;
  /** The adapter's sessions here and the feature → session id map. */
  list(): Promise<{ sessions: { sessionId: string; title?: string; updatedAt?: string }[]; features: Record<string, string> }>;
  load(sessionId: string): Promise<void>;
  /** After `Stopped`: a new adapter and session for the same feature; the new id. */
  restart(): Promise<string>;
  /** Installs the pinned adapter into the app's data dir. Ask the user first. */
  install(): Promise<void>;
  /** Returns the unsubscribe. */
  onEvent(cb: (e: AgentEvent) => void): () => void;
}

/** Edits to the architecture map (desktop only). Each resolves with the fresh view, or rejects with the reader's message. */
export interface EditApi {
  apply(edit: Edit): Promise<View>;
  keep(n?: number): Promise<View>;
  drop(n: number): Promise<View>;
  dropAll(): Promise<View>;
  undo(): Promise<View>;
  redo(): Promise<View>;
}

/** Desktop app members are absent in `ig view`'s page, which has only `view`. */
export interface IgApi {
  /** The trust gate. Show `Trust.svelte` before starting an agent unless `trusted`. */
  trustInventory?(): Promise<TrustState>;
  /** Trust the project as hashed (rejects if it changed since). */
  trustProject?(hash: string): Promise<void>;
  /** The agent. */
  agent?: AgentApi;
  /** Opens an http(s) link in the system browser. */
  openUrl?(url: string): Promise<void>;
  /** Edit the map. */
  edit?: EditApi;
  /** The map (both hosts). */
  view?(): Promise<View>;
  /** The project's files changed and the host rebuilt (desktop only): read `view` again. Returns the unsubscribe. */
  onRebuild?(cb: () => void): () => void;
  onShow?(cb: (s: Show) => void): () => void;
  shown?(): Show | null;
}

export function pickApi(): IgApi {
  return (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ ? tauri() : embedded();
}
