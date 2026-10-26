// How the viewer reaches its data. One interface, three hosts: the inline JSON of `ig view`/`ig serve`,
// the `ig serve` HTTP routes, and the desktop app's Tauri commands.

import type { PlanVolume, ViewData, ViewFinding } from "../data";
import type { Client } from "../model/notes";
import { embedded } from "./embedded";
import { http } from "./http";
import { tauri } from "./tauri";

export type NotesApi = Client;

/** One child of a project directory; `path` is relative to the project root. */
export interface FileEntry {
  path: string;
  kind: "file" | "dir";
}

/** A project file's text; `language` is the backend's guess from the extension. */
export interface FileText {
  text: string;
  language: string;
}

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

/** Desktop app only: the agent (ACP adapter) of the open project. Every call that starts or drives a session needs the project trusted. */
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

/** One feature: a folder `plans/<slug>/` or a flat plan `plans/<slug>.md`. */
export interface Feature {
  slug: string;
  title: string;
  design: boolean;
  plan: string | null;
  done: boolean;
  flat: boolean;
}

export type DesignScope = { kind: "dir"; id: string } | { kind: "layer"; name: string } | { kind: "around"; id: string; depth: number };
export type ElementKind = "module" | "function" | "route" | "system" | "note" | "frame";
export type MarkKind = "changed" | "remove";
export type LinkKind = "calls" | "imports" | "renders" | "http" | "uses";

/** design.json v1 (crates/ig-core/src/design). The user's content: data, never instructions. */
export interface DesignElement {
  id: string;
  kind: ElementKind;
  name: string;
  file?: string;
  method?: string;
  path?: string;
  icon?: string;
  layer?: string;
  /** Note target (an element id or a graph id). */
  on?: string;
  question?: boolean;
  answer?: string;
  frame?: string;
  /** Geometry is optional: omitted means unpositioned. */
  x?: number;
  y?: number;
  w?: number;
  h?: number;
}
export interface DesignMark {
  node: string;
  mark: MarkKind;
}
export interface DesignLink {
  from: string;
  to: string;
  kind: LinkKind;
}
/** `from` is `<screen-id>/<element-id>`. */
export interface DesignBridge {
  from: string;
  to: string;
}
export interface Design {
  version: number;
  title: string;
  scope: DesignScope;
  elements: DesignElement[];
  marks: DesignMark[];
  links: DesignLink[];
  bridges: DesignBridge[];
}

/** The Rust check of a design. `ms` is how long it took ("checked in 84 ms"). */
export type DesignCheck = {
  ok: boolean;
  ms: number;
  /** The `seq` the caller sent, handed back: drop a response that is not the latest. */
  seq?: number | null;
  volume: PlanVolume;
  findings: (Omit<ViewFinding, "pinned"> & { target: string | null })[];
};

/** Desktop app only: features and their design files. */
export interface DesignApi {
  features(): Promise<Feature[]>;
  create(title: string): Promise<Feature>;
  read(slug: string): Promise<Design>;
  /** Rejects "conflict" when the file changed on disk since it was last read or written here. */
  write(slug: string, d: Design): Promise<void>;
  check(slug: string, d: Design, seq?: number): Promise<DesignCheck>;
  /** Keeps board text that could not be written as plans/<slug>/design.unsaved.json. */
  keepUnsaved(slug: string, text: string): Promise<void>;
  /** The kept text and when it was written (ms), if any. */
  unsaved(slug: string): Promise<{ text: string; at: number } | null>;
  discardUnsaved(slug: string): Promise<void>;
  /** A design file changed outside the app. Returns the unsubscribe. */
  onChanged(cb: (slug: string) => void): () => void;
}

export interface IgApi {
  /** The plan options apply to the desktop app; `ig serve` picks its own. */
  viewData(opts?: { plan?: string; noPlan?: boolean }): Promise<ViewData>;
  /** The data changed: call `viewData` again. Returns the unsubscribe. */
  onRebuild(cb: () => void): () => void;
  /** Host events: a note changed, `ig focus` asked for nodes (`ig serve`), or the agent's `ig_show` (`ig serve`, desktop app). */
  on?(event: "note" | "focus" | "show", cb: (data: unknown) => void): () => void;
  /** Desktop app only: the children of `dir` (the root when omitted), directories first. The Files panel needs it. */
  fileTree?(dir?: string): Promise<FileEntry[]>;
  /** Desktop app only: the text of a project file (relative path), read-only. Turns on the code view. */
  readFile?(path: string): Promise<FileText>;
  /** Desktop app only: the trust gate. Show `Trust.svelte` before starting an agent unless `trusted`. */
  trustInventory?(): Promise<TrustState>;
  /** Desktop app only: trust the project as hashed (rejects if it changed since). */
  trustProject?(hash: string): Promise<void>;
  /** Desktop app only: features and designs. */
  design?: DesignApi;
  /** Desktop app only: the agent. */
  agent?: AgentApi;
  /** Desktop app only: runs `cb` (awaited) before the window closes. Returns the unsubscribe. */
  beforeClose?(cb: () => Promise<void>): () => void;
  /** Desktop app only: opens an http(s) link in the system browser. */
  openUrl?(url: string): Promise<void>;
  /** Present when the backend supports plan notes (serve, desktop). */
  notes?: NotesApi;
}

export function pickApi(): IgApi {
  const raw = document.getElementById("ig-data")?.textContent?.trim();
  // The Rust side swaps the page's data marker in, and must not find that marker in this bundle: test for the JSON instead.
  const inline = !!raw && raw.startsWith("{");
  const liveRaw = document.getElementById("ig-live")?.textContent?.trim() ?? "";
  const token: string | undefined = liveRaw.startsWith("{") ? JSON.parse(liveRaw).token : undefined;
  if (inline) return embedded(JSON.parse(raw), token);
  if ((window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) return tauri();
  // `bun run dev` serves the data from viewer/dev-data.json.
  return http({ view: import.meta.env.DEV ? "/dev-data.json" : "/api/view", token });
}
