// Arrow kinds the kind chip offers.
import type { ArrowKind, ArrowKindView } from "../types";

export const ARROW_KINDS: ArrowKind[] = ["calls", "sends", "reads", "writes", "uses", "implements", "triggers"];

export const arrowKindNames = (kinds?: ArrowKindView[]): ArrowKind[] => (kinds?.length ? kinds.map((k) => k.name) : ARROW_KINDS);
