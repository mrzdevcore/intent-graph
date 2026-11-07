// `ig view`'s page: the View the Rust side wrote into <script id="ig-data">.

import type { View } from "../map/types";
import { parseDiff } from "../model/diff";
import { parseShow } from "../model/show";
import type { IgApi } from "./index";

function read<T>(id: string, parse: (raw: unknown) => T | null): T | null {
  try {
    return parse(JSON.parse(document.getElementById(id)?.textContent ?? ""));
  } catch {
    return null;
  }
}

export function embedded(): IgApi {
  return {
    view: async () => JSON.parse(document.getElementById("ig-data")?.textContent ?? "") as View,
    shown: () => read("ig-show", parseShow),
    diff: () => read("ig-diff", parseDiff),
  };
}
