import { expect, test } from "vitest";
import { ends, find, trace } from "../src/map/search";
import type { View } from "../src/map/types";
import shop from "./fixtures/shop-view.json";

const view = shop as unknown as View;

test("a query is two ends when it says A to B", () => {
  expect(ends("web to stripe")).toEqual(["web", "stripe"]);
  expect(ends("Web -> Stripe")).toEqual(["Web", "Stripe"]);
  expect(ends("web → postgres")).toEqual(["web", "postgres"]);
  expect(ends("stripe")).toBeNull();
  expect(ends("to stripe")).toBeNull();
});

test("find: titles first, then kind or tech, then file paths, with where each box is", () => {
  const hits = find(view, "stri");
  expect(hits.slice(0, 2).map((h) => h.box.id).sort()).toEqual(["api/infra-stripe", "stripe"]);
  expect(hits.find((h) => h.box.id === "api/infra-stripe")!.where).toBe("Api");
  const byPath = find(view, "pay-gateway");
  expect(byPath[0].path).toBe("apps/api/src/infra/stripe/pay-gateway.ts");
  expect(find(view, "  ")).toEqual([]);
});

test("trace: the chain of arrows, lifted to where both ends meet", () => {
  const t = trace(view, "web", "stripe")!;
  expect(t).toMatchObject({ parent: "system", boxes: ["web", "api", "stripe"], reversed: false });
  expect(t.arrows.map((a) => a.id)).toEqual(["web->api", "api->stripe"]);
  // a deep box stands for its box at the level where the ends meet
  expect(trace(view, "apps/web/pages/checkout.tsx#CheckoutPage", "postgres")!.boxes).toEqual(["web", "api", "postgres"]);
  expect(trace(view, "api/routers", "api/infra-prisma")!.boxes).toEqual(["api/routers", "api/infra-prisma"]);
});

test("trace: the other way round when only that joins them; none when nothing does", () => {
  expect(trace(view, "postgres", "web")).toMatchObject({ boxes: ["web", "api", "postgres"], reversed: true });
  expect(trace(view, "postgres", "stripe")).toBeNull();
  // one holds the other
  expect(trace(view, "api", "api/routers")).toBeNull();
});
