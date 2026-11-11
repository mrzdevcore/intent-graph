# Editing the map like Excalidraw

Date: 2026-11-10. Status: proposed. Design canvas: the "Editing the map" page of
the design canvas (light and dark rows). Arrow shapes and the
circuit fixes (the "Arrow shapes" board and the "Circuit, kept and fixed" row of "Map wiring") are a
separate project that comes next; the leaf-node viewers come after that.

## Intent

Editing the map today asks before it lets you draw: "+ Box" opens a long kind menu, then a form;
an arrow asks for its kind before it exists; once drawn, an arrow cannot be changed, and a box's
kind can only be changed by hand in `ig/`. People sketch architecture the way they sketch in
Excalidraw: shapes first, meaning after. The map should feel like that: draw a box, name it where it
sits, drag an arrow to another box, and set the kind and the details afterwards from the box or the
arrow itself.

Arrows also carry what an agent needs to build them: a short line shown on the map and a longer
context kept in the panel.

## Goals

- Draw a box by dragging (or clicking, or double-clicking the ground) and type its name on it.
  Nothing else is asked.
- A box may have no kind yet, and `ig/` holds it.
- Connect by dragging from a dot on any side of a box to another box; the arrow kind is guessed from
  the two boxes' kinds. Letting go on empty ground makes a new box there, already connected.
- Select a box: a bar above it offers its kind (searchable dropdown), edit details, arrow from here,
  open the code, more. Right-click offers the same, with a Kind submenu.
- Select an arrow: a bar on it offers kind, the short note, context (with a dot when set), flip,
  edit details, delete. Drag an end to reconnect. Double-click to edit the label in place.
- A details panel for a box or an arrow, editable, saved as you type.
- Select several boxes (drag a frame on the ground, Shift-click): set their kind, group them, delete
  them, in one step that undoes in one step.
- One floating tool island: hand (H), select (V), box (B), arrow (A), group frame (F), K for "add
  a box of a known kind" (today's kind list, for people who know what they want), and Arrange
  (⌥⇧F), which lays the map out well in one go, like a code formatter.
