// The add-box form's rules: kinds by depth, the id slug (mirrors `slug` in crates/ig-app/src/map/facts/slug.rs),
// and the "code lives at" prefill.
import type { BoxKind } from "../types";

// ponytail: the brief's "service" is the Rust kind `api`
export const SYSTEM_KINDS: BoxKind[] = ["app", "api", "library", "worker", "store", "queue", "external", "person"];
export const CONTAINER_KINDS: BoxKind[] = ["usecase", "domain", "port", "folder", "component", "entry"];
/** Kinds that are not code: no "code lives at". */
export const NO_CODE: BoxKind[] = ["external", "person"];

const MAX_ID = 64;

/** `[a-z0-9-]`, runs of anything else as one `-`, trimmed, at most 64 chars; `fallback` when nothing is left. */
export function slug(s: string, fallback = "box"): string {
  const t = s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+/, "").slice(0, MAX_ID).replace(/-+$/, "");
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

/** The parent's first glob with its trailing `**` replaced by `<slug>/**`; `<slug>/**` when it has none. */
export function prefill(globs: string[], name: string): string {
  const s = name.trim() ? slug(name) : "";
  const g = globs[0];
  return g?.endsWith("**") ? `${g.slice(0, -2)}${s}/**` : `${s}/**`;
}
