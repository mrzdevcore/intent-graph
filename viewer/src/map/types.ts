// Mirrors `ig view --json` (crates/ig-app/src/map). Test fixture: test/fixtures/shop-view.json.
export type Source = "ig" | "agent" | "person";
export type Confidence = "extracted" | "inferred" | "ambiguous";
export type Field<T> = { value: T; source: Source; confidence?: Confidence };
export type Level = "system" | "container" | "component" | "code";
export type BoxKind =
  | "person" | "system" | "external" | "app" | "service" | "job" | "store" | "queue" | "model"
  | "module" | "entry" | "usecase" | "port" | "adapter" | "agent" | "domain" | "group"
  | "folder" | "file" | "component" | "parts" | "task" | "function" | "class" | "method";
export type Pattern =
  | { kind: "onion" } | { kind: "hexagonal" } | { kind: "none" } | { kind: "layered"; bands: string[] };
export type MapBox = {
  id: string;
  parent?: string;
  level: Level;
  madeOf: string[];
  madeOfSource?: Source;
  package?: string;
  files: number;
  kind: Field<BoxKind>;
  title: Field<string>;
  tech?: Field<string>;
  description?: Field<string>;
  logo?: Field<string>;
  badges?: Field<string[]>;
  pattern?: Field<Pattern>;
  slot?: Field<string>;
  serviceName?: string;
  pinned?: string[];
  needsReading?: boolean;
  stale?: boolean;
  /** Against ig/: "build" is in the spec with no code yet, "drift" is code the spec does not describe, "moved" is in
   * the spec at a new place with its code still at the old; absent is built. */
  origin?: "build" | "drift" | "moved";
  /** A team kind's name (`ig/kinds/<name>.kdl`); `kind` is then its base. */
  custom?: string;
  /** The kind's own properties (`provider`, `hosted`…) and `owner`. */
  props?: Record<string, string>;
};
/** One of `View.kinds`: a kind a box in ig/ may be, from crates/ig-app/assets/kinds.kdl or ig/kinds/. */
export type KindView = {
  name: string;
  base: BoxKind;
  custom: boolean;
  title: string;
  means: string;
  icon: string;
  /** False for a kind that is not code (person, external): no `at`. */
  code: boolean;
  role: "root" | "run" | "design" | "group";
  props: { name: string; means: string; required: boolean; oneOf: string[] }[];
};
export type ArrowKind = "calls" | "sends" | "reads" | "writes" | "uses" | "implements" | "triggers" | "bundle";
export type MapArrow = {
  id: string;
  parent: string;
  from: string;
  to: string;
  kind: ArrowKind;
  label?: string;
  count: number;
  kinds: ArrowKind[];
  inLoop?: boolean;
  origin?: "build" | "drift";
};
export type MarkState = "ok" | "warn" | "bad" | "down";
export type Mark = { layer: string; target: string; state: MarkState; badge?: string; value?: string; note?: string };
/** A box's place, in its parent's own inner units. */
export type Frame = { x: number; y: number; w: number; h: number };
export type Region = { parent: string; kind: "ring" | "band" | "group"; label: string; frame: Frame };
export type Placed = {
  frames: Record<string, Frame>;
  inner: Record<string, [number, number]>;
  regions: Region[];
};
/** One declared part of a UI component: `kind` says how (prop, emit, ref, computed, useState…). */
export type Part = { name: string; kind: string; line: number; detail?: string };
/** A piece of a file: `text` from line `line` on, `cut` lines left out at its end. */
export type Snippet = { file: string; line: number; text: string; cut?: number; complexity: number };
export type View = {
  version: number;
  boxes: MapBox[];
  arrows: MapArrow[];
  marks: Mark[];
  placed: Placed;
  /** The code of each function, class or method box, by box id. */
  sources?: Record<string, Snippet>;
  /** The list each part box of a component holds (inputs, state, uses, renders), by box id. */
  parts?: Record<string, Part[]>;
  notices: string[];
  /** What reads but looks wrong in ig/ (crowded boxes, arrows the kinds do not expect…). */
  warnings: string[];
  kinds: KindView[];
  /** The journal's entries nobody kept yet. */
  proposals?: Proposal[];
};
export type Proposal = { n: number; by: "claude" | "reading"; summary: string; ids: string[]; marks: string[] };

/** Mirrors `Edit` in crates/ig-app/src/map/edit.rs. */
export type Edit =
  | { op: "add"; parent: string; kind: string; id: string; title: string; at: string[]; before: string | null; props?: Record<string, string> }
  | { op: "remove"; id: string }
  | { op: "move"; id: string; parent: string; before: string | null; slot?: string }
  | { op: "rename"; id: string; to: string }
  | { op: "set"; id: string; key: string; value: string | null }
  | { op: "arrow"; from: string; to: string; kind: ArrowKind; label: string | null }
  | { op: "unarrow"; from: string; to: string; kind: ArrowKind }
  | { op: "restore"; parent: string; before: string | null; kdl: string };
