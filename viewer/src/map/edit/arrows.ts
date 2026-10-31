// Arrow kinds the draw popover offers, and the last one used this session (module state, not stored).
import type { ArrowKind } from "../types";

export const ARROW_KINDS: ArrowKind[] = ["calls", "sends", "reads", "writes", "uses", "implements", "triggers"];
export const last = { kind: "calls" as ArrowKind };
