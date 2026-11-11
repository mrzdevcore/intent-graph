// Mirrors `ig view --json` (crates/ig-app/src/map). Test fixture: test/fixtures/shop-view.json.
export type Source = "ig" | "agent" | "person";
export type Confidence = "extracted" | "inferred" | "ambiguous";
export type Field<T> = { value: T; source: Source; confidence?: Confidence };
export type Level = "system" | "container" | "component" | "code";
export type BoxKind =
  | "person" | "system" | "external" | "app" | "service" | "job" | "store" | "queue" | "model"
  | "module" | "entry" | "usecase" | "port" | "adapter" | "agent" | "domain" | "group"
  | "folder" | "file" | "component" | "parts" | "task" | "table" | "function" | "class" | "method";
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
  context?: Field<string>;
  serviceName?: string;
  pinned?: string[];
  needsReading?: boolean;
  stale?: boolean;
  /** Against ig/: "build" is in the spec with no code yet, "drift" is code the spec does not describe, "moved" is in
   * the spec at a new place with its code still at the old; absent is built. */
  origin?: "build" | "drift" | "moved" | "unmanaged";
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
/** One of `View.arrowKinds`: an arrow kind and the kinds its rules join (empty = any). */
export type ArrowKindView = { name: ArrowKind; from: string[]; to: string[] };
export type ArrowKind = "calls" | "sends" | "reads" | "writes" | "uses" | "implements" | "triggers" | "bundle";
export type MapArrow = {
  id: string;
  parent: string;
  from: string;
  to: string;
  kind: ArrowKind;
  label?: string;
  note?: string;
  context?: string;
  /** Where ig/ writes it: "ig/shop.kdl:12". */
  written?: string;
  /** Where the code does it: a few "file:line". */
  seen?: string[];
  count: number;
  kinds: ArrowKind[];
  inLoop?: boolean;
  origin?: "build" | "drift" | "unmanaged";
  /** The arrows as written, when this one is drawn between boxes above their ends. */
  parts?: MapArrow[];
  ghost?: boolean;
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
/** One column of a table, as its schema declares it (crates/ig-core/src/model.rs). */
export type Column = {
  name: string; type: string; line: number; pk?: boolean; nullable?: boolean; unique?: boolean; default?: string;
  references?: { table: string; column?: string }; check?: string;
};
/** A table or view a table box draws: `reads` are a view's tables. */
export type Table = {
  name: string; kind: "table" | "view"; source: string; file: string; range: { startLine: number; endLine: number };
  columns: Column[]; reads?: string[];
};
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
  /** The table or view each table box draws, by box id. */
  tables?: Record<string, Table>;
  notices: string[];
  /** What reads but looks wrong in ig/ (crowded boxes, arrows the kinds do not expect…). */
  warnings: string[];
  kinds: KindView[];
  arrowKinds?: ArrowKindView[];
  /** The journal's entries nobody kept yet. */
  proposals?: Proposal[];
  landscape?: Landscape;
};
export type Skipped = { dir: string; why: string };
export type Landscape = { projects: Skipped[]; unsure: Skipped[]; leftOut: Skipped[]; links: { from: string; to: string; files: string[] }[] };
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
  | {
      op: "arrowset";
      from: string;
      to: string;
      key: string;
      value: string | null;
      before?: string | null;
      restore?: [number, string][];
    }
  | { op: "batch"; edits: Edit[] }
  | { op: "restore"; parent: string; before: string | null; kdl: string };
