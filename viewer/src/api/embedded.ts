// The page's own inline JSON (`ig view`, and `ig serve`'s first paint). Once the server rebuilds, the
// inline copy is stale, so later reads go to the server.

import type { ViewData } from "../data";
import type { IgApi } from "./index";
import { http } from "./http";

export function embedded(data: ViewData, token?: string): IgApi {
  const live = token ? http({ token }) : undefined;
  let fresh = true;
  return {
    viewData: async () => {
      if (!fresh && live) return live.viewData();
      fresh = false;
      return data;
    },
    onRebuild: (cb) => (live ? live.onRebuild(cb) : () => {}),
    on: live?.on,
    notes: live?.notes,
  };
}
