---
name: view
description: Open the ig viewer on the current work and say what it shows - the plan being designed, the build in progress, or the finished change. Use when asked to show, view or visualize the graph, the plan or the result.
---

# Show the ig viewer

1. Run `ig view`; it writes `.ig/view.html` and opens it. The desktop app shows `ig_show` answers live.
2. Say in one line which state it shows and what to look at (`ig view --json` has it as `state`):
   - `planned`: dashed nodes are planned, and the Plan panel holds the volume.
   - `applying`: solid nodes are built, dashed nodes are still missing.
   - `done`: the code diffs and the brief.
   - `plain`: the plain viewer, no plan.
3. To open on specific nodes, call the `ig_show` tool with their ids first.

Never edit `ig.yaml`, `rules.md` or a plan from this skill.
