import { expect, test } from "vitest";
import type { ViewData } from "../src/data";
import { index } from "../src/model/graph";
import { sequence, toMermaid, toPlantUml } from "../src/model/sequence";
import fixture from "./fixture-data.json";

test("Mermaid and PlantUML text", () => {
  const sq = sequence(index(fixture as ViewData), "apps/web/src/main.tsx#App");
  expect(sq.msgs.length).toBeGreaterThan(0);
  expect(toMermaid(sq)).toMatch(/^sequenceDiagram\n {2}participant p0 as main\n/);
  const pu = toPlantUml(sq);
  expect(pu).toMatch(/^@startuml\nparticipant "main" as p0\n/);
  expect(pu.endsWith("\n@enduml")).toBe(true);
});
