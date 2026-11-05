// `ig view`'s page: the View the Rust side wrote into <script id="ig-data">.

import type { View } from "../map/types";
import { parseShow } from "../model/show";
import type { IgApi } from "./index";

export function embedded(): IgApi {
  return {
    view: async () => JSON.parse(document.getElementById("ig-data")?.textContent ?? "") as View,
    shown: () => {
      try {
        return parseShow(JSON.parse(document.getElementById("ig-show")?.textContent ?? ""));
      } catch {
        return null;
      }
    },
  };
}
