// @vitest-environment happy-dom
import { expect, test } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import Corner from "../src/map/edit/Corner.svelte";

test("the corner has a Fit button beside the readout, both fit the map", () => {
  let fits = 0;
  const target = document.createElement("div");
  const m = mount(Corner, { target, props: { k: 0.76, onzoom: () => {}, onfit: () => fits++ } });
  flushSync();
  const fit = [...target.querySelectorAll("button")].find((b) => b.textContent === "Fit")!;
  fit.click();
  (target.querySelector('[aria-label="Fit the map"]') as HTMLButtonElement).click();
  expect(fits).toBe(2);
  expect(target.textContent).toContain("76%");
  unmount(m);
});
