# Arrow shapes and circuit fixed: implementation plan

**Goal:** Four arrow shapes with Auto (curved on small maps, circuit on large), a per-arrow shape kept
in `ig/`, circuit wires that leave and enter facing sides, and open groups wired outside in.

**Architecture:** Rust gains the arrow `shape` property end to end. The viewer gets one pure module
that chooses sides and pins for any shape and draws elbow/curved/straight paths; `wires.ts` keeps its
circuit tracks and ribbons but routes from those pins; Map.svelte picks the shape per arrow (own shape,
else the map's default, Auto by size); pads get inside wires.

**Tech Stack:** Rust (kdl, serde), Svelte 5, TypeScript, vitest.

**Spec:** `docs/specs/2026-11-11-arrow-shapes-design.md`

## Global Constraints

- Shapes: `circuit` | `elbow` | `curved` | `straight`; arrow property `shape`, absent = follow the map.
- Auto: curved when the level on screen draws at most 12 boxes and 16 arrows, else circuit.
- Sides: facing sides; pins spread over the middle 60 % of a side, one pin per wire.
- Curved: cubic Bézier, control points along each side's outward normal at 40 % of the distance; avoid
  boxes (up to two pushes), else elbow.
- Ribbons when zoomed far out unchanged. Routing stays off the gesture path.
- Colours via tokens; rem for UI sizes; as few code comments as possible; commits without attribution.
- `cargo test --workspace`, `cd viewer && pnpm run test`, `pnpm run typecheck` (0/0) green.

## Review Focus

- A hand-written arrow with an unknown `shape` value: a read error naming the allowed values.
- Two boxes overlapping (hand-placed): the side chooser still returns sides, no NaN paths.
- An arrow between a box and its own ancestor's sibling across an open group: pads, not a direct wire.
- A curved arrow whose every push still crosses a box: falls back to elbow, never draws through.
- Auto flipping while a box is being dragged: no flicker (decide on settle only).

---

### Task 1: The `shape` property

**Files:** `crates/ig-app/src/map/spec/read.rs` (arrow), `arrows.rs` (`MapArrow.shape: Option<Shape>`,
serde camelCase, skipped when none, carried to bundle parts), `edit/arrow.rs` (`arrowset key=shape`,
byte-exact inverse), `crates/ig/src/mcp/schema.rs` (arrowset keys), `crates/ig/assets/ig-map.md` →
regenerate `ig-map.5` (`IG_UPDATE_MAN=1`), `docs/reference.md`, viewer `types.ts`.
- [ ] Tests: reads each shape; unknown value is a read error; arrowset set/clear with inverse bytes;
  the view JSON carries `shape`; schema accepts `shape`. Commit.

### Task 2: Sides, pins and the three simple shapes

**Files:** new `viewer/src/map/shapes/path.ts` (pure), test `viewer/test/map-shapes.test.ts`.
**Interfaces:**
- `type Shape = "circuit" | "elbow" | "curved" | "straight"`
- `sides(a: Frame, b: Frame): [Side, Side]` (`Side = "l" | "r" | "t" | "b"`)
- `pins(arrows: {id, from, to}[], frames: Record<string, Frame>): Map<string, { from: P; to: P; sides: [Side, Side] }>` (spread over the middle 60 %, ordered by heading, one pin each; people/externals use the figure edge — take the figure rect from the frame as MapBox draws it)
- `path(shape: "elbow" | "curved" | "straight", end: {from, to, sides}, avoid: Frame[]): { d: string; mid: P; len: number; head: [P, P] }`
- [ ] Tests: side by side, stacked, diagonal, overlapping frames; pin spread and order; elbow corner
  and rounding; curved control points, avoidance and fallback to elbow; straight; arrowhead tangent;
  mid by length. Commit.

### Task 3: Circuit from facing sides

**Files:** `viewer/src/map/wires.ts`, `viewer/test/map-wires.test.ts`.
- [ ] Route circuit wires from `pins()`: straight run when pins align within half a track, one bend
  otherwise, a drop through a gap only when a box is in the way; left/right tracks between columns like
  the row tracks; keep lanes, chamfers, pads at the source and the ribbon mode. Remove the
  upper-exits-bottom rule.
- [ ] Tests: two side-by-side boxes → one straight horizontal wire; a person left of a box → enters the
  box's left side; wires from two sources never share a pin; ribbon mode output unchanged on its
  existing tests. Update tests that encoded the old side rule (name them in the report). Commit.

### Task 4: Choosing the shape on the map

**Files:** `viewer/src/map/Map.svelte` (`wired`), `MapArrow.svelte`, `Controls.svelte` (More → "Arrows:
Auto | Circuit | Elbow | Curved | Straight", stored per browser in try/catch, shows "· curved here"),
`edit/ArrowBar.svelte` + `edit/ArrowPanel.svelte` ("Line" dropdown → `arrowset key=shape`).
**Interfaces:** `autoShape(boxes: number, arrows: number): "curved" | "circuit"` (12 / 16).
- [ ] Per arrow: its own `shape`, else the map default, Auto by the drawn level's counts. Auto decided
  on settle (not during a drag/gesture); a change cross-fades.
- [ ] Tests: thresholds; own shape wins; the More setting persists; the bar sends `arrowset shape` and
  "Like the map" clears it; selection, hit area, end handles and labels work on a curved arrow. Commit.

### Task 5: Open groups wired outside in

**Files:** `viewer/src/map/pads.ts`, `Pads.svelte`, Map.svelte wiring, tests `map-pads.test.ts`.
- [ ] Each pad with a `child` gets an inside wire from the pad to the child's facing side, in the
  shape used inside that box. Hover lights pad, inside and outside wire together (gold).
- [ ] A pad with no child shows a "?" chip; dragging its stub end onto a child sends `arrowset`
  `from` (outgoing) or `to` (incoming) = that child; tested.
- [ ] Commit.

### Task 6: Whole-branch check

- [ ] `make test`, typecheck; screenshots of the six-box map and this repo's map (Auto) for the user;
  whole-branch review; fixes.
