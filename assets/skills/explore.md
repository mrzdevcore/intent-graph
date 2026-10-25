---
name: explore
description: Explain how part of this codebase fits together using the ig dependency graph - who uses a module, what it depends on, how a request flows. Read-only. Use when asked how something works, what depends on X, or what would be affected.
---

# Explore the code with the ig graph

Read-only: never edit files. The only thing you may write is `.ig/focus.json`, and only through `ig focus`.

1. Find candidate node ids. Run `ig query <text>`: for an unknown id it lists suggestions. Or run
   `ig check --all --json > /dev/null` and search `.ig/graph.json` for the name.
   Ids are `file` for a module, `file#Name` for a member, `file#METHOD /path` for a route.
2. Walk the graph: `ig query <id> --in` (who depends on it), `--out` (what it depends on),
   `--depth N` (default 2). Then read the code at the `file:line` it prints.
3. Answer in under 15 lines, plus one Mermaid `flowchart` or `sequenceDiagram` of at most 12 nodes.
4. Finish with `ig focus <id>...` for the nodes you discussed, and tell the user:
   "run /ig:view to see it in the graph".
