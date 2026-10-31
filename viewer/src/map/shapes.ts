// What each run block is drawn as, shared by Shape.svelte, MapBox.svelte and the map.
import type { MapBox } from "./types";

export type ShapeName = "service" | "app" | "cli" | "job" | "store" | "queue" | "model" | "agent" | "external" | "person" | "group";
const DRAWN = new Set(["service", "app", "job", "store", "queue", "model", "agent", "external", "person", "group"]);

/** What a box is drawn as, by its base kind; an app whose tech names a CLI, or whose entries are commands and
 *  no route, is a terminal. `kids`: its children. None for design blocks and code. */
export function shapeOf(b: MapBox, kids: MapBox[] = []): ShapeName | undefined {
  const k = b.kind.value;
  if (!DRAWN.has(k)) return undefined;
  if (k !== "app") return k as ShapeName;
  const ways = kids.filter((c) => c.kind.value === "entry").map((c) => c.props?.kind);
  return /\bcli\b/i.test(b.tech?.value ?? "") || (ways.includes("command") && !ways.includes("route")) ? "cli" : "app";
}

/** An external is a cloud when its frame holds the cloud's text inset (about 13.75 x 7.5rem at the map's 16px
 *  units), else a dashed rounded box. */
export const cloud = (w: number, h: number) => w >= 220 && h >= 120;
/** Room the shape keeps clear of the text: top, right, bottom, left. */
const PAD: Partial<Record<ShapeName, [number, number, number, number]>> = {
  service: [30, 14, 12, 14], app: [32, 14, 12, 14], cli: [30, 14, 26, 14], job: [12, 50, 24, 14],
  store: [36, 18, 22, 18], queue: [14, 44, 14, 64], model: [20, 22, 20, 22], agent: [14, 44, 14, 26], external: [12, 16, 12, 16],
};
export const padOf = (s: ShapeName, w: number, h: number): [number, number, number, number] =>
  s === "external" && cloud(w, h) ? [h * 0.3, w * 0.14, h * 0.06, w * 0.17] : PAD[s] ?? [12, 14, 12, 14];
// a cloud in a 100 x 60 box: a flat bottom, a small bump left and right, a big one on top
export const CLOUD = "M18 59H82C96 59 100 43 90 35C92 17 70 7 58 15C50 1 26 3 24 21C8 21 1 34 5 46C7 55 12 59 18 59Z";
// the family whose hue a shape wears (theme tokens --kind-<family>, -fill, -band)
export const FAMILY: Record<string, string> = { model: "ai", agent: "ai", cli: "app" };
export const hue = (s: ShapeName) => `var(--kind-${FAMILY[s] ?? s})`;
