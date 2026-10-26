// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";
import Files from "../src/components/Files.svelte";

test("revealing a nested path lists each directory once, not in a loop", async () => {
  const calls: (string | undefined)[] = [];
  // past 100 calls a listing never answers, so a loop stops and the test can fail instead of hang
  const list = (dir?: string) => (calls.push(dir), calls.length > 100 ? new Promise<never>(() => {}) : Promise.resolve([]));
  const path = "micro/pdp/front/src/views/VPdp.vue";
  const c = mount(Files, { target: document.createElement("div"), props: { list, path, version: 1, onselect: () => {} } });
  flushSync();
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
  unmount(c);
  expect(calls.length).toBeLessThanOrEqual(6); // the root and the 5 ancestors
});
