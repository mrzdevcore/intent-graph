// Pure helpers over design.json v1. Mirrors crates/ig-core/src/design (ids, graph ids, keys).

import type { Design, DesignBridge, DesignElement, DesignLink, ElementKind, LinkKind } from "../api";
import type { ViewNode } from "../data";

export type Element = DesignElement;
export type Link = DesignLink;
export type Bridge = DesignBridge;

/** `^[a-z][a-z0-9-]*$`: the one id rule for elements, screens and bridges. */
export function isElementId(s: string): boolean {
  return /^[a-z][a-z0-9-]*$/.test(s);
}

const PREFIX: Record<ElementKind, string> = { module: "mod", function: "fn", route: "route", system: "sys", note: "note", frame: "frame" };

/** Lowercase ASCII words joined by `-` (Rust: split on non-alphanumerics, drop empties). */
function kebab(name: string): string {
  return name.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join("-");
}

/** A fresh element id, exactly as Rust `new_id`: numbered when taken; notes and letterless names from 1. */
export function newId(d: Design, kind: ElementKind, name: string): string {
  const prefix = PREFIX[kind];
  const slug = kebab(name);
  const taken = (id: string) => d.elements.some((e) => e.id === id);
  if (kind === "note" || !slug) {
    for (let n = 1; ; n++) if (!taken(`${prefix}-${n}`)) return `${prefix}-${n}`;
  }
  const base = `${prefix}-${slug}`;
  if (!taken(base)) return base;
  for (let n = 2; ; n++) if (!taken(`${base}-${n}`)) return `${base}-${n}`;
}

/** The code-graph id an element stands for, if it has enough to say (Rust `graph_id`). */
export function graphId(e: Element): string | null {
  const file = e.file || null;
  switch (e.kind) {
    case "module": return file;
    case "function": return file && `${file}#${e.name}`;
    case "route": return e.method && e.path && file ? `${file}#${e.method} ${e.path}` : null;
    case "system": return `system:${e.name}`;
    default: return null;
  }
}

export const linkKey = (l: Link): string => `${l.from}|${l.kind}|${l.to}`;
export const bridgeKey = (b: Bridge): string => `${b.from}>${b.to}`;

function mostCommon(xs: string[]): string | null {
  const n = new Map<string, number>();
  xs.forEach((x) => n.set(x, (n.get(x) ?? 0) + 1));
  let best: string | null = null;
  n.forEach((c, x) => (best === null || c > n.get(best)!) && (best = x));
  return best;
}

/** Where a new element's file would go: the layer's most common directory and extension. */
export function proposeFile(nodes: ViewNode[], layer: string | null, _kind: ElementKind, name: string): string | null {
  const files = nodes.filter((n) => n.module && n.layer === layer).map((n) => n.file);
  if (!files.length) return null;
  const dir = mostCommon(files.map((f) => f.slice(0, f.lastIndexOf("/") + 1))) ?? "";
  const ext = mostCommon(files.map((f) => { const b = f.slice(f.lastIndexOf("/") + 1); return b.includes(".") ? b.slice(b.lastIndexOf(".")) : ""; })) ?? "";
  return dir + kebab(name) + ext;
}

export function defaultLinkKind(fromKind: string, toKind: string): LinkKind {
  if (toKind === "route") return "http";
  if (toKind === "system") return "uses";
  if (fromKind === "module" && toKind === "module") return "imports";
  return "calls";
}
