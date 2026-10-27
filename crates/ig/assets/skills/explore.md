---
name: explore
description: Explain how part of this codebase fits together using the ig dependency graph - who uses a module, what it depends on, how a request flows. Read-only. Use when asked how something works, what depends on X, or what would be affected.
---

# Explore the code with the ig graph

Read-only: never edit files. The only thing you may write is `.ig/show.json` (through `ig_show`).

1. Find candidate ids. Run `ig view --json` and search `boxes[]` (`id`, `title`, `level`, `madeOf`)
   for the name; `arrows[]` (`from`, `to`, `kind`, `label`) holds the dependencies between boxes.
   Code-level boxes (`level` `code`) carry graph node ids: `file#Name` for a function or class.
2. Walk `arrows[]`: who depends on a box (`to` is it), what it depends on (`from` is it), one or two
   hops, down through `parent`. Then read the code in the box's `madeOf` files.
3. For "how does X work" or "how is X connected", call the `ig_show` tool with the path or subgraph
   (`nodes`, `edges` as `[from, to]` or `"from|kind|to"`), its ordered `steps` (`target`: a node id
   or `from|to`, `text`: one line) and a `title`; `mode` is `focus`, `architecture` or `chain`.
   It checks every id (an unknown one comes back with suggestions) and writes only `.ig/show.json`.
   Then answer in at most 10 lines. Draw a Mermaid `flowchart` or `sequenceDiagram` (at most 12
   nodes) only when `ig_show` says no viewer is open.
4. When no viewer was open, tell the user: "run /ig:view to see it in the graph".
