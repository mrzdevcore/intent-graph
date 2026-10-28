// `ig view`'s page: the View the Rust side wrote into <script id="ig-data">.

import type { View } from "../map/types";
import type { IgApi } from "./index";

export function embedded(): IgApi {
  return {
    view: async () => JSON.parse(document.getElementById("ig-data")?.textContent ?? "") as View,
  };
}
