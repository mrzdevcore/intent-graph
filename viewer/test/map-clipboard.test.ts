import { expect, test } from "vitest";
import type { Edit, View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";
import { copy, fromKdl, paste, toKdl } from "../src/map/edit/clipboard";

const fresh = () => structuredClone(shop) as unknown as View;
const places = (v: View) => ({ ...v.placed.frames, "web/": { x: 0, y: 0, w: 440, h: 344 }, "api/": { x: 0, y: 0, w: 2304, h: 1232 } });
const edits = (e: Edit) => (e.op === "batch" ? e.edits : [e]);
const adds = (e: Edit) => edits(e).flatMap((x) => (x.op === "add" ? [x] : []));

function described() {
  const v = fresh(), api = v.boxes.find((b) => b.id === "api")!;
  api.tech = { value: "Node · express", source: "ig" };
  api.context = { value: "Keep it\nstateless", source: "ig" };
  api.props = { owner: "payments" };
  const a = v.arrows.find((x) => x.id === "api/application-checkout->api/infra-prisma")!;
  Object.assign(a, { note: "once per order", context: "in one transaction" });
  return v;
}

test("boxes alone: kind, title, own properties, context and size; no children, no arrows", () => {
  const v = described(), c = copy(["api", "postgres"], v, false, places(v));
  expect(c.boxes.map((b) => b.id)).toEqual(["api", "postgres"]);
  expect(c.boxes[0]).toMatchObject({ kind: "app", title: "Api", props: { tech: "Node · express", owner: "payments" }, context: "Keep it\nstateless", place: v.placed.frames.api });
  expect(c.arrows).toEqual([{ from: "api", to: "postgres", kind: "reads" }]);
  expect(c.parent).toBe("system");
});

test("with what's inside: the children and the arrows between copied boxes, with note and context; arrows out are dropped", () => {
  const v = described(), c = copy(["api"], v, true, places(v));
  expect(c.boxes.map((b) => b.id)).toContain("api/routers");
  expect(c.boxes.find((b) => b.id === "api/routers")!.parent).toBe("api");
  expect(c.boxes.some((b) => b.id.includes("#"))).toBe(false);
  expect(c.arrows.some((a) => a.to === "postgres" || a.to === "stripe" || a.from === "web")).toBe(false);
  expect(c.arrows).toContainEqual({ from: "api/application-checkout", to: "api/infra-prisma", kind: "calls", label: "save", note: "once per order", context: "in one transaction" });
  expect(c.arrows.some((a) => a.kind === "bundle")).toBe(false);
  expect(c.boxes[0].room).toEqual({ x: 0, y: 0, w: 2304, h: 1232 });
});

test("paste: one batch of adds and arrows, fresh ids, no at, titles get ' copy' next to the original", () => {
  const v = described(), c = copy(["api"], v, true, places(v));
  const { batch, ids, places: at } = paste(c, "system", { x: 600, y: 600 }, v);
  expect(batch.op).toBe("batch");
  const a = adds(batch);
  expect(a[0]).toEqual({ op: "add", parent: "system", kind: "app", id: "api-2", title: "Api copy", at: [], before: null, props: { tech: "Node · express", owner: "payments" } });
  expect(a.find((x) => x.title === "Routers")).toMatchObject({ parent: "api-2", id: "api-2/routers" });
  expect(a.every((x) => x.at.length === 0)).toBe(true);
  expect(edits(batch)).toContainEqual({ op: "set", id: "api-2", key: "context", value: "Keep it\nstateless" });
  expect(edits(batch)).toContainEqual({ op: "arrow", from: "api-2/application-checkout", to: "api-2/infra-prisma", kind: "calls", label: "save" });
  expect(edits(batch)).toContainEqual({ op: "arrowset", from: "api-2/application-checkout", to: "api-2/infra-prisma", key: "note", value: "once per order" });
  expect(edits(batch)).toContainEqual({ op: "arrowset", from: "api-2/application-checkout", to: "api-2/infra-prisma", key: "context", value: "in one transaction" });
  const i = edits(batch).findIndex((x) => x.op === "arrow"), last = edits(batch).map((x) => x.op).lastIndexOf("add");
  expect(i).toBeGreaterThan(last);
  expect(ids).toEqual(["api-2"]);
  const p = at["api-2"];
  expect([p.w, p.h]).toEqual([v.placed.frames.api.w, v.placed.frames.api.h]);
  expect(Math.abs(p.x + p.w / 2 - 600)).toBeLessThanOrEqual(24);
  expect(at["api-2/routers"]).toEqual(v.placed.frames["api/routers"]);
  expect(at["api-2/"]).toEqual({ x: 0, y: 0, w: 2304, h: 1232 });
});

test("paste again takes the next free id; into another map the ids and titles stay", () => {
  const v = fresh(), c = copy(["postgres"], v, false, places(v));
  v.boxes.push({ ...v.boxes.find((b) => b.id === "postgres")!, id: "postgres-2" });
  expect(adds(paste(c, "system", null, v).batch)[0]).toMatchObject({ id: "postgres-3", title: "Postgres copy" });
  const other = fresh();
  other.boxes = other.boxes.filter((b) => b.id !== "postgres");
  expect(adds(paste(c, "system", null, other).batch)[0]).toMatchObject({ id: "postgres", title: "Postgres" });
  expect(adds(paste(c, "web", null, fresh()).batch)[0]).toMatchObject({ id: "postgres-2", title: "Postgres" });
});

test("pasted where the copy came from, off the map: beside the original, at the same height", () => {
  const v = fresh(), c = copy(["postgres", "stripe"], v, false, places(v));
  const { places: at, ids } = paste(c, "system", null, v);
  const f = v.placed.frames, l = Math.min(f.postgres.x, f.stripe.x), r = Math.max(f.postgres.x + f.postgres.w, f.stripe.x + f.stripe.w);
  expect(ids).toEqual(["postgres-2", "stripe-2"]);
  expect(at["postgres-2"].x - f.postgres.x).toBe(r - l + 48);
  expect(at["postgres-2"].y).toBe(f.postgres.y);
  expect(at["stripe-2"].x - at["postgres-2"].x).toBe(f.stripe.x - f.postgres.x);
});

test("KDL round trip: the copy as ig/ text reads back the same, places aside", () => {
  const v = described(), c = copy(["api", "postgres"], v, true, places(v));
  const text = toKdl(c);
  expect(text).toMatch(/^app "api" title="Api" tech="Node · express" owner="payments" \{\n    context "Keep it\\nstateless"\n    reads "postgres"\n/);
  expect(text).not.toMatch(/\bat "/);
  const back = fromKdl(text, v.kinds)!;
  const bare = c.boxes.map(({ place: _p, room: _r, ...b }) => b);
  expect(back.boxes).toEqual(bare);
  expect(back.arrows).toEqual(expect.arrayContaining(c.arrows));
  expect(back.arrows).toHaveLength(c.arrows.length);
});

test("KDL copied from an ig/ file: comments, raw strings, old names; at, badges and pattern left out; arrows out dropped", () => {
  const text = `// ig's intended architecture
service "api" title="API" tech="Rust · axum" {
    at "crates/api/**"
    pattern "hexagonal"
    badges "beta"
    context #"""
        Answers the web app.
          Keeps no state.
        """#
    domain "api/orders" title="Orders" slot="core" { at "crates/api/src/orders/**" }
    port "api/payments" title="Payments" slot="ports"
    adapter "api/stripe" title="Stripe client" /* the only one */ {
        implements "api/payments" note="checked at boot"
        calls "stripe"
    }
    writes "db" label="reads and writes"
    depends "api/orders"
}
folder "tools" title=r"Tools"
`;
  const c = fromKdl(text, fresh().kinds)!;
  expect(c.boxes.map((b) => [b.id, b.kind, b.parent])).toEqual([
    ["api", "service", null], ["api/orders", "domain", "api"], ["api/payments", "port", "api"], ["api/stripe", "adapter", "api"], ["tools", "group", null],
  ]);
  expect(c.boxes[0]).toMatchObject({ title: "API", props: { tech: "Rust · axum" }, context: "Answers the web app.\n  Keeps no state." });
  expect(c.boxes[1].props).toEqual({});
  expect(c.boxes[4].title).toBe("Tools");
  expect(c.arrows).toEqual([{ from: "api/stripe", to: "api/payments", kind: "implements", note: "checked at boot" }, { from: "api", to: "api/orders", kind: "uses" }]);
  const { batch } = paste(c, "system", { x: 0, y: 0 }, fresh());
  expect(adds(batch).map((a) => a.id)).toEqual(["api-2", "api-2/orders", "api-2/payments", "api-2/stripe", "tools"]);
});

test("text that is not ig/ boxes reads as nothing", () => {
  const kinds = fresh().kinds;
  expect(fromKdl("hello world", kinds)).toBeNull();
  expect(fromKdl("", kinds)).toBeNull();
  expect(fromKdl('service "api" title="API" {', kinds)).toBeNull();
  expect(fromKdl('system "shop" title="Shop"', kinds)).toBeNull();
  expect(fromKdl("Just some words, with a comma.", kinds)).toBeNull();
});
