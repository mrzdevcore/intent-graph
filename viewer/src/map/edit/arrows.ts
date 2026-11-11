// Arrow kinds the kind chip offers.
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

export const keep = (a: MapArrow): Edit => ({ op: "arrow", from: a.from, to: a.to, kind: specKind(a), label: a.kind === "bundle" ? null : a.label ?? null });

export function onArrow(a: MapArrow, edits: Edit[]): Edit | null {
  const all = a.origin === "drift" ? [keep(a), ...edits] : edits;
  return all.length === 0 ? null : all.length === 1 && a.origin !== "drift" ? all[0] : { op: "batch", edits: all };
}

// as crates/ig-app spec::repr and set::context_format: the arrow as the writer would put it, at the top level
const DISALLOWED = /[\u200E\u200F\u202A-\u202E\u2066-\u2069\uFEFF\u2028\u2029]/;
const control = (c: string) => /[\u0000-\u001F\u007F-\u009F]/.test(c);
const hex = (c: string) => `\\u{${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}}`;
const ESC: Record<string, string> = { "\\": "\\\\", '"': '\\"', "\n": "\\n", "\r": "\\r", "\t": "\\t" };
export const kdlString = (s: string) => `"${[...s].map((c) => ESC[c] ?? (control(c) || DISALLOWED.test(c) ? hex(c) : c)).join("")}"`;

function contextKdl(v: string): string {
  const plain = (c: string) => (!control(c) || c === "\n" || c === "\t") && !DISALLOWED.test(c);
  const lines = v.split("\n");
  if (!v.includes("\n") || ![...v].every(plain) || !lines.every((l) => l === "" || l.trim() !== "")) return kdlString(v);
  const hashes = "#".repeat(1 + Math.max(0, ...v.split('"').slice(1).map((r) => r.match(/^#*/)![0].length)));
  const pad = "    ";
  return `${hashes}"""\n${lines.map((l) => (l ? pad + l : "")).join("\n")}\n${pad}"""${hashes}`;
}

export function arrowKdl(a: MapArrow): string {
  const label = a.kind === "bundle" ? null : a.label;
  return [specKind(a), kdlString(a.to), label != null && `label=${kdlString(label)}`, a.note != null && `note=${kdlString(a.note)}`,
    a.context != null && `context=${contextKdl(a.context)}`].filter(Boolean).join(" ");
}
