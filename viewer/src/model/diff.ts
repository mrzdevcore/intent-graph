// `ig diff --json` (crates/ig-app/src/map/diff.rs), as the page carries it in #ig-diff.

export type BoxRef = { id: string; title: string; parent: string | null };
export type ChangedBox = BoxRef & { files: string[]; filesChanged: number; direct: boolean };
export type ArrowRef = { from: string; to: string; kind: string; fromTitle: string; toTitle: string };
export type Problem = { kind: "rule" | "drift"; text: string; boxes: string[] };
export type IntentStatus = "same" | "changed" | "added" | "removed" | "baseBroken";
export type Intent = {
  status: IntentStatus;
  note?: string;
  added: BoxRef[];
  removed: BoxRef[];
  moved: { id: string; from: string | null; to: string | null }[];
  changed: { id: string; fields: string[] }[];
  arrowsAdded: ArrowRef[];
  arrowsRemoved: ArrowRef[];
};
export type Diff = {
  base: string;
  head: string | null;
  source: "spec" | "code";
  summary: string;
  boxes: { added: BoxRef[]; removed: BoxRef[]; changed: ChangedBox[] };
  arrows: { added: ArrowRef[]; removed: ArrowRef[] };
  intent: Intent;
  problems: { new: Problem[]; fixed: Problem[] };
};

const isObj = (x: unknown): x is Record<string, unknown> => !!x && typeof x === "object" && !Array.isArray(x);
const list = <T,>(x: unknown): T[] => (Array.isArray(x) ? (x as T[]) : []);
const has = (o: Record<string, unknown>, ...keys: string[]) => keys.every((k) => Array.isArray(o[k]));

export function parseDiff(raw: unknown): Diff | null {
  if (!isObj(raw) || typeof raw.base !== "string" || typeof raw.summary !== "string") return null;
  const { boxes, arrows } = raw;
  if (!isObj(boxes) || !has(boxes, "added", "removed", "changed") || !isObj(arrows) || !has(arrows, "added", "removed")) return null;
  const i = isObj(raw.intent) ? raw.intent : {};
  const p = isObj(raw.problems) ? raw.problems : {};
  return {
    base: raw.base,
    head: typeof raw.head === "string" ? raw.head : null,
    source: raw.source === "spec" ? "spec" : "code",
    summary: raw.summary,
    boxes: { added: list(boxes.added), removed: list(boxes.removed), changed: list(boxes.changed) },
    arrows: { added: list(arrows.added), removed: list(arrows.removed) },
    intent: {
      status: (typeof i.status === "string" ? i.status : "same") as IntentStatus,
      ...(typeof i.note === "string" ? { note: i.note } : {}),
      added: list(i.added), removed: list(i.removed), moved: list(i.moved), changed: list(i.changed),
      arrowsAdded: list(i.arrowsAdded), arrowsRemoved: list(i.arrowsRemoved),
    },
    problems: { new: list(p.new), fixed: list(p.fixed) },
  };
}
