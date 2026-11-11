# Databases on the map: domains, focus and ER diagrams

Date: 2026-11-11. Status: approved by the user ("go and build"). Branch `db-viewers` (git worktree off
`excalidraw-editing`), merged back into `excalidraw-editing` when ready. Design canvas: the "Databases
on the map" page of the design canvas: row 1 direction A (domains) and
B (focus), row 2 the card at three distances and the noise reducers, row 3 "A + B, as ER diagrams you
can read" (ERFocus with the table tree, Notation, DomainER).

## Intent

A database read from a repo's migrations is drawn today as every table, every column, every foreign key
at once, in a grid inside its store (crates/ig-app/src/map/tables.rs, viewer/src/map/Table.svelte). On
shop-front that is 95 cards, 1,033 rows and 193 lines: unreadable. The user picked A + B: the
whole schema grouped into domains, and a focus on one table with its neighbours, both drawn as ER
diagrams with cardinality (0..1, 1, 0..n, n..n), and a table tree to find and pick tables.

## Goals

- **Quiet the noise** (row 2 of the canvas): translation tables (`*_translations`) folded into their
  table as an i18n strip; the bookkeeping columns `deleted`, `created_at`, `updated_at`, `deleted_at`
  folded into one "+4" line; `*_backup`, `*_backup_<date>`, `<table>_backup_*` and `*_temp` tables
  set aside in a "leftovers" group (not `user_backup_codes`); what a view reads listed on its card
  and drawn only when the view is picked; hub tables (pointed at by 10 or more tables, translation
  and join tables not counted) pinned: their rows name the target, no wire. Each reducer is a switch, on by default.
- **A · Domains**: inside an opened store, tables grouped by name prefix into domain groups; a domain with
  fewer than 3 tables goes into "other"; views in their own group; leftovers in theirs. Closed, a domain is
  a group box like any other; arrows between domains are bundled with a count (the map's existing
  bundles). Opened, its tables are drawn as an ER diagram (DomainER board): keys only, row-to-row lines
  with cardinality ends, outside links as pads on its edge.
- **B · Focus**: picking a table (on the map or in the tree) and pressing Focus (or F with a table
  selected… F is the frame tool: use `Enter` on a selected table, and a "Focus" button on its card) opens
  the focus view: the table tree on the left, the table in the middle with every column, what it
  references on the right, what references it on the left, each line joining the foreign-key row to the
  primary-key row with crow's-foot ends; "1 hop | 2 hops"; clicking a neighbour moves the focus; Esc or
  "Back to the map" returns.
- **The table tree**: a sidebar listing every table and view by domain (counts per domain), a search field
  matching table and column names, the set-aside groups (translations, leftovers) at the bottom; picking
  focuses the table. It is in the focus view; on the map it opens from the store's menu ("Tables…").
- **Cardinality**, read from the schema (`ig_core::model::Column`: `pk`, `nullable`, `unique`,
  `references`): the parent end is 1 (non-null FK) or 0..1 (nullable FK); the child end is 0..n, or
  0..1 when the FK is unique or is the whole primary key (one-to-one). A join table (its primary key is
  two foreign keys and it has at most 2 other columns, bookkeeping excluded) is folded into one n..n line
  named on the line; "Show join tables" brings it back. The map skips self-references; only the focus
  draws the loop. View reads are dotted, without ends.
- **Ends notation**: crow's foot (default), "0..n" text labels, or both — a per-browser setting in the
  More menu, like the scroll wheel.
- **The card at three distances** (Cards board): far = a chip (name, column count, tags i18n / ←N hub /
  diff +a ~c); middle = keys only (PK and FK rows) and "+N columns · +4 bookkeeping"; near = every column
  with types, nullable marked `?`, PK gold badge, FK outlined badge.

## Non-goals

- Editing tables or columns from the map (the schema comes from migrations).
- Row counts, indexes, live data, query plans.
- Databases other than what the schema reader already reads (SQL DDL/migrations, Prisma, Django,
  SQLAlchemy, Drizzle, TypeORM).

## Anti-goals

- Drawing everything at once again: a store opened on a 95-table schema must read at fit.
- A cardinality the schema does not say: when unsure (an ORM that does not tell nullability), draw the
  plain line with no ends rather than guess.
- New colours: the ig design system's tokens; table headers neutral (`--map-sunken`), the picked table
  gold (`--map-trace`), as on the canvas.

## Decisions

- **Rust builds the structure** (tables.rs): domain groups as boxes of kind `group` (id
  `<store>#<domain>`, title the prefix), table boxes inside them; translations folded into `Table`
  (`translations: Vec<String>` and the translated column names); leftovers and views groups; arrows carry
  `fromColumn`, `toColumn` (the FK and PK column names) and `ends: { from, to }` with values `one`,
  `zeroOne`, `many0`, `many1` (none when unknown); join tables folded into one arrow with `via: "<join
  table>"` and `ends: many0/many0`; hub arrows flagged `pinned: true`. The switches (fold translations,
  leftovers aside, pin hubs, fold join tables, fold bookkeeping) are viewer settings applied to what Rust
  sends (Rust sends everything needed to show either way). Rust lays out the default-folded structure
  (join tables and the `(translations)` group get no frame); the viewer places whatever a switch brings
  back. With Pin hubs on, only foreign-key hub arrows are hidden; a pinned `via` arrow stays drawn.
  A table box is as tall as its keys-only card and a domain box takes its tables' shape, so an opened
  domain reads at fit; near, every column is shrunk into the box once it can be read. A store whose
  children are all tables (no domains) takes their shape the same way.
- **The viewer draws**: Table.svelte at three distances; cardinality ends drawn at wire ends by a small
  pure module (`viewer/src/map/db/ends.ts`) used by the map's wires and by the focus view; row-to-row
  attachment: a table arrow's end attaches at its column's row on the card (an anchor offset fed to the
  router's pins). The focus view is its own component (`viewer/src/map/db/Focus.svelte`) with its own
  simple layout (three columns, as on the ERFocus board), not the map's layout.
- **Order of work vs arrow shapes**: the arrow-shapes work (circuit from facing sides, pins, curved) is
  being built on `excalidraw-editing` at the same time. Everything that does not touch map wires is built
  first; the row-to-row anchors and ends on map wires come last, after rebasing `db-viewers` onto
  `excalidraw-editing` with arrow shapes in.

## Done when

- shop-front's store opened reads at fit: domains closed, counted links between them.
- Opening `batches` shows its 8 tables as an ER diagram, keys only, row-to-row lines with crow's-foot
  ends, outside links as pads.
- Picking `batches` in the tree opens the focus: 18 columns (+4 folded), 6 parents on the right (1, and
  0..1 for `batch_cycle`), 8 children on the left (0..1 for `manual_entries`); clicking a neighbour moves
  the focus; Esc returns to the map.
- Search finds a table by a column name. The ends notation switch works. The noise switches work.
- Tests: Rust (domain grouping, translations, leftovers, views, ends from nullable/unique/pk, join-table
  folding, hubs, from/to columns); viewer (card distances, ends module, focus layout and navigation, tree
  search, settings). `make test` green.
