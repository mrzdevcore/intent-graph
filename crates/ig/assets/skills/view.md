---
name: view
description: Open the ig system map and say what it shows. Use when asked to show, view or visualize the architecture, the map or the graph.
---

# Show the ig viewer

In the desktop app the map is already open: skip step 1, and use `ig_show` to point at something on it.

1. Run `ig view`; it writes `.ig/view.html` and opens it.
2. Say in one line what to look at: `ig view --json` has the `boxes[]`, `arrows[]` and the layers'
   `marks[]` (a `bad` or `warn` mark, with its `badge`, is worth naming), and `notices[]`.

Never edit `ig.yaml` from this skill.
