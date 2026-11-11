# Databases on the map: implementation plan

**Goal:** Domains, a focus view with a table tree, ER cardinality, and quieter tables, for databases on the map.

**Architecture:** Rust (crates/ig-app/src/map/tables.rs) builds domain groups, folds translations and
join tables, and annotates table arrows with columns and ends; the viewer draws cards at three distances,
cardinality ends, the focus view and the tree. Map-wire changes come last, after rebasing on arrow shapes.

**Tech Stack:** Rust, Svelte 5, TypeScript, vitest. Work in ../intent-graph-db-viewers (branch db-viewers).

**Spec:** `docs/specs/2026-11-11-database-viewers-design.md`

## Global Constraints

- Ends values: `one` | `zeroOne` | `many0` | `many1`; none when the schema does not say.
- Parent end: 1 if the FK column is non-null, 0..1 if nullable. Child end: 0..n, or 0..1 when the FK is
  unique or is the whole primary key.
- Join table: primary key = two FKs, at most 2 other columns (bookkeeping excluded) → one n..n arrow `via`.
- Hub: pointed at by 10 or more tables. Bookkeeping: deleted, created_at, updated_at, deleted_at.
- Domain: name prefix before the first `_`; fewer than 3 tables → "other".
- Colours via the design system tokens; rem for UI; as few code comments as possible; commits without attribution.
- `CARGO_TARGET_DIR=$PWD/target cargo test --workspace`, `cd viewer && pnpm run test`, `pnpm run typecheck` (0/0).

## Review Focus

- A schema with no prefixes (single-word table names): no empty or one-table domains, all in "other".
- An ORM schema without nullability info: no ends drawn, never a guess.
- A self-referencing table and a table referencing a missing table: no crash, no dangling wire.
- A table named exactly like a domain prefix (`batches` and `batches_*`): it belongs to that domain.
- 95+ tables: opening the store stays fast (no full-column render when closed or far).

---

### Task 1: Structure in Rust
**Files:** crates/ig-app/src/map/tables.rs (+ tests), ig-core model only if a field is needed, view types.
- [ ] Domain groups, leftovers group, views group, translations folded into `Table.translations`.
- [ ] Arrows: `fromColumn`, `toColumn`, `ends {from,to}`, join tables folded (`via`), hubs `pinned`.
- [ ] Tests for each rule and the Review Focus cases. Commit.

### Task 2: The card at three distances
**Files:** viewer/src/map/Table.svelte, MapBox.svelte (chip at far zoom), viewer types, tests map-table.test.ts.
- [ ] Chip (name, count, tags i18n / ←N / diff marks), keys only + "+N columns · +4 bookkeeping", full with `?` for nullable; i18n strip; pinned rows show "→ target". Commit.

### Task 3: Cardinality ends module
**Files:** new viewer/src/map/db/ends.ts (pure: SVG for one/zeroOne/many0/many1 at a point with an outward direction; label text), More-menu setting "Ends: Crow's foot | Labels | Both" (per browser), tests map-ends.test.ts. Commit.

### Task 4: The table tree and the focus view
**Files:** new viewer/src/map/db/Tree.svelte, db/Focus.svelte, db/focus.ts (pure layout: three columns, 1 or 2 hops, row anchors, lines with ends), Map.svelte entry points (card "Focus" button, Enter on a selected table, store menu "Tables…"), tests map-focus.test.ts.
- [ ] Tree: domains with counts, search over table and column names, set-aside groups; picking focuses.
- [ ] Focus: as the ERFocus board; neighbour click moves focus; 1/2 hops; Esc/Back returns; noise switches respected. Commit.

### Task 5: Noise switches
**Files:** Controls.svelte More menu ("Fold translations", "Set leftovers aside", "Pin hubs", "Fold join tables", "Fold bookkeeping"), applied in the viewer to the view's tables/arrows; tests. Commit.

### Task 6: Rebase and map wires
- [ ] Rebase db-viewers onto excalidraw-editing once arrow shapes are in.
- [ ] Table arrows attach at their column rows (anchors into the router's pins) and draw ends (Task 3) on map wires; view reads dotted, drawn only when the view is picked; pinned arrows not drawn. Tests. Commit.

### Task 7: Check and merge
- [ ] `make test`; screenshots on shop-front (store open, `batches` open, focus); review; merge into excalidraw-editing.
