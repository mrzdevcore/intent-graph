// The desktop app backend: Tauri commands and events.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ViewData } from "../data";
import type { IgApi } from "./index";

export function tauri(): IgApi {
  return {
    viewData: (opts) => invoke<ViewData>("view_data", { plan: opts?.plan, noPlan: opts?.noPlan }),
    onRebuild(cb) {
      // listen() is async: an unsubscribe that comes before it resolves must still unlisten.
      let off: (() => void) | undefined;
      let stopped = false;
      listen("rebuild", () => cb()).then((u) => (stopped ? u() : (off = u)));
      return () => {
        stopped = true;
        off?.();
        off = undefined;
      };
    },
  };
}
