// The open board, if any, registers here so other panels (the plan view) can select on it.
let board: ((ids: string[]) => boolean) | null = null;

/** Board side: returns the unregister function. */
export function registerBoard(select: (ids: string[]) => boolean): () => void {
  board = select;
  return () => {
    if (board === select) board = null;
  };
}

/** Selects `id` on the open board; false when no board is open or the id is not on it (the caller falls back to graph focus). */
export function selectOnBoard(id: string): boolean {
  if (!board) return false;
  return board([id]);
}
