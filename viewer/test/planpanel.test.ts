// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";
import { registerBoard } from "../src/model/boardselect";
import PlanPanel from "../src/components/PlanPanel.svelte";
import type { ViewData } from "../src/data";

test("plan versus design: three lists with counts, each id selects", () => {
  const gap = { ruleId: "plan/design-gap", severity: "error", message: "m", tool: "ig", file: "p", line: 0, nodeId: "fn-a", fingerprint: "f" };
  const data = {
    root: "/p", editor: "vscode://file{file}:{line}",
    plan: { title: "T", steps: [], done: false, volume: null, findings: [gap], design: { gaps: 1, omitted: [{ id: "fn-b", reason: "later" }], added: ["x.ts#y", "z.ts"] } },
  } as unknown as ViewData;
  const picked: string[] = [];
  const el = document.createElement("div");
  const c = mount(PlanPanel, { target: el, props: { data, known: {}, notes: [], focus: (id: string) => picked.push(id) } });
  flushSync();
  const t = el.textContent!;
  for (const h of ["Not in the plan (1)", "Left out (1)", "Added by the agent (2)", "fn-b later", "m"]) expect(t).toContain(h);
  el.querySelector<HTMLButtonElement>(".design-list button")!.click();
  expect(picked).toEqual(["fn-a"]);
  // with a board open the id selects there instead
  const onBoard: string[][] = [];
  const off = registerBoard((ids) => (onBoard.push(ids), ids[0] !== "fn-b"));
  el.querySelector<HTMLButtonElement>(".design-list button")!.click();
  expect([onBoard, picked]).toEqual([[["fn-a"]], ["fn-a"]]);
  el.querySelectorAll<HTMLButtonElement>(".design-list button")[1].click(); // fn-b: not on the board
  expect(picked).toEqual(["fn-a", "fn-b"]);
  picked.pop();
  off();
  el.querySelector<HTMLButtonElement>(".design-list button")!.click();
  expect(picked).toEqual(["fn-a", "fn-a"]);
  unmount(c);
});
