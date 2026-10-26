---
name: explore
description: Explain how part of this codebase fits together using the ig dependency graph - who uses a module, what it depends on, how a request flows. Read-only. Use when asked how something works, what depends on X, or what would be affected.
---

# Explore the code with the ig graph

Read-only: never edit files. The only things you may write are `.ig/focus.json` (through `ig focus`) and `.ig/show.json` (through `ig_show`).

1. Find candidate node ids. Run `ig query <text>`: for an unknown id it lists suggestions. Or run
   `ig check --all --json > /dev/null` and search `.ig/graph.json` for the name.
   Ids are `file` for a module, `file#Name` for a member, `file#METHOD /path` for a route.
2. Walk the graph: `ig query <id> --in` (who depends on it), `--out` (what it depends on),
   `--depth N` (default 2). Then read the code at the `file:line` it prints.
3. For "how does X work" or "how is X connected", call the `ig_show` tool with the path or subgraph
   (`nodes`, `edges` as `[from, to]` or `"from|kind|to"`), its ordered `steps` (`target`: a node id
   or `from|to`, `text`: one line) and a `title`; `mode` is `focus`, `architecture` or `chain`.
   It checks every id (an unknown one comes back with suggestions) and writes only `.ig/show.json`.
   Then answer in at most 10 lines. Draw a Mermaid `flowchart` or `sequenceDiagram` (at most 12
   nodes) only when `ig_show` says no viewer is open.
4. When no viewer was open, finish with `ig focus <id>...` for the nodes you discussed, and tell the user:
   "run /ig:view to see it in the graph".
