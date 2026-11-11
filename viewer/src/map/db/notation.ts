import type { Notation } from "./ends";

const KEY = "ig-ends";

export function loadNotation(): Notation {
  try {
    const v = globalThis.window?.localStorage?.getItem(KEY);
    return v === "labels" || v === "both" ? v : "crowsfoot";
  } catch {
    return "crowsfoot";
  }
}

export function saveNotation(n: Notation) {
  try {
    if (n === "crowsfoot") globalThis.window?.localStorage?.removeItem(KEY);
    else globalThis.window?.localStorage?.setItem(KEY, n);
  } catch { /* private mode: the setting lasts the page */ }
}
