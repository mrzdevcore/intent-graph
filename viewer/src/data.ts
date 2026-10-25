// What `view::data` (src/view.rs) emits: the page's embedded JSON and `ig view --json`.

export type Status = "added" | "changed" | "removed" | "unchanged";
export type Severity = "error" | "warning" | "note";

/** One diff row: op, line before, line after, text. */
export type DiffLine = [op: " " | "+" | "-", before: number | null, after: number | null, text: string];

export interface ViewNode {
  id: string;
  kind: string;
  name: string;
  file: string;
  line: number;
  endLine: number;
  parent: string | null;
  status: Status;
  complexity: number;
  loc: number;
  intents: string[];
  module: boolean;
  layer: string | null;
  lane: number | null;
  test: boolean;
  planned?: boolean;
  /** For a system: its icon category and its packages from `blueprint.systems`. */
  category?: string;
  packages?: string[];
  /** Line diff, when the node changed. */
  diff?: DiffLine[];
  /** Its text, when it is new (`+`) or removed (`-`). */
  code?: { line: number; text: string; op: "+" | "-" };
}

export interface ViewEdge {
  from: string;
  to: string;
  kind: string;
  confidence: string;
  status: Status;
  line: number;
  violation: boolean;
  planned?: boolean;
}

export interface ViewFinding {
  ruleId: string;
  severity: Severity;
  message: string;
  tool: string;
  file: string;
  line: number;
  nodeId?: string;
  edge?: { from: string; to: string };
  snippet?: string;
  fix?: string;
  fingerprint: string;
  /** The graph node the finding is shown on, if any. */
  pinned: string | null;
}

export interface ViewIntent {
  id: string;
  text: string;
  implementedBy: string[];
  testedBy: string[];
}

export interface IntentChange {
  status: Status;
  id: string;
  text: string;
}

export interface PlanVolume {
  newFiles: number;
  newNodes: Record<string, number>;
  changedNodes: number;
  removedNodes: number;
  newSystems: string[];
  newCrossLayer: [string, string][];
  newCrossLane: [string, string][];
  blastRadius: { total: number; byLayer: Record<string, number>; sample: string[] };
}

export interface ViewPlan {
  title: string;
  steps: string[];
  done: boolean;
  volume: PlanVolume | null;
  findings: Omit<ViewFinding, "pinned">[];
}

export interface ViewData {
  nodes: ViewNode[];
  edges: ViewEdge[];
  findings: ViewFinding[];
  intents: ViewIntent[];
  intentChanges: IntentChange[];
  notices: string[];
  /** Editor link template with `{file}` and `{line}`. */
  editor: string;
  root: string;
  mode: "changed" | "architecture";
  layers: string[];
  /** Blueprint lane names, the last being "Systems"; empty without a blueprint. */
  lanes: string[];
  blocking: boolean;
  base: string | null;
  head: string | null;
  counts: { added: number; changed: number; removed: number; unchanged: number };
  plan?: ViewPlan;
  state: "applying" | "planned" | "done" | "plain";
  /** Node ids to start in Focus mode on (from `ig focus`). */
  focus?: string[];
}

// Plan notes (src/notes.rs), from `ig serve`'s /api/notes.
export type NoteKind = "comment" | "move" | "link" | "remove" | "add";

/** comment: target (to for an edge), text · move: target, layer · link: target → to, text = edge kind ·
 *  remove: target (to for an edge) · add: text = node kind, target = name, layer = file or layer. */
export interface NoteInput {
  kind: NoteKind;
  target: string;
  to?: string | null;
  layer?: string | null;
  text?: string | null;
}

export interface Note extends NoteInput {
  id: string;
  done: boolean;
  reply: string | null;
}
