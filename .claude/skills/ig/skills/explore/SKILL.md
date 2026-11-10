---
name: explore
description: Explain how part of this codebase fits together and draw it on the ig map - who uses a module, what it depends on, how a request flows. Use when asked to show something, where X is used, how something works, what depends on X, or what would be affected.
---

# Explore the code with the ig graph

You read code freely. The only file you write yourself is `.ig/show.json`, through `ig_show`.
Without an `ig/` (the intended architecture), tell the user to run `ig analyse`; never run it yourself.
To change the architecture, follow the draw skill (`/ig:draw`, `$ig-draw` in Codex).

Only when the user asks to change the architecture: `ig_plan` prints what is left to build or move
and the drift; `ig_edit` edits `ig/` (`{"edits": [...]}`: add, remove, move, rename, set, arrow,
unarrow). Your edits are proposals the user reviews under Proposals in the map (and with `git diff ig/` in a git repository). Give `at` (code globs) for a
new box, and use `move` to put a box elsewhere: it keeps its id.

1. Find candidate ids. Run `ig view --json` and search `boxes[]` (`id`, `title`, `level`, `madeOf`)
   for the name; `arrows[]` (`from`, `to`, `kind`, `label`) holds the dependencies between boxes.
   Code-level boxes (`level` `code`) carry graph node ids: `file#Name` for a function or class.
2. Walk `arrows[]`: who depends on a box (`to` is it), what it depends on (`from` is it), one or two
   hops, down through `parent`. Then read the code in the box's `madeOf` files.
3. For "show me X", "where is X used", "how does X work" or "how is X connected", call the `ig_show`
   tool: it draws the answer on the map. Give the path or subgraph (`nodes`: box ids; `edges` as
   `[from, to]` or `"from|kind|to"`, from `arrows[]`), its ordered `steps` (`target`: a box id or
   `from|to`, `text`: one line) and a `title`; `mode` is `focus`, `architecture` or `chain`.
   It checks every id (an unknown one comes back with suggestions) and writes only `.ig/show.json`.
   Then answer in at most 10 lines. Draw a Mermaid `flowchart` or `sequenceDiagram` (at most 12
   nodes) only when `ig_show` says no viewer is open.
4. When `ig_show` says "shown in app", the map beside the chat already shows it: never run `ig view`.
   When no viewer was open, tell the user: "run /ig:view to see it in the graph".
