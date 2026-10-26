// The board's link to the files and to Claude's turns (Task 9 adds the store itself).

import type { BoardEngine } from "./engine";

/** Agent changes arriving up to this long after a turn ends still belong to that turn. */
export const AGENT_GRACE_MS = 1000;
export interface AgentTurn { id: string; start: number; end: number | null }

/** The turn an agent change at time `at` (the host's clock) belongs to, or null: the engine's
 *  `external(disk, turn)` then joins that turn's undo step. */
export function agentStep(at: number, turns: AgentTurn[]): string | null {
  const t = [...turns].reverse().find((t) => at >= t.start && (t.end === null || at <= t.end + AGENT_GRACE_MS));
  return t ? t.id : null;
}

/** Writes the board; only once the write succeeded is the written text the merge base (an edit
 *  made during the write stays unsaved, so a later merge keeps it). Every write goes through here. */
export async function save(engine: Pick<BoardEngine, "board" | "saved">, put: (text: string) => Promise<void>): Promise<string> {
  const text = engine.board();
  await put(text);
  engine.saved(text);
  return text;
}
