// The `ig serve` backend: routes under /api, plus the server-sent event stream.

import type { ViewData } from "../data";
import { client } from "../model/notes";
import type { IgApi } from "./index";

/** `view` is the data URL (the dev server serves /dev-data.json); `token` turns on the notes API. */
export function http(o: { token?: string; view?: string } = {}): IgApi {
  const subs = new Map<string, Set<(data: unknown) => void>>();
  let es: EventSource | undefined;
  let n = 0;

  /** One EventSource shared by every listener, closed with the last. */
  function on(event: string, cb: (data: unknown) => void) {
    if (!window.EventSource) return () => {};
    if (!es) es = new EventSource("/api/events");
    if (!subs.has(event)) {
      const set = new Set<(data: unknown) => void>();
      subs.set(event, set);
      es.addEventListener(event, (e) => {
        const raw = (e as MessageEvent).data;
        let v: unknown = raw;
        try { v = raw === undefined || raw === "" ? undefined : JSON.parse(raw); } catch { /* plain text */ }
        set.forEach((f) => f(v));
      });
    }
    subs.get(event)!.add(cb);
    n++;
    return () => {
      if (!subs.get(event)!.delete(cb)) return;
      if (--n === 0) {
        es?.close();
        es = undefined;
        subs.clear();
      }
    };
  }

  return {
    async viewData() {
      const r = await fetch(o.view ?? "/api/view");
      const v = await r.json();
      if (!r.ok) throw new Error(v.error || "ig serve: " + r.status);
      return v as ViewData;
    },
    onRebuild: (cb) => on("rebuild", () => cb()),
    on: (event, cb) => on(event, cb),
    notes: o.token ? client(o.token) : undefined,
  };
}