- Box and arrow titles in Excalifont (Excalidraw's hand font), bundled so `ig view` stays offline.
  The light theme gets the dot grid too.

## Non-goals

- Arrow shapes (curved, circuit, Auto) and the circuit routing fixes: next project.
- Free-text notes on the ground (the T tool in the canvas): not in the first build.
- Sharing hand positions and sizes between people (they stay per browser, as hand moves are today).
- Editing in `ig view` (the browser page has no edit API today; editing stays a desktop feature).
- Alt-drag to copy and pasting a plain list of names into boxes: later (copy and paste of boxes is
  in).

## Anti-goals

- A draw that needs a decision before anything appears.
- An edit that silently loses something: changing a kind keeps the box's other properties; an
  arrow reconnected keeps its kind, label, note and context.
- `ig/` files that stop being readable and hand-editable: every change is still a small KDL edit,
  surrounding text untouched, as `Spec::edit` does today.
- A big text box on the map: context lives in the panel, the map shows one short muted line.

## How it works today (what this builds on)

- Edits are the `Edit` enum in `crates/ig-app/src/map/edit.rs` (mirrored in
  `viewer/src/map/types.ts`): add, remove, move (reparent), rename (id), set (title, kind, props,
  `at`…), arrow, unarrow, restore (undo only). Applied by `Spec::edit`, journaled in
  `.ig/journal.json` with an inverse, undone and redone on the server. The desktop host exposes them
  as Tauri commands; the MCP `ig_edit` tool takes the same JSON.
- The KDL node name is the kind; `title` is required; unknown box properties are read errors; an
  arrow is `<kind> "<to>" label="…"` inside the box it leaves, one per pair; the reader ignores other
  arrow properties.
- Box positions are computed by `layout()`. Hand moves are offsets kept in the browser
  (`localStorage`, `ig-moves:<system>`); there is no resize.
- Arrow kinds are hardcoded in the viewer (`edit/arrows.ts`); the kind rules (`sends` to a queue…)
  live in `kinds.kdl` and only feed warnings.

## Decisions

### The ig/ format (`ig-map.5`)

- **A new built-in kind `box`**: "Not decided yet". It is something to build (the user's choice): a
  code block like a service, so it can take `at`, `ig plan` lists it as "to build, kind not decided",
  it can drift, and its arrows count as planned until the code has them. No own properties. It sits
  anywhere a run block or a design block may sit, and may hold either; no misplaced warning applies to
  it or to its children. It is what a drawn box is until someone sets a kind. Setting a kind is the
  existing `set key=kind` edit; properties the new kind does not take are kept in the edit's inverse
  so undo restores them, and dropped from the file (a read error otherwise). Setting a kind whose
  children would be out of place is allowed, with the usual warning.
- **`context` on any box**: free text, any length, for whoever builds it. Written as a KDL
  multi-line raw string when it has line breaks.
- **Two arrow properties**: `note` (one line, at most 80 characters, shown on the map under the
  label) and `context` (free text). The reader reads and validates them. Other unknown arrow
  properties become a warning, not a read error, so maps in the wild keep reading (my call).
- `description` stays the one line shown on hover and in lists; `context` is the long text for
  whoever builds it. The panel says so under each field.
- `ig-map.5` and `docs/reference.md` describe all three.

### Edits

- `set` keeps working for boxes, with `context` as a key.
- **New `arrowSet {from, to, key, value}`** for an existing arrow. Keys: `kind`, `label`, `note`,
  `context`, `from`, `to` (reconnect one end; the arrow node moves to the new source box when `from`
  changes), `flip` (value ignored). Refused when the result would duplicate a pair or point at
  itself. Its inverse is another `arrowSet`.
- **New `batch {edits: [...]}`**: applied in order, journaled as one entry, undone in one step. Used
  for "drop an arrow on the ground" (add box + arrow), multi-select delete or kind, and group (add a
  `group` + move each box into it). All or nothing: if any edit is refused, none applies and the
  refusal names the box. Only the end state must be valid; the steps in between are not saved or
  checked one by one (my call).
- **Delete takes what hangs on the box** (my call): deleting a box from the map sends a `batch` that
  removes its arrows (both ways) and everything inside it, then the box; one undo brings all of it
  back. When the box has children, the bar asks once inline ("Delete billing and 3 boxes inside?").
  The `remove` edit itself keeps refusing a box with children or arrows, so the MCP tool stays
  careful.
- The viewer sends `title` with `add`; a drawn box stays a client-only draft until its name is
  confirmed (Enter, or clicking away with a name typed). Esc on an empty draft removes it; nothing
  reaches `ig/`. "Drop an arrow on the ground" is the same: the draft and its rubber-band arrow stay
  in the browser and the `batch` (add + arrow) is sent on confirm (my call).
- **Which arrows can be edited** (my call): an arrow written in `ig/` can. A bundle (several arrows
  drawn as one between boxes above their ends) is read-only; its bar lists its parts, and picking
  one opens the boxes and selects it. A drift arrow (in the code, not in `ig/`) shows a "Keep it in
  ig/" button first; editing it sends a `batch` of `arrow` (writing it) and the change.
- **Group across parents** (my call): the boxes must share a parent; otherwise the group button is
  disabled with "These boxes are in different places". The group goes in that parent.
- **Copy and paste** (the user asked for it, with a choice of what to copy):
  - **Copy** (⌘C) takes the selected boxes alone: kind, title, own properties, context, size.
  - **Copy with what's inside** (⌘⇧C, and in the right-click menu) also takes their children, the
    arrows between the copied boxes (with kind, label, note, context), all of it.
  - Arrows to boxes outside the copy are not taken. `at` is never copied: two boxes would claim the
    same files.
  - **Paste** (⌘V) puts the copy at the pointer, inside the box or group under it (or where the
    copy came from when the pointer is off the map), as one `batch` of adds and arrows. Ids are made
    fresh (`billing` → `billing-2`); titles stay as they are, or get " copy" when pasted next to the
    original. The pasted boxes are selected, and placed by the placement rules (overlaps nudge).
  - The clipboard holds the copy as `ig/` KDL text too, so it can be pasted into another map or
    read in a text editor; pasting KDL text copied from an `ig/` file works the same way.
  - **Duplicate** (⌘D) is copy and paste in one step, next to the original, boxes alone; with Shift
    (⌘⇧D) with what's inside.
  - Pasting into a place a kind cannot sit is allowed with the usual warning.
- The view sends the arrow kinds and their target rules (from `kinds.kdl`), so the viewer stops
  hardcoding them and can guess.

### Guessing an arrow's kind

From the target's kind, in the format's rule order: a store → `writes`; a queue → `sends`; a job or
an entry → `triggers`; adapter to port → `implements`; anything else → `calls`. A `box` target (no
kind yet) → `uses`. The chip on the new arrow opens the kind list at once; one click changes it.

### Interaction (viewer)

- **Tool island** at the top centre of the map, keys shown on each tool. Tools stay as they are
  after one use only for the hand; box and arrow go back to select after one shape, unless Shift is
  held or the tool was double-clicked (locked).
- **Draw a box**: box tool + drag draws a draft at that size; a click places the default size;
  double-click on empty ground in select mode does the same. The draft's title is in edit mode on
  the box (Excalifont caret). Enter confirms (sends `add` with kind `box` and the title, inside the
  group or box under it), Tab confirms and opens the kind list, Esc cancels when empty.
- **Smart placement, nothing jumps** (the user: "it should feel natural and smooth", and "this
  should be smart" about simply snapping boxes back). Once a box has been on screen, its place is
  the truth; the automatic layout only places what has no place yet, and only moves others when it
  must, as little as it can. The viewer owns this, in one placement module, with these rules:
  - **A drawn box** lands where and as big as it was drawn.
  - **A box with no place yet** (added by K, by Claude or the MCP tool, pasted, or new in the code)
    goes into the free spot of its parent nearest to the boxes it has arrows with (else nearest the
    parent's centre of mass), on the parent's grid, never on top of another box.
  - **Overlap**: if a drawn, resized, moved or pasted box overlaps others, the others are nudged
    out of the way along the shortest direction, just enough for the gap the layout uses, and that
    nudge repeats outward until nothing overlaps. The box the person is holding never moves.
  - **The parent grows** when a child would leave it (drawn at its edge, resized, nudged), and its
    own neighbours are nudged by the same rule one level up.
  - **Delete leaves the gap**; nothing closes in on it.
  - **Everything that moves glides** with the existing `glide.ts` animation; nothing jumps.
  - **First open** of a map with no stored places uses today's layout as the starting places.
  - **Arrange** (below) is the one action that moves boxes the person placed.
  The server layout keeps computing frames; the viewer keeps, per box, the place it decided, as hand
  moves are kept today.
- **Arrange, a formatter for the map** (the user: "like prettier for coding"). One button that
  lays out the boxes well, so people can draw freely and tidy in one go:
  - **Where**: the last tool of the island (a wand icon, ⌥⇧F as in code formatters), the map's more
    menu, a group's right-click menu ("Arrange inside"), and the bar of a multi-selection ("Arrange
    these"). It arranges what is open on screen, or the group or selection it was asked on, and the
    groups inside, from the innermost out.
  - **Never worse than what is on screen**: the map must read at least as large when fitted to the
    stage (16:10, 1400×875, or the real stage) as before, so ig's first layout is the bar. Every
    layout is scored by one function: the zoom at which the scope fits the stage, lowered a little
    for crossings and arrows against the flow, then for wire length. A new layout replaces the
    current one only when it fits at least as large and scores 2% higher; otherwise nothing moves
    and the toast says "Already reads well". ig's own layout of the scope (its server frames, all
    the way down) is one more candidate, so a map left in a bad state goes back to it when nothing
    reads better: Arrange is never worse than ig's layout either. A group inside the one arranged
    keeps its frame or shrinks it, so what holds it never gets worse. "Arrange these" compares the
    selection's flowed block with the selection as it is the same way.
  - **The toast** counts only the boxes at the level on screen; when only insides moved it says
    "Arranged inside 3 groups".
  - **What good looks like**, in this order of weight:
    1. Arrows flow one way: callers before what they call, left to right or top to bottom, whichever
       reads larger (people and apps first, stores and outside systems last), so most arrows point
       the same way. A layer longer than 6 boxes, or one that would make the map too long, wraps
       into several lines of the same layer, row by row, so a fan-out becomes a block, not a tower.
    2. Few crossings: within each column, boxes are ordered to cross as few arrows as possible.
    3. Short, straight arrows: a box sits level with the box it talks to most, so that arrow is
       straight.
    4. Even spacing on the map's grid, boxes in a column share a width, groups fit their children
       with the same margin everywhere.
    5. Boxes that belong together stay together: a group is arranged as one block, its children
       arranged inside it.
  - **Deterministic**: the same map gives the same result, and arranging twice changes nothing,
    like a formatter. It never changes `ig/`, only places and sizes.
  - **Kind rules it knows**: people first (left or top), outside systems and stores last (right or
    bottom), queues between their senders and readers.
  - Everything glides to its new place; a toast says "Arranged 14 boxes" with **Undo**.
  - **Undo**: places live in the browser, not in the server journal, so the viewer keeps its own
    history of place changes (arrange, drag, resize, nudges). ⌘Z undoes whichever came last, a
    place change or an `ig/` edit; redo the same.
  - Arrange is built in the viewer's placement module, from the view's boxes and arrows; the server
    `layout()` stays the starting point for a map opened the first time.
- **Position and size** are kept like hand moves today, per browser, now as `{x, y, w, h}` per box;
  the layout's size is the default. Corner handles resize; the box's content reflows inside it, and
  the parent grows (gliding) if the box would leave it.
- **Connect**: four dots on the sides of a hovered or selected box, under any tool. Dragging from a
  dot draws a dashed gold band; the box under the pointer outlines in gold and its nearest side
  catches. Release on a box → `arrow` with the guessed kind. Release on empty ground → `batch` of
  `add` (a `box` draft there, title in edit mode) and `arrow`. With the arrow tool, a drag can start
  anywhere on a box. Two boxes selected + A connects the first to the second.
- **Box bar** (above the selected box): kind dropdown (icon and name; "Set a kind" for `box`), edit
  details (E), arrow from here, open the code (O, only when the box has `at`), more (the right-click
  menu).
- **Kind dropdown**: a search field, "Best match" (while typing, the top hit; before typing, the
  kind most used for boxes in the same parent, my call), then built-in run kinds, then design kinds when
  inside a run block, then the team's kinds (`ig/kinds`), then "No kind" (back to `box`). Type to
  filter, arrows to move, Enter to set. Each row shows the kind's swatch and its short meaning on the
  right.
- **Right-click on a box**: Rename (F2), Kind ▸ (the five most used and "All kinds… K"), Edit
  details (E), Arrow from here (A), Add a box inside (B), Move to a group… (M), Copy (⌘C), Copy with
  what's inside (⌘⇧C), Paste (⌘V, when the clipboard holds boxes), Duplicate (⌘D), Arrange inside
  (for a box with children), Open the code (O), Delete (⌫). Right-click on the ground: Paste, New box
  here, Arrange this group.
- **Arrow selection**: click an arrow (a wider invisible hit area along it). It turns gold, its ends
  get handles, and the arrow bar sits at its middle: kind dropdown, note (opens an inline field on
  the arrow), context (opens the panel at the context field; a gold dot when set), flip, edit
  details, delete. Dragging an end handle to another box sends `arrowSet from|to`. Double-click
  edits the label in place. Right-click gives the same as a menu.
- **On the map**, an arrow shows its label and, under it, its note in a smaller muted Excalifont
  line, cut with an ellipsis past the arrow's length. Context never shows on the map.
- **Details panel** (right side, replaces nothing: the read-only details card keeps its place when
  not editing):
  - Box: name (Excalifont), kind dropdown, description, tech, the kind's own properties (chips for
    `one-of`), context, made of (`at` globs), its arrows (kind chip, other end, planned when not
    seen in the code), inside (children, "+ Add a box"). `logo`, `owner`, `badges` and `pattern`
    stay file-only for now (my call).
  - Arrow: title "from → to", kind (chips), label, "On the arrow" (the note), "Context for whoever
    builds it", a read-only "In ig/<file>" KDL preview, and "In the code" (the files where ig saw
    it, when it did).
  - Fields save on blur and on Enter (Shift+Enter for new lines in context), one `set`/`arrowSet`
    each. Esc closes and returns focus to the map.
- **Several boxes**: drag on empty ground with select draws a marquee; Shift-click adds or removes.
  The bar shows "N boxes": kind, group (draws a `group` around them and puts its title in edit
  mode), align (row or column, client-side positions), delete. Each is one `batch`.
- **Group frame tool (F)**: drag to draw a `group` around whatever it encloses (a `batch` of add
  group + moves); its title is in edit mode.
- **K**: opens today's kind-first add form at the pointer, unchanged.
- Undo and redo buttons and zoom move to the bottom-left corner, as small islands.

### Moving around (the user: zooming, navigating and dragging should feel natural)

Today every wheel event zooms (`Map.svelte` `wheel` → `zoomAt`) and the dot grid is a fixed 18 px
screen pattern on the stage, so it does not move or scale with the map.

- **The dot grid belongs to the map.** Dots sit on the map's own grid (the unit boxes snap to),
  so they pan with the map and their spacing grows and shrinks with the zoom. To stay readable, the
  grid changes level like a ruler: when dots would come closer than about 8 px, only every 4th
  shows; when they spread past about 64 px, the halfway dots fade in. The change crossfades, never
  pops. Dot size stays the same on screen.
- **Trackpad**: two fingers pan in any direction; pinch zooms around the point under the fingers.
  **Mouse**: the wheel zooms around the pointer (a mouse has no other way to zoom), Shift+wheel
  pans sideways. ⌘ or Ctrl + wheel always zooms. The two are told apart by the event (pinch comes
  with `ctrlKey`, a trackpad scroll with small, fractional deltas); a setting in the more menu
  ("Scroll wheel: zoom | pan") overrides the guess.
- **Drag to pan** with the hand tool, with Space held (any tool), or with the middle mouse button.
  With select, a drag on empty ground draws the selection frame, as in Excalidraw.
- **Momentum**: a pan released with speed coasts a short way and eases to a stop. Zoom from the
  keyboard or the zoom buttons eases (about 150 ms) instead of jumping.
- **Keys**: ⌘+ and ⌘− zoom around the centre; ⌘0 back to 100 %; Shift+1 fits the whole map;
  Shift+2 fits the selection; the zoom readout in the corner fits on click. Arrow keys pan when
  nothing is selected and move the selection by one grid step when something is.
- **Limits**: zoom stays between the map fitting a quarter of the screen and a box filling it; at
  the limits it resists softly instead of stopping dead.
- **Smooth**: while a gesture runs, only the map layer's transform changes (no layout, no wire
  rerouting, no level-of-detail swap); detail and wires update when the gesture settles. Panning
  and zooming hold 60 frames a second on the 95-table shop-front map.
