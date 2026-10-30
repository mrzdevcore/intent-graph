// Arrow kinds the draw popover offers, and the last one used this session (module state, not stored).
import type { ArrowKind } from "../types";

export const ARROW_KINDS: ArrowKind[] = ["calls", "depends", "data", "network", "event"];
export const last = { kind: "calls" as ArrowKind };
