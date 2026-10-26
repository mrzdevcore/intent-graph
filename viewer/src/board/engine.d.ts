import type { PieceKind } from "../api";

// The board engine as the viewer sees it (crates/ig-board, wasm). Hand-written: the generated
// glue in ./pkg is gitignored and absent from `ig view` builds; host.ts adapts it to this.

/** `mods` bits: Shift 1, Alt 2, Ctrl 4, Meta 8, the second press of a double-click 16.
 *  `pressure`: `PointerEvent.pressure` (pen strokes; 0.5 when absent). */
export type EngineInput =
  | { t: "down"; x: number; y: number; button: number; mods: number; pressure?: number }
  | { t: "move"; x: number; y: number; mods: number; pressure?: number }
  | { t: "up"; x: number; y: number; mods: number }
  | { t: "wheel"; x: number; y: number; dx: number; dy: number; mods: number }
  | { t: "key"; key: string; mods: number }
  | { t: "tool"; tool: Tool };

export type Tool = "select" | "module" | "function" | "route" | "system" | "note" | "frame" | "link"
  // the screens: a screen frame, the UI pieces palette, a piece, pen, arrow, image import, link to the architecture
  | "screen" | "pieces" | `piece:${PieceKind}` | "pen" | "arrow" | "import" | "bridge";

export type Surface = "architecture" | "screens";

export interface EngineOut {
  redraw: boolean;
  /** The board changed: save it. */
  changed: boolean;
  /** Its meaning changed: re-check it. */
  meaning: boolean;
  selection: string[];
  /** Rename this element: `rect` is in canvas CSS px. */
  edit: { id: string; rect: [number, number, number, number]; text: string } | null;
  /** A placed element and its layer: the host proposes its file. */
  placed: { id: string; layer: string | null } | null;
  cursor: string;
  error: string | null;
  /** What an outside change brought in or took out (element ids, link keys, …). */
  touched?: string[];
  /** Pick mode: the element or context box clicked. */
  picked?: string | null;
  /** A piece dragged out of the view and let go at this canvas point: it stayed put. */
  dropped?: { id: string; x: number; y: number } | null;
}

export interface Camera { x: number; y: number; zoom: number }

/** A label in world px; `title` is the check's reason. */
/** `paper`: on a screen's paper (dark ink in both themes). `name`: a screen's name, drawn unscaled
 *  above its frame at any zoom, with its state as `chip` (LOADING, ERROR…). */
export interface Label { id: string; text: string; r: [number, number, number, number]; faded: boolean; title: string | null;
  paper?: boolean; name?: boolean; chip?: string }

/** `question`: a sticky note marked as one; `answer`: its answer, null while open. */
export interface A11yItem { id: string; kind: string; text: string; selected: boolean; highlighted: boolean; problem: string | null;
  question: boolean; answer: string | null }

/** `amend` joins the top undo step (a host follow-up, such as a placed element's file). */
export type Step = "user" | "amend" | { agent: string };

/** Board text across the boundary: a design.json text, or `{design?, screens?}` where `screens`
 *  is every screen (an array) or some by id (an object; null removes one). */
export interface BoardEngine {
  load(boardJson: string, contextJson: string): void;
  /** From now on only looked at (pan, zoom, fit); `pick`: hover highlights, a click is `picked`. */
  setReadOnly(pick: boolean): void;
  /** Switches the surface when given (selection and tool reset); the current one. */
  surface(s?: Surface): Surface;
  /** Every screen's canonical file text, by id. */
  screens(): Record<string, string>;
  /** A state of a screen (loading, error, empty…) next to its base: one undo step. */
  variant(screen: string, state: string): EngineOut;
  /** A stored image (`putAsset`'s name) becomes a screen's locked background: one undo step. */
  importImage(asset: string): EngineOut;
  /** The rename overlay's commit: an element's name, a screen's title, a piece's or nav's label. */
  rename(id: string, text: string): EngineOut;
  /** Another scope's context: the board, its history and the camera stay. */
  setContext(contextJson: string): void;
  input(ev: EngineInput): EngineOut;
  /** The element, link key or context box under a canvas point. */
  hit(x: number, y: number): string | null;
  tool(): Tool;
  /** One history step; `error` set (and nothing changed) when invalid. */
  ops(opsJson: string, step: Step): EngineOut;
  external(diskJson: string, turn: string | null): EngineOut;
  setCheck(targetsJson: string): void;
  dismiss(id: string | null): void;
  highlighted(): string[];
  select(ids: string[]): void;
  /** Takes back agent turn `turn` as one user step; later edits stay. */
  revert(turn: string): EngineOut;
  undo(): EngineOut;
  redo(): EngineOut;
  /** The canonical design.json text (`ig_design::to_text`). */
  board(): string;
  /** What changed between two board texts (design and screens), as lines for Claude; none for a
   *  geometry-only change. */
  delta(prevJson: string, nextJson: string): string[];
  /** After EVERY successful write of `written` (a `board()` result): later merges start from it. */
  saved(written: string): void;
  palette(colors: Record<string, [number, number, number, number]>): void;
  resize(w: number, h: number, dpr: number): void;
  /** Draws only when something changed; says whether it drew. */
  frame(): boolean;
  /** Rebuilds the GL resources after `webglcontextrestored`. */
  restore(): void;
  camera(): Camera;
  labels(): Label[];
  a11y(): A11yItem[];
  /** Uploads the image's texture (kept for a restored context); drawn instead of its placeholder. */
  setImage(asset: string, bitmap: ImageBitmap): void;
  /** Frees the image's texture and closes its bitmap (no screen shows it, or its file changed). */
  dropImage(asset: string): void;
  free(): void;
}
