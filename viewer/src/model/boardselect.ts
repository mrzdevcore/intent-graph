// The open board, if any, registers here so other panels (the plan view) can select on it.
let board: ((ids: string[]) => void) | null = null;

/** Board side: returns the unregister function. */
export function registerBoard(select: (ids: string[]) => void): () => void {
  board = select;
  return () => {
    if (board === select) board = null;
  };
}

/** Selects `id` on the open board; false when no board is open (the caller falls back to graph focus). */
export function selectOnBoard(id: string): boolean {
  if (!board) return false;
  board([id]);
  return true;
}
