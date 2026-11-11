export type Tool = "hand" | "select" | "box" | "arrow" | "frame";

export class ToolState {
  current = $state<Tool>("select");
  locked = $state(false);

  pick(t: Tool, lock = false) {
    this.current = t;
    this.locked = lock;
  }
  used(shift = false) {
    if (shift) this.locked = true;
    if (!this.locked && this.current !== "hand") this.current = "select";
  }
}
