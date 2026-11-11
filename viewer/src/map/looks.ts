import type { BoxKind, Proposal, MapArrow, MapBox, Mark, MarkState, View } from "./types";

export type Tone = "neutral" | "warn" | "bad" | "down";
export type Look = {
  edge: string; width: number; dashed: boolean; glow: string | null; bg: string; fade: number;
  chips: { label: string; tone: Tone }[]; dot: boolean;
};

const TRACE = "var(--map-trace)";
const WARN = "var(--map-warn)", BAD = "var(--map-bad)", DOWN = "var(--map-faint)", OK = "var(--map-green)", NEUTRAL = "var(--map-edge)";
const KIND: Partial<Record<BoxKind, { edge: string; dashed?: boolean }>> = {
  entry: { edge: "var(--map-blue)" }, usecase: { edge: OK }, port: { edge: OK, dashed: true }, domain: { edge: "var(--map-gold)" }, box: { edge: NEUTRAL, dashed: true },
};
const STATE: Record<Exclude<MarkState, "ok">, { edge: string; tone: Tone; bg: string }> = {
  warn: { edge: WARN, tone: "warn", bg: "var(--map-warn-bg)" },
  bad: { edge: BAD, tone: "bad", bg: "var(--map-bad-bg)" },
  down: { edge: DOWN, tone: "down", bg: "var(--map-down-bg)" },
};
const ORIGIN: Record<NonNullable<MapBox["origin"]>, { edge: string; label: string; tone: Tone }> = {
  build: { edge: TRACE, label: "to build", tone: "neutral" },
  drift: { edge: WARN, label: "not in ig/", tone: "warn" },
  moved: { edge: "var(--map-blue)", label: "to move", tone: "neutral" },
  unmanaged: { edge: WARN, label: "not in ig/", tone: "warn" },
};
/** The chip of a proposal: who made the edit. */
export const tag = (by: Proposal["by"]) => (by === "claude" ? "Claude" : "reading");
const RANK: Record<MarkState, number> = { ok: 0, warn: 1, down: 2, bad: 3 };
const worst = (ms: Mark[]) => ms.reduce<Mark | null>((w, m) => (!w || RANK[m.state] > RANK[w.state] ? m : w), null);
const shown = (marks: Mark[], on: Set<string>, target: string) => marks.filter((m) => m.target === target && on.has(m.layer));

/** Precedence: health > highlight (other layers' marks) > origin (build, drift, moved) > status (stale) > kind. */
export function look(id: string, marks: Mark[], on: Set<string>, box: MapBox, by?: Proposal["by"]): Look {
  const mine = shown(marks, on, id);
  const health = worst(mine.filter((m) => m.layer === "health"));
  const other = worst(mine.filter((m) => m.layer !== "health" && m.state !== "ok"));
  const kind = KIND[box.kind.value];
  const hit = health && health.state !== "ok" ? health : other;
  const s = hit && hit.state !== "ok" ? STATE[hit.state] : null;
  const chips = mine.filter((m) => m.state !== "ok" && (m.badge ?? m.value)).map((m) => ({
    label: (m.badge ?? m.value)!, tone: STATE[m.state as Exclude<MarkState, "ok">].tone,
  }));
  const o = box.origin ? ORIGIN[box.origin] : null;
  if (o) chips.push({ label: o.label, tone: o.tone });
  if (box.stale) chips.push({ label: "stale", tone: "neutral" });
  if (by) chips.push({ label: tag(by), tone: "neutral" });
  return {
    edge: s?.edge ?? (by ? TRACE : o?.edge) ?? kind?.edge ?? NEUTRAL,
    width: s && s.tone !== "down" ? 2.5 : 1.5,
    dashed: s ? s.tone === "down" : !!(by || o || box.stale || kind?.dashed),
    glow: s && s.tone !== "down" ? s.edge : null,
    bg: s?.bg ?? "var(--map-surface)",
    fade: s?.tone === "down" || (!s && !o && box.stale) ? 0.6 : !s && box.origin === "build" ? 0.85 : 1,
    chips,
    dot: !s && mine.some((m) => m.state === "ok" && (m.layer === "health" || m.layer === "state")),
  };
}

