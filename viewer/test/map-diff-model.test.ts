import { expect, test } from "vitest";
import { diffAnswer } from "../src/map/diff";
import type { Diff, ArrowRef, BoxRef, ChangedBox } from "../src/model/diff";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = () => structuredClone(shop) as unknown as View;
const box = (id: string, parent: string | null, title = id): BoxRef => ({ id, title, parent });
const changed = (id: string, parent: string, n: number): ChangedBox => ({ ...box(id, parent), files: [], filesChanged: n, direct: true });
const arrow = (from: string, to: string): ArrowRef => ({ from, to, kind: "calls", fromTitle: from, toTitle: to });

function diff(over: Partial<{ added: BoxRef[]; removed: BoxRef[]; changed: ChangedBox[]; arrowsAdded: ArrowRef[]; arrowsRemoved: ArrowRef[] }> = {}, extra: Partial<Diff> = {}): Diff {
  return {
    base: "a".repeat(40), head: null, source: "code", summary: "",
    boxes: { added: over.added ?? [], removed: over.removed ?? [], changed: over.changed ?? [] },
    arrows: { added: over.arrowsAdded ?? [], removed: over.arrowsRemoved ?? [] },
    intent: { status: "same", added: [], removed: [], moved: [], changed: [], arrowsAdded: [], arrowsRemoved: [] },
    problems: { new: [], fixed: [] },
    ...extra,
  };
}

test("steps follow ig/, problems, boxes by size, then arrows", () => {
  const d = diff({
    changed: [changed("api/domain-order", "api", 3), changed("web/pages", "web", 10)],
    added: [box("api/routers", "api")],
    removed: [box("api/gone", "api")],
    arrowsAdded: [arrow("api/domain-order", "api/infra-prisma"), arrow("api/domain-order", "api/application-ports")],
  }, {
    intent: { status: "changed", added: [], removed: [], moved: [], changed: [{ id: "api", fields: ["title"] }], arrowsAdded: [], arrowsRemoved: [] },
    problems: { new: [{ kind: "rule", text: "domain → infrastructure", boxes: ["api/domain-order"] }], fixed: [] },
  });
  const { answer } = diffAnswer(view(), d);
  expect(answer.steps.map((s) => s.group)).toEqual(["ig/", "New problems", "Boxes", "Boxes", "Boxes", "Boxes", "Arrows"]);
  expect(answer.steps.slice(2, 6).map((s) => s.target)).toEqual(["web/pages", "api/domain-order", "api/routers", "api/gone"]);
  expect(answer.steps.slice(2, 6).map((s) => s.kind)).toEqual(["changed", "changed", "added", "removed"]);
  expect(answer.steps[3].meta).toBe("3 files");
  expect(answer.steps[6].title).toBe("api/domain-order → 2 boxes");
  expect(answer.title).toBe("What this change does");
});

test("a change inside a changed box folds into its parent's step", () => {
  const d = diff({ changed: [changed("api", "system", 12), changed("api/domain-order", "api", 3)], added: [box("api/new", "api", "New")] });
  const { answer, marks } = diffAnswer(view(), d);
  expect(answer.steps.map((s) => s.target)).toEqual(["api"]);
  expect(answer.steps[0].meta).toBe("12 files · + New");
  expect(marks.boxes.get("api/domain-order")).toBe("changed");
  expect(marks.files.get("api/domain-order")).toBe(3);
});

test("a removed box is counted on the nearest ancestor on the map", () => {
  const d = diff({ removed: [box("api/x/y/z", "api/x/y"), box("api/x/y", "api/x"), box("api/x", "api")] });
  const { answer, marks } = diffAnswer(view(), d);
  expect(marks.removedHere.get("api")!.map((b) => b.id).sort()).toEqual(["api/x", "api/x/y", "api/x/y/z"]);
  expect(answer.steps.map((s) => s.target)).toEqual(["api/x"]);
  expect(answer.steps[0].boxes).toEqual(["api"]);
  expect(answer.boxes.has("api")).toBe(true);
});

test("a removed arrow with both ends present becomes a ghost, one with an end gone does not", () => {
  const d = diff({ arrowsRemoved: [arrow("api/root", "api/domain-order"), arrow("api/gone", "api/domain-order")] });
  const { view: v, marks } = diffAnswer(view(), d);
  const ghosts = v.arrows.filter((a) => a.ghost);
  expect(ghosts.map((a) => [a.from, a.to, a.parent])).toEqual([["api/root", "api/domain-order", "api"]]);
  expect(marks.ghostIds).toEqual(new Set([ghosts[0].id]));
});

test("arrows between nested boxes match the drawn arrow", () => {
  const base = view();
  const top = base.arrows.find((a) => a.id === "web->api")!;
  top.parts = [{ ...top, id: "web/pages->api/routers", from: "web/pages", to: "api/routers", parent: "system" }];
  const d = diff({ arrowsAdded: [arrow("web/pages", "api/routers")], arrowsRemoved: [arrow("web/pages", "api/domain-order")] });
  const { view: v, marks } = diffAnswer(base, d);
  expect(marks.addedArrows.has(JSON.stringify(["web", "api"]))).toBe(true);
  const ghost = v.arrows.find((a) => a.ghost)!;
  expect([ghost.from, ghost.to, ghost.parent]).toEqual(["web", "api", "system"]);
});

test("no change: no steps, nothing marked", () => {
  const { answer, marks, view: v } = diffAnswer(view(), diff());
  expect(answer.steps).toEqual([]);
  expect(marks.boxes.size).toBe(0);
  expect(answer.boxes.size).toBe(0);
  expect(v.arrows.some((a) => a.ghost)).toBe(false);
});

test("the ends of a changed arrow are part of the change", () => {
  const { answer } = diffAnswer(view(), diff({ arrowsRemoved: [arrow("api/root", "api/domain-order")] }));
  expect([...answer.boxes].sort()).toEqual(["api/domain-order", "api/root"]);
});

test("the root of the map gets no step of its own", () => {
  const d = diff({ changed: [{ ...changed("system", "x", 192), parent: null }, changed("api", "system", 12)] });
  const { answer, marks } = diffAnswer(view(), d);
  expect(answer.steps.map((s) => s.target)).toEqual(["api"]);
  expect(marks.boxes.has("system")).toBe(false);
});
