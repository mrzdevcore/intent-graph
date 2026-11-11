import type { ArrowShape } from "../types";

export type MapShape = "auto" | ArrowShape;
export const SHAPES: ArrowShape[] = ["circuit", "elbow", "curved", "straight"];
export const SMALL = { boxes: 12, arrows: 16 };

export const autoShape = (boxes: number, arrows: number): "curved" | "circuit" =>
  boxes <= SMALL.boxes && arrows <= SMALL.arrows ? "curved" : "circuit";

export const mapShape = (pref: MapShape, boxes: number, arrows: number): ArrowShape =>
  pref === "auto" ? autoShape(boxes, arrows) : pref;

export const lineOf = (a: { shape?: ArrowShape }, map: ArrowShape): ArrowShape => a.shape ?? map;
