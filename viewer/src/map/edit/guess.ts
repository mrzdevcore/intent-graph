import type { ArrowKind, ArrowKindView, KindView } from "../types";

const RULES: ArrowKindView[] = [
  { name: "writes", from: [], to: ["store"] },
  { name: "sends", from: [], to: ["queue"] },
  { name: "triggers", from: [], to: ["job", "entry"] },
  { name: "implements", from: ["adapter"], to: ["port"] },
];
const ORDER: ArrowKind[] = ["writes", "sends", "triggers", "implements"];

export const baseOf = (name: string, kinds: KindView[]): string => {
  const k = kinds.find((x) => x.name === name);
  return k?.custom ? k.base : name;
};

export function guessArrow(fromKind: string, toKind: string, kinds: ArrowKindView[]): ArrowKind {
  const rules = kinds.length ? kinds : RULES;
  for (const name of ORDER) {
    const r = rules.find((k) => k.name === name);
    if (r && r.to.includes(toKind) && (!r.from.length || r.from.includes(fromKind))) return name;
  }
  return toKind === "box" ? "uses" : "calls";
}
