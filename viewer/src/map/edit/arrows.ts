// Arrow kinds the kind chip offers, and what an arrow on the map lets you change.
import type { ArrowKind, ArrowKindView, Edit, MapArrow, MapBox } from "../types";
import { written } from "./menu";

export const ARROW_KINDS: ArrowKind[] = ["calls", "sends", "reads", "writes", "uses", "implements", "triggers"];

export const arrowKindNames = (kinds?: ArrowKindView[]): ArrowKind[] => (kinds?.length ? kinds.map((k) => k.name) : ARROW_KINDS);

// as crates/ig-app TELLING: what a bundle is written as in ig/
const TELLING: ArrowKind[] = ["calls", "sends", "reads", "writes", "triggers", "implements", "uses"];
export const specKind = (a: MapArrow): ArrowKind => (a.kind !== "bundle" ? a.kind : TELLING.find((k) => a.kinds.includes(k)) ?? "uses");

export type Editing = "edit" | "drift" | "bundle" | "none";
export function editing(a: MapArrow, box: (id: string) => MapBox | undefined): Editing {
  if (a.parts?.length) return "bundle";
  if (a.ghost || a.origin === "unmanaged" || !written(box(a.from)) || !written(box(a.to))) return "none";
  return a.origin === "drift" ? "drift" : "edit";
}

/** The arrow written into ig/ as it is drawn: what a drift arrow's first change goes with. */
export const keep = (a: MapArrow): Edit => ({ op: "arrow", from: a.from, to: a.to, kind: specKind(a), label: a.kind === "bundle" ? null : a.label ?? null });

/** `edits` on arrow `a`; a drift arrow is written first, in the same batch. */
export function onArrow(a: MapArrow, edits: Edit[]): Edit | null {
  const all = a.origin === "drift" ? [keep(a), ...edits] : edits;
  return all.length === 0 ? null : all.length === 1 && a.origin !== "drift" ? all[0] : { op: "batch", edits: all };
}
