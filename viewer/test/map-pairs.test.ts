import { expect, test } from "vitest";
import { wires } from "../src/map/wires";
import { pairLook, spots } from "../src/map/labels";
import type { Frame, MapArrow, Mark } from "../src/map/types";

const arrow = (from: string, to: string, o: Partial<MapArrow> = {}): MapArrow => ({ id: `${from}->${to}`, parent: "p", from, to, kind: "calls", count: 1, kinds: ["calls"], ...o });
const frames: Record<string, Frame> = { api: { x: 0, y: 0, w: 200, h: 100 }, web: { x: 400, y: 0, w: 200, h: 100 }, db: { x: 0, y: 300, w: 200, h: 100 } };

test("two curved arrows between the same boxes share one line, drawn each way", () => {
  const as = [arrow("api", "web"), arrow("web", "api"), arrow("api", "db")];
  const w = wires(as, frames, "routed", {}, () => "curved");
  expect([...w.twins]).toEqual([["web->api", "api->web"]]);
  const [there, back] = [w.wires.get("api->web")!, w.wires.get("web->api")!];
  expect(back.ends).toEqual([there.ends[1], there.ends[0]]);
  expect(w.wires.get("api->db")).toBeDefined();
});

test("circuit arrows keep a line each", () => {
  const w = wires([arrow("api", "web"), arrow("web", "api")], frames, "routed");
  expect(w.twins.size).toBe(0);
});

test("a line's label: own labels joined, a risk as ⚠, and how many arrows it holds", () => {
  const risk: Mark = { target: "web->api", layer: "risks", state: "warn", badge: "risk: query in a loop" };
  const l = pairLook([arrow("api", "web", { label: "update" }), arrow("web", "api")], [risk], new Set(["risks"]));
  expect(l.label).toBe("⚠ update ×2");
  expect(l.tone).toBe("warn");
  expect(pairLook([arrow("api", "web")], [], new Set()).label).toBeNull();
  expect(pairLook([arrow("web", "api")], [risk], new Set(["risks"])).label).toBe("⚠ query in a loop");
});

test("a label slides along its line off a box and off the label placed before it", () => {
  const line = (y: number): [number, number][] => [[0, y], [400, y]];
  const at = spots([{ id: "a", pts: line(50), w: 80, h: 20 }, { id: "b", pts: line(50), w: 80, h: 20 }], [{ x: 160, y: 0, w: 80, h: 100 }]);
  const [a, b] = [at.get("a")!, at.get("b")!];
  expect(Math.abs(a[0] - 200)).toBeGreaterThanOrEqual(80);
  expect(Math.abs(a[0] - b[0])).toBeGreaterThanOrEqual(80);
});
