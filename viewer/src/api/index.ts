// How the viewer reaches its data. One interface, three hosts: the inline JSON of `ig view`/`ig serve`,
// the `ig serve` HTTP routes, and the desktop app's Tauri commands.

import type { ViewData } from "../data";
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
  hooks: { event: string; command: string }[];
  instructions: string[];
  mcp: string[];
  unreadable: string[];
  ig_plugin: boolean;
  hash: string;
  trusted: boolean;
}

export interface IgApi {
  /** The plan options apply to the desktop app; `ig serve` picks its own. */
  viewData(opts?: { plan?: string; noPlan?: boolean }): Promise<ViewData>;
  /** The data changed: call `viewData` again. Returns the unsubscribe. */
  onRebuild(cb: () => void): () => void;
  /** `ig serve` events: a note changed, or `ig focus` asked for nodes. */
  on?(event: "note" | "focus", cb: (data: unknown) => void): () => void;
  /** Desktop app only: the children of `dir` (the root when omitted), directories first. The Files panel needs it. */
  fileTree?(dir?: string): Promise<FileEntry[]>;
  /** Desktop app only: the text of a project file (relative path), read-only. Turns on the code view. */
  readFile?(path: string): Promise<FileText>;
  /** Desktop app only: the trust gate. Show `Trust.svelte` before starting an agent unless `trusted`. */
  trustInventory?(): Promise<TrustState>;
  /** Desktop app only: trust the project as hashed (rejects if it changed since). */
  trustProject?(hash: string): Promise<void>;
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
