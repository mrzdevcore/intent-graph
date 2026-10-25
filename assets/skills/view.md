---
name: view
description: Open the ig viewer on the current work and say what it shows - the plan being designed, the build in progress, or the finished change. Use when asked to show, view or visualize the graph, the plan or the result.
---

# Show the ig viewer

1. Run `ig view`. If the user wants it live while you work, run `ig view --watch` in the background instead.
2. Say in one line which state it shows and what to look at (`ig view --json` has it as `state`):
   - `planned`: dashed nodes are planned, and the Plan panel holds the volume.
   - `applying`: solid nodes are built, dashed nodes are still missing.
   - `done`: the code diffs and the brief.
   - `plain`: the plain viewer, no plan.
3. Run `ig focus <ids>` first if you want the viewer to open on specific nodes. It shows them once.

Never edit `.ig/baseline.json`, `ig.yaml`, `rules.md` or a plan from this skill.