/** Bad and Down marks under each box (on its descendants and their arrows), for the "N inside" badge. */
export function insideCounts(view: View, marks: Mark[], on: Set<string>): Map<string, number> {
  const parent = new Map(view.boxes.map((b) => [b.id, b.parent]));
  const home = new Map<string, string | undefined>(view.arrows.map((a) => [a.id, a.parent]));
  const out = new Map<string, number>();
  for (const m of marks) {
    if (!on.has(m.layer) || (m.state !== "bad" && m.state !== "down")) continue;
    // `seen`: a bad map's parent loop ends the walk
    const seen = new Set([m.target]);
    for (let p = parent.has(m.target) ? parent.get(m.target) : home.get(m.target); p && !seen.has(p); p = parent.get(p)) {
      seen.add(p);
      out.set(p, (out.get(p) ?? 0) + 1);
    }
  }
  return out;
}

/** What links box `id`: arrows drawn at it, and for a box inside a group, its own arrows as written. */
export function linksOf(arrows: MapArrow[], id: string): MapArrow[] {
  const at = (a: MapArrow) => a.from === id || a.to === id;
  return arrows.flatMap((a) => (at(a) ? [a] : (a.parts ?? []).filter(at)));
}

const ARROW: Record<string, { color: string; dash: string | null; head: "filled" | "open" | "hollow" }> = {
  // ponytail: names only, the old looks (uses as imports did); each kind's own look comes with Task 6
  uses: { color: "var(--map-faint)", dash: null, head: "filled" },
  calls: { color: "var(--map-fg-2)", dash: null, head: "filled" },
  bundle: { color: "var(--map-faint)", dash: null, head: "filled" },
  sends: { color: "var(--map-purple)", dash: "2 5", head: "open" },
  triggers: { color: "var(--map-purple)", dash: "2 5", head: "open" },
  reads: { color: "var(--map-teal)", dash: null, head: "filled" },
  writes: { color: "var(--map-teal)", dash: null, head: "filled" },
  implements: { color: "var(--map-faint)", dash: "6 4", head: "hollow" },
};

export function arrowLook(arrow: MapArrow, marks: Mark[], on: Set<string>, by?: Proposal["by"]) {
  const base = ARROW[arrow.kind];
  const hit = worst(shown(marks, on, arrow.id).filter((m) => m.state !== "ok"));
  const s = hit ? STATE[hit.state as Exclude<MarkState, "ok">] : null;
  const o = by ? { color: TRACE, dash: "6 4" } : arrow.origin === "build" ? { color: TRACE, dash: "6 4" } : arrow.origin === "drift" ? { color: WARN, dash: "2 4" } : null;
  return {
    color: s?.edge ?? o?.color ?? base.color,
    // thicker the more edges it lifts: 1 → +0, 10 → +0.75, 100 and more → +1.5
    width: (s ? 2.5 : 1.5) + Math.min(1.5, 0.75 * Math.log10(Math.max(1, arrow.count))),
    dash: o?.dash ?? base.dash,
    head: base.head,
    label: arrow.label ?? hit?.badge ?? (by ? tag(by) : null) ?? (arrow.origin === "drift" ? "not in ig/" : null),
    tone: (s?.tone ?? "neutral") as Tone,
  };
}

export type ArrowLook = ReturnType<typeof arrowLook> & { strong?: boolean };

export function diffLook(look: ArrowLook, as: "added" | "ghost"): ArrowLook {
  return as === "ghost"
    ? { ...look, color: "var(--color-removed)", dash: "5 4", strong: true }
    : { ...look, color: "var(--color-added)", strong: true };
}

/** How many links an arrow stands for: a declared one has count 0 and is one. */
export const linkCount = (a: MapArrow) => Math.max(1, a.count);
