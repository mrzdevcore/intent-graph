// The desktop app backend: Tauri commands and events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ViewData } from "../data";
import type { AgentEvent, Design, Feature, FileEntry, FileText, IgApi, TrustState } from "./index";

/** listen() is async: an unsubscribe that comes before it resolves must still unlisten. */
function subscribe<T>(name: string, cb: (payload: T) => void): () => void {
  let off: (() => void) | undefined;
  let stopped = false;
  listen<T>(name, (e) => cb(e.payload)).then((u) => (stopped ? u() : (off = u)));
  return () => {
    stopped = true;
    off?.();
    off = undefined;
  };
}

/** What must be written before the app goes away (the board's autosave). */
const flushers = new Set<() => Promise<void>>();

/** Quit (Cmd+Q) or window close: every flush runs (a failing one is logged, the whole is bounded
 *  by 2 s), then `ack` lets the close or the held exit go on (crates/ig-desktop/src/exit.rs). */
export async function flushThenAck(fs: Iterable<() => Promise<void>>, ack: () => Promise<unknown>, ms = 2000): Promise<void> {
  const all = Promise.all([...fs].map((f) => f().catch((e: unknown) => console.error(e))));
  await Promise.race([all, new Promise((r) => setTimeout(r, ms))]);
  await ack();
}

export function tauri(): IgApi {
  // always listening: with nothing to flush, the ack comes at once
  subscribe("flush-before-exit", () => void flushThenAck(flushers, () => invoke("exit_flushed")));
  // one close handler: Tauri closes the window once it returns
  try {
    getCurrentWindow().onCloseRequested(() => flushThenAck(flushers, async () => {})).catch((e: unknown) => console.error(e));
  } catch {
    // no Tauri window (tests): nothing to hold
  }
  return {
    viewData: (opts) => invoke<ViewData>("view_data", { plan: opts?.plan, noPlan: !!opts?.noPlan }),
    fileTree: (dir) => invoke<FileEntry[]>("file_tree", { dir }),
    readFile: (path) => invoke<FileText>("read_file", { path }),
    trustInventory: () => invoke<TrustState>("trust_inventory"),
    trustProject: (hash) => invoke<void>("trust_project", { hash }),
    openUrl: (url) => invoke<void>("open_url", { url }),
    onRebuild: (cb) => subscribe("rebuild", () => cb()),
    // Tauri awaits the handler, then closes the window (core:window:allow-destroy).
    beforeClose: (cb) => {
      flushers.add(cb);
      return () => void flushers.delete(cb);
    },
    // Only `show` is emitted (crates/ig-desktop/src/main.rs).
    on: (event, cb) => subscribe(event, cb),
    design: {
      features: () => invoke<Feature[]>("features"),
      create: (title) => invoke<Feature>("create_feature", { title }),
      read: (slug) => invoke<Design>("read_design", { slug }),
      write: (slug, design) => invoke<void>("write_design", { slug, design }),
      // Task 3 adds the command.
      check: (slug, design, seq) => invoke("check_design", { slug, design, seq }),
      keepUnsaved: (slug, text) => invoke<void>("keep_unsaved", { slug, text }),
      unsaved: (slug) => invoke<{ text: string; at: number } | null>("read_unsaved", { slug }),
      discardUnsaved: (slug) => invoke<void>("discard_unsaved", { slug }),
      onChanged: (cb) => subscribe<{ slug: string }>("design-changed", (e) => cb(e.slug)),
    },
    agent: {
      start: (feature) => invoke<string>("agent_start", { feature }),
      prompt: (text, attachments) => invoke<string>("agent_prompt", { text, attachments: attachments ?? [] }),
      cancel: () => invoke<void>("agent_cancel"),
      answer: (requestId, optionId) => invoke<void>("agent_answer", { requestId, optionId }),
      reply: (requestId, content) => invoke<void>("agent_reply", { requestId, content }),
      setConfig: (configId, value) => invoke<void>("agent_set_config", { configId, value }),
      list: () => invoke("agent_list"),
      load: (sessionId) => invoke<void>("agent_load", { sessionId }),
      restart: () => invoke<string>("agent_restart"),
      install: () => invoke<void>("agent_install"),
      onEvent: (cb) => subscribe<AgentEvent>("agent-event", cb),
    },
  };
}
