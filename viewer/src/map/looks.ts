import type { BoxKind, MapArrow, MapBox, Mark, MarkState, View } from "./types";

export type Tone = "neutral" | "warn" | "bad" | "down";
export type Look = {
  edge: string; width: number; dashed: boolean; glow: string | null; bg: string; fade: number;
  chips: { label: string; tone: Tone }[]; dot: boolean;
};

const WARN = "#d08a00", BAD = "#c0392b", DOWN = "#8a8378", OK = "#2f8f5b", NEUTRAL = "#a39b8e";
const KIND: Partial<Record<BoxKind, { edge: string; dashed?: boolean }>> = {
  entry: { edge: "#3b6fd8" }, usecase: { edge: OK }, port: { edge: OK, dashed: true }, domain: { edge: "#b8860b" },
};
const STATE: Record<Exclude<MarkState, "ok">, { edge: string; tone: Tone; bg: string }> = {
  warn: { edge: WARN, tone: "warn", bg: "#fff6e0" },
  bad: { edge: BAD, tone: "bad", bg: "#fdf0ee" },
  down: { edge: DOWN, tone: "down", bg: "#f1efeb" },
};
const RANK: Record<MarkState, number> = { ok: 0, warn: 1, down: 2, bad: 3 };
const worst = (ms: Mark[]) => ms.reduce<Mark | null>((w, m) => (!w || RANK[m.state] > RANK[w.state] ? m : w), null);
const shown = (marks: Mark[], on: Set<string>, target: string) => marks.filter((m) => m.target === target && on.has(m.layer));

/** Precedence: health > highlight (other layers' marks) > status (stale) > kind. */
export function look(id: string, marks: Mark[], on: Set<string>, box: MapBox): Look {
  const mine = shown(marks, on, id);
  const health = worst(mine.filter((m) => m.layer === "health"));
  const other = worst(mine.filter((m) => m.layer !== "health" && m.state !== "ok"));
  const kind = KIND[box.kind.value];
  const hit = health && health.state !== "ok" ? health : other;
  const s = hit && hit.state !== "ok" ? STATE[hit.state] : null;
  const chips = mine.filter((m) => m.state !== "ok" && (m.badge ?? m.value)).map((m) => ({
    label: (m.badge ?? m.value)!, tone: STATE[m.state as Exclude<MarkState, "ok">].tone,
  }));
  if (box.stale) chips.push({ label: "stale", tone: "neutral" });
  return {
    edge: s?.edge ?? kind?.edge ?? NEUTRAL,
    width: s && s.tone !== "down" ? 2.5 : 1.5,
    dashed: s ? s.tone === "down" : !!(box.stale || kind?.dashed),
    glow: s && s.tone !== "down" ? s.edge : null,
    bg: s?.bg ?? "#fffdf8",
    fade: s?.tone === "down" || (!s && box.stale) ? 0.6 : 1,
    chips,
    dot: !!health && health.state === "ok" && !s,
  };
}

/** Bad and Down marks under each box (on its descendants and their arrows), for the "N inside" badge. */
export function insideCounts(view: View, marks: Mark[], on: Set<string>): Map<string, number> {
  const parent = new Map(view.boxes.map((b) => [b.id, b.parent]));
  const home = new Map<string, string | undefined>(view.arrows.map((a) => [a.id, a.parent]));
  const out = new Map<string, number>();
  for (const m of marks) {
    if (!on.has(m.layer) || (m.state !== "bad" && m.state !== "down")) continue;
    for (let p = parent.has(m.target) ? parent.get(m.target) : home.get(m.target); p; p = parent.get(p))
      out.set(p, (out.get(p) ?? 0) + 1);
  }
  return out;
}

const ARROW: Record<string, { color: string; dash: string | null; head: "filled" | "open" | "hollow" }> = {
  uses: { color: "#3a3631", dash: null, head: "filled" },
  calls: { color: "#3a3631", dash: null, head: "filled" },
  depends: { color: "#8a8378", dash: null, head: "filled" },
  bundle: { color: "#8a8378", dash: null, head: "filled" },
  network: { color: "#3b6fd8", dash: "6 4", head: "filled" },
  event: { color: "#7b4fc9", dash: "2 5", head: "open" },
  data: { color: "#1f7a8c", dash: null, head: "filled" },
  implements: { color: "#8a8378", dash: "6 4", head: "hollow" },
};

export function arrowLook(arrow: MapArrow, marks: Mark[], on: Set<string>) {
  const base = ARROW[arrow.kind];
  const hit = worst(shown(marks, on, arrow.id).filter((m) => m.state !== "ok"));
  const s = hit ? STATE[hit.state as Exclude<MarkState, "ok">] : null;
  return {
    color: s?.edge ?? base.color,
    width: s ? 2.5 : 1.5,
    dash: base.dash,
    head: base.head,
    label: arrow.label ?? hit?.badge ?? null,
    tone: (s?.tone ?? "neutral") as Tone,
  };
}
