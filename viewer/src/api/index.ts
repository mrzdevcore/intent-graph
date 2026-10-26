// How the viewer reaches its data. One interface, three hosts: the inline JSON of `ig view`/`ig serve`,
// the `ig serve` HTTP routes, and the desktop app's Tauri commands.

import type { ViewData } from "../data";
import type { Client } from "../model/notes";
import { embedded } from "./embedded";
import { http } from "./http";
import { tauri } from "./tauri";

export type NotesApi = Client;

export interface IgApi {
  /** The plan options apply to the desktop app; `ig serve` picks its own. */
  viewData(opts?: { plan?: string; noPlan?: boolean }): Promise<ViewData>;
  /** The data changed: call `viewData` again. Returns the unsubscribe. */
  onRebuild(cb: () => void): () => void;
  /** `ig serve` events: a note changed, or `ig focus` asked for nodes. */
  on?(event: "note" | "focus", cb: (data: unknown) => void): () => void;
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
