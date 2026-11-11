# Arrow shapes, and circuit wiring fixed

Date: 2026-11-11. Status: proposed (decisions taken with the user in conversation). Design canvas:
the design canvas, the "Arrow shapes" board on "Editing the map" and
the "Circuit, kept and fixed" row on "Map wiring". Follows the Excalidraw-style editing work
(`2026-11-10-excalidraw-editing-design.md`), on the same branch.

## Intent

Every arrow on the map is drawn as a circuit-board wire today (`viewer/src/map/wires.ts`). On a large
map that reads well. On a small one it does not: on a six-box map the wire from `cli` to `User API`
leaves the bottom of `cli`, runs under everything and climbs into the bottom of `User API`, though the
two boxes face each other. The user: "the arrows should not be always the PCB circuit type", "for
small maps, curved is great and for large one circuit is great", "I prefer circuit but [it] should not
be like this", and, for an open group, "I would like to see the connection here (outside in)".

## Goals

- Four arrow shapes: **circuit** (today's tracks, 45° bends, a pad at the source), **elbow** (plain
  right angles), **curved** (a smooth curve, like Excalidraw), **straight**.
- **Auto**, the map's default: curved when what is open on screen is small, circuit when it is large.
  Small = at most 12 boxes and 16 arrows drawn at the level on screen (named constants).
- Any arrow can take its own shape; it keeps it whatever Auto picks.
- Circuit fixed: a wire leaves the side of its box that faces the other box and enters the side
  that faces back; fewest bends; wires from different boxes never merge (at the near zoom); a person
  or an outside system wires from its figure.
- An open group shows its connections outside in: each port on the frame continues inside to the
  child box that makes or takes the call.

## Non-goals

- Ribbons when zoomed far out stay as they are (they gather wires on purpose).
- Hand-drawn waypoints on an arrow (dragging the middle of a wire): later.
- Arrowhead styles, line widths, colours per arrow.

## Anti-goals

- A wire that detours around the far side of a box when the boxes face each other.
- A curve that passes through a box it does not connect (curves bend around the boxes they would cross,
  or fall back to elbow for that arrow).
- Changing an arrow's shape changes nothing else about it (kind, label, note, context, ends).

## Decisions

### Where the shape lives

- **Per arrow, in `ig/`**: an arrow property `shape` = `circuit` | `elbow` | `curved` | `straight`,
  absent = follow the map. It is part of the drawing's intent, so it travels with the map. The reader
  accepts and validates it; `arrowset key=shape` sets and clears it (byte-exact inverse); the MCP
  `ig_edit` schema lists it; `ig-map.md`/`ig-map.5` and `docs/reference.md` describe it.
- **The map's default** (Auto | Circuit | Elbow | Curved | Straight) is a viewing preference kept per
  browser like the scroll-wheel setting, in the More menu ("Arrows: Auto"). My call: it is how a person
  likes to look at maps, not part of the architecture.
- The view sends `shape` on `MapArrow` (and on bundle parts). A bundle (several arrows drawn as one)
  follows the map; its parts' shapes apply when the group is opened.

### Choosing sides (all shapes)

One function decides where a wire leaves and enters, shared by every shape:
- Compare the two frames: if they overlap more vertically than horizontally (side by side), the wire
  leaves the facing left/right side and enters the facing one; if stacked, bottom/top. Diagonal: the
  axis with the larger gap wins.
- On a side, several wires spread over its middle 60 % in the order of where they head, one pin each
  (today's `SPREAD`), so wires from different boxes never share a pin.
- A person's or an outside system's wire starts at the figure's facing edge, not under its name.

### Circuit (fixed)

Keep `wires.ts`'s tracks, lanes, 45° chamfers, pads at the source and the ribbon mode, but route from
the facing sides above: a straight run when the two pins line up within half a track; one bend when
they do not; a vertical drop through a gap only when a box is in the way. Tracks between columns are
used like tracks between rows today (left/right sides). Today's "upper box exits its bottom, lower
enters its top" rule goes.

### Elbow, curved, straight

- **Elbow**: from the facing pins, horizontal–vertical–horizontal (or the vertical form), the corner
  at the middle, rounded with the circuit chamfer size; no lanes, no pads.
- **Curved**: a cubic Bézier from pin to pin, control points along each side's outward normal at
  40 % of the distance; if the curve crosses another box's frame, bend it around by pushing the control
  points out (up to twice), else draw that arrow as elbow.
- **Straight**: pin to pin.
- The arrowhead points along the last segment / the curve's end tangent. The label and the note sit at
  the path's middle (by length), as today.
- Selection, the wide hit area, end handles, reconnect by dragging an end and the label/note editing
  (from the editing work) work the same on every shape.

### Auto

Counted per drawn level (the parent whose inside is on screen): boxes drawn and arrows drawn there. Auto
switches when the person opens or closes a group or zooms across the level change; the wires glide
(cross-fade) rather than jump. The More menu shows "Arrows: Auto · curved here" so the choice is
visible.

### Ports outside in

On an open box, each pad (`viewer/src/map/pads.ts`, `Pads.svelte`) that has a `child` gets a wire
inside the box from the pad to that child's facing side, in the shape the map uses inside that box,
routed with the same side rules. Hovering a pad, its inside wire or the outside wire lights both halves
gold. A pad with no child keeps its short stub and shows a small "?" chip ("which part?"); dragging that
stub's end onto a child sends `arrowset` with `from` (outgoing) or `to` (incoming) set to the child, so
the arrow is now written from/to that child in `ig/`.

### The arrow bar

The arrow bar (editing work) gets a "Line" dropdown: Like the map, Circuit, Elbow, Curved, Straight
(`arrowset key=shape`, "Like the map" clears it). The arrow panel shows the same choice.

## Done when

- On the six-box map from the user's screenshot, with Auto, arrows are curved, each leaves and enters
  facing sides, and none passes through a box.
- On this repo's map (13 boxes at the top level), Auto draws circuit; `cli`/`Desktop app` wires leave
  their facing sides; no two wires from different boxes share a pin.
- An arrow set to curved stays curved on a large map, survives reload (it is in `ig/`), and undoes.
- Opening a group shows each pad wired to its child; a "?" stub dragged onto a child rewrites the
  arrow's end in `ig/`.
- Tests: the side chooser (side by side, stacked, diagonal, overlapping), each shape's path and
  arrowhead, curved avoiding a box and falling back to elbow, circuit straight runs and single bends,
  Auto's thresholds and switch, the `shape` property (read, set, clear, inverse bytes, schema), pads
  wired inside and the "?" drag. Existing wiring tests updated only where the side rule intentionally
  changes them (named in the report).
- Panning and zooming still hold their frame rate on the 95-table map (routing stays off the gesture
  path, as now).

## Left to the builder

- The exact curve-avoidance search and lane allocation for left/right tracks.
- Whether the side chooser lives in `wires.ts` or a new module next to it.
- The cross-fade's timing.
