import type { End } from "../types";

export type Notation = "crowsfoot" | "labels" | "both";

const LABELS: Record<End, string> = { one: "1", zeroOne: "0..1", many0: "0..n", many1: "1..n" };
const HALF = 7, FOOT = 14;

export const endLabel = (end: End): string => LABELS[end];

export function endMarks(end: End, at: [number, number], out: [number, number]): { lines: string; rings: [number, number][] } {
  const len = Math.hypot(out[0], out[1]);
  const [ox, oy] = len > 0 ? [out[0] / len, out[1] / len] : [1, 0];
  const [px, py] = [-oy, ox];
  const r = (n: number) => Math.round(n * 1000) / 1000;
  const pt = (d: number, s = 0) => `${r(at[0] + ox * d + px * s)} ${r(at[1] + oy * d + py * s)}`;
  const bar = (d: number) => `M${pt(d, HALF)}L${pt(d, -HALF)}`;
  const foot = `M${pt(FOOT)}L${pt(0, HALF)}M${pt(FOOT)}L${pt(0)}M${pt(FOOT)}L${pt(0, -HALF)}`;
  const ring = (d: number): [number, number] => [r(at[0] + ox * d), r(at[1] + oy * d)];
  switch (end) {
    case "many0": return { lines: foot, rings: [ring(21)] };
    case "many1": return { lines: foot + bar(18), rings: [] };
    case "one": return { lines: bar(8) + bar(13), rings: [] };
    case "zeroOne": return { lines: bar(8), rings: [ring(18)] };
  }
}