- Dragging a box near the edge of the screen pans the map slowly in that direction.

### Look

- Everything from the ig design system's tokens; selection and hot wires in `map-trace` gold; the
  box kinds keep their hues; a `box` (no kind) is a dashed `map-edge` outline on `map-surface` with
  "no kind yet" in its band.
- `--font-hand` becomes Excalifont (OFL-1.1, the Latin and Latin Extended subsets from
  `@excalidraw/excalidraw` 0.18.0, woff2, bundled in the viewer, licence file next to them), falling
  back to the current stack.
- Light theme `--map-dots` gets a visible colour (today transparent), so both themes show the grid.
  The ig design system artifact is updated to match (Excalifont as the hand family, the light dots).

## Done when

- A person can, without a menu or a form: draw three boxes, name them, connect them, then set two
  kinds and change one arrow's kind, note and context; reload; see the same map; undo every step.
- `ig/` after that is small KDL edits a person can read, with `box`, `context` and `note` in it, and
  `ig check` (or the reader) accepts it.
- Existing edit, journal and reader tests pass; new tests cover `box`, `context`, `note`, `arrowSet`
  (each key, refusals, inverse), `batch` (apply, inverse, one journal entry), the kind guess, draft
  boxes (Esc leaves nothing), the dropdown filter, the arrow bar, the placement rules (a new box
  lands near its arrows, overlaps nudge the least, the held box never moves, the parent grows,
  delete leaves the gap), Arrange (arrows flow one way, fewer crossings than the start on the fixtures, deterministic, idempotent, one undo), the shared undo of place changes and `ig/` edits, and copy and paste (boxes alone, with what's inside, fresh
  ids, no `at`, KDL text in and out).
- Drawing, pasting or resizing next to other boxes never makes a box the person didn't touch jump;
  what moves glides.
- The dot grid pans and scales with the map and changes level without popping; trackpad pan and
  pinch, mouse wheel zoom, Space-drag and momentum work as described; tests cover the wheel-event
  classification and the grid level for a given zoom.
- The viewer typecheck passes with no warnings.

## Left to the builder

- Component structure in `viewer/src/map/edit/` (new files are fine; keep the session class as the
  owner of in-flight edit state).
- Hit-testing details for arrows, the handle sizes, and how the marquee intersects boxes.
- The exact search ranking in the kind dropdown.
- Whether `batch` is one enum variant or a list at the API level, as long as the journal sees one
  entry.
