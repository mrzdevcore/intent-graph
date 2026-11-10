---
name: draw
description: Draw or change the intended architecture in ig/ - add, move, rename or remove boxes and arrows, sketch a target architecture before the code exists. Use when asked to design the architecture, draw the target, add a service or store to the map, or change ig/.
---

# Draw the intended architecture in ig/

`ig/` is the team's intended architecture. Without an `ig/`, tell the user to run `ig analyse`;
never run it yourself.

1. Change `ig/` through the `ig_edit` tool (`{"edits": [...]}`: add, remove, move, rename, set,
   arrow, unarrow). Your edits are proposals the user reviews under Proposals in the map. Check
   the result with `ig_plan`.
2. When the `ig_*` tools are not available, say so first: in Claude Code they appear once the
   folder is trusted and `/reload-plugins` has run. Edit `ig/*.kdl` yourself only when the user
   agrees; then run `ig fmt` and `ig plan`, and fix every error and warning they print.
3. Keep each box to at most 12 boxes inside it, and give a box that has code its `at` globs.

The format of `ig/` follows; it is also `man 5 ig-map`.

