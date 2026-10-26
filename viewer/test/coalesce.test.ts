import { expect, test } from "vitest";
import { coalesce } from "../src/model/coalesce";

test("a burst runs the job once now and once after, never two at a time", async () => {
  let runs = 0;
  let inFlight = 0;
  let maxInFlight = 0;
  const done: (() => void)[] = [];
  const trigger = coalesce(() => {
    runs++;
    maxInFlight = Math.max(maxInFlight, ++inFlight);
    return new Promise<void>((r) => done.push(() => (inFlight--, r())));
  });
  for (let i = 0; i < 50; i++) trigger();
  expect(runs).toBe(1);
  done.shift()!();
  await new Promise((r) => setTimeout(r, 0));
  expect(runs).toBe(2); // the one pending run for the 49 that came meanwhile
  done.shift()!();
  await new Promise((r) => setTimeout(r, 0));
  expect(runs).toBe(2);
  expect(maxInFlight).toBe(1);
});
