import cytoscape from "cytoscape";
import { expect, test } from "vitest";
import { applyLinks } from "../src/model/graph";

test("hidden links: every edge gets nolink except the selected node's", () => {
  const cy = cytoscape({ headless: true, elements: [
    { data: { id: "a" } }, { data: { id: "b" } }, { data: { id: "c" } },
    { data: { id: "ab", source: "a", target: "b" } }, { data: { id: "bc", source: "b", target: "c" } },
  ] });
  const hidden = () => cy.edges(".nolink").map((e) => e.id()).sort();
  applyLinks(cy, false);
  expect(hidden()).toEqual(["ab", "bc"]);
  cy.getElementById("a").select();
  applyLinks(cy, false);
  expect(hidden()).toEqual(["bc"]);
  applyLinks(cy, true);
  expect(hidden()).toEqual([]);
});
