import { expect, test } from "vitest";
import { agentStep, save } from "../src/board/store";

test("agentStep groups arrivals up to 1 s after the turn ends", () => {
  const turns = [{ id: "t1", start: 100, end: 500 }, { id: "t2", start: 5000, end: null }];
  expect(agentStep(300, turns)).toBe("t1");
  expect(agentStep(1500, turns)).toBe("t1");
  expect(agentStep(1501, turns)).toBeNull();
  expect(agentStep(9000, turns)).toBe("t2");
  expect(agentStep(50, turns)).toBeNull();
});

test("save makes the written text the merge base, only after the write succeeds", async () => {
  let board = "v1";
  const saved: string[] = [];
  const engine = { board: () => board, saved: (t: string) => void saved.push(t) };
  // an edit lands while the write is in flight: the base is still what was written
  await save(engine, async () => void (board = "v2"));
  expect(saved).toEqual(["v1"]);
  await expect(save(engine, () => Promise.reject(new Error("disk full")))).rejects.toThrow("disk full");
  expect(saved).toEqual(["v1"]);
});
