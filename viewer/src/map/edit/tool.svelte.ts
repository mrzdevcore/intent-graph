// The editing tool in hand: Select, + Box (next click or the form adds one) or → Arrow.
export type Tool = "select" | "box" | "arrow";

export class ToolState {
  current = $state<Tool>("select");
}
