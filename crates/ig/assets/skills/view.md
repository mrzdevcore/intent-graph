---
name: view
description: Open the ig system map and say what it shows. Use when asked to show, view or visualize the architecture, the map or the graph.
---

# Show the ig viewer

1. Run `ig view --no-read` (you are the agent: never let it start another one); it writes `.ig/view.html` and opens it.
2. Say in one line what to look at: `ig view --json --no-read` has the `boxes[]`, `arrows[]` and the layers'
   `marks[]` (a `bad` or `warn` mark, with its `badge`, is worth naming), and `notices[]`.

Never edit `ig.yaml` from this skill.
