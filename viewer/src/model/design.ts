// Pure helpers over design.json v1 that stay on the TypeScript side (the rest is Rust:
// crates/ig-design). Mirrors its id rule.

import type { ElementKind } from "../api";
import type { ViewNode } from "../data";

/** `^[a-z][a-z0-9-]*$`: the one id rule for elements, screens and bridges. */
export function isElementId(s: string): boolean {
  return /^[a-z][a-z0-9-]*$/.test(s);
}

/** Lowercase ASCII words joined by `-` (Rust: split on non-alphanumerics, drop empties). */
function kebab(name: string): string {
  return name.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join("-");
}

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
