// The add-box form's rules: the kinds offered where the box goes (from `view.kinds`), their icons, the id slug
// (mirrors `slug` in crates/ig-app/src/map/facts/slug.rs), and the "code" prefill.
import {
  AppWindow, Bot, Box, Cable, Clock, Database, Folder, Globe, Inbox, ListChecks, LogIn, Network, Package, Plug, Server, Shapes, Sparkles, User,
} from "lucide-static";
import type { KindView, MapBox } from "../types";

// the `icon` names of crates/ig-app/assets/kinds.kdl; any other name (a team kind's) draws a box
const ICONS: Record<string, string> = {
  "app-window": AppWindow, bot: Bot, cable: Cable, clock: Clock, database: Database, folder: Folder, globe: Globe, inbox: Inbox,
  "list-checks": ListChecks, "log-in": LogIn, network: Network, package: Package, plug: Plug, server: Server, shapes: Shapes, sparkles: Sparkles, user: User,
};
/** The kind's lucide icon as a CSS mask image (drawn in the text colour). */
export const iconMask = (icon: string) => `url("data:image/svg+xml,${encodeURIComponent(ICONS[icon] ?? Box)}")`;

/** The kind of box `b` in the table (a team kind by its own name). */
export const kindOf = (b: MapBox, kinds: KindView[]) => kinds.find((k) => k.name === (b.custom ?? b.kind.value));

/** Whether a box in `parent` sits inside a running block: a run block (or a module, which stands for one) at or above it.
 *  Mirrors `run_above` in crates/ig-app/src/map/warn.rs. */
export function inside(parent: string, boxes: Map<string, MapBox>, kinds: KindView[]): boolean {
  for (let b = boxes.get(parent); b; b = b.parent && b.parent !== b.id ? boxes.get(b.parent) : undefined) {
    if (kindOf(b, kinds)?.role === "run" || b.kind.value === "module") return true;
  }
  return false;
}

/** The form's groups: outside a running block the run kinds, the team's and grouping; inside, the design kinds and group. */
export function sections(kinds: KindView[], within: boolean): { label: string; kinds: KindView[] }[] {
  const fits = (k: KindView) => (within ? k.role === "design" || k.role === "group" : k.role === "run");
  const out = within
    ? [{ label: "Inside", kinds: kinds.filter((k) => !k.custom && fits(k)) }, { label: "Your team's kinds", kinds: kinds.filter((k) => k.custom && fits(k)) }]
    : [{ label: "Runs", kinds: kinds.filter((k) => !k.custom && fits(k)) }, { label: "Your team's kinds", kinds: kinds.filter((k) => k.custom && (fits(k) || k.role === "group")) },
      { label: "Grouping", kinds: kinds.filter((k) => !k.custom && k.role === "group") }];
  return out.filter((s) => s.kinds.length);
}

const MAX_ID = 64;

/** `[a-z0-9-]`, runs of anything else as one `-`, trimmed, at most 64 chars; `fallback` when nothing is left. */
export function slug(s: string, fallback = "box"): string {
  const t = s.replace(/[A-Z]/g, (c) => c.toLowerCase()).replace(/[^a-z0-9]+/g, "-").replace(/^-+/, "").slice(0, MAX_ID).replace(/-+$/, "");
  return t || fallback;
}

/** `slug(name)`, with `-2`, `-3`… when taken (still at most 64 chars). */
export function freeId(name: string, taken: Set<string>): string {
  const base = slug(name);
  let id = base;
  for (let n = 2; taken.has(id); n++) {
    const suffix = `-${n}`;
    id = base.slice(0, MAX_ID - suffix.length).replace(/-+$/, "") + suffix;
  }
  return id;
}

/** The parent's first glob up to its last `/` before any glob character, then `<slug>/**`; `<slug>/**` when it has none. */
export function prefill(globs: string[], name: string): string {
  const s = name.trim() ? slug(name) : "";
  const g = globs[0];
  // the folder of the parent's first glob: `apps/api/**`, `apps/api/main.ts` and `apps/api/*/x.ts` all give `apps/api/`
  const lit = g ? g.slice(0, g.search(/[*?[{]/) < 0 ? undefined : g.search(/[*?[{]/)) : "";
  const dir = lit.slice(0, lit.lastIndexOf("/") + 1);
  return `${dir}${s}/**`;
}
