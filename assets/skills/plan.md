---
name: plan
description: Plan a change against the ig dependency graph before writing any code - write plans/<slug>.md with the planned nodes and edges, check it with ig plan check (volume, blast radius, rule findings) and ask the user to approve it. Use when asked to plan, design or scope a feature or change.
---

# Plan a change with the ig graph

Planning only: never write or edit source code in this skill. The only file you write is the plan.

1. Read `rules.md`, `ig.yaml` and the user's request.
2. Explore with `ig query <id> --in|--out [--depth N]`. For an unknown id it lists suggestions.
   Ids are `file` for a module, `file#Name` for a member (`file#Class.method` for a method),
   `file#METHOD /path` for a route and `system:<Name>` for an external system.
3. Write `plans/<kebab-slug>.md` in this format:

   ````md
   # <Title>

   <Intent: at most 3 lines.>

   ## Steps
   1. <layer or package>: <what changes>
   2. ...

   ```ig-plan
   nodes:
     - id: packages/core/src/export.ts#exportRows
       status: new          # new | changed | removed
       kind: function       # optional: default function for file#Name, module for a file
       exported: true       # optional
     - id: packages/core/src/repo.ts#Repo.find
       status: changed
   edges:
     - from: packages/core/src/export.ts#exportRows
       to: packages/core/src/repo.ts#Repo.find
       kind: calls          # optional, default calls
   systems: [PostgreSQL]    # optional: external systems the change uses
   ```
   ````

   At most 8 steps, each tagged with its layer or package. `changed` and `removed` ids must
   exist in the graph; `new` ids must not.
4. Run `ig plan check plans/<slug>.md --json`. Fix the plan until `ok` is true: correct ids that
   `plan/unknown-node` or `plan/unknown-edge-end` reject, and rework steps that break a rule
   (`ig/arch`) or add a structural regression.
5. Show the user the volume in 4-6 lines (new files and nodes, changed and removed nodes, new
   cross-layer links and systems, blast radius by layer) and say "run /ig:view".
6. Ask the user to approve the plan or ask for changes. Change the plan only when they say so,
   and re-run step 4 after every change.
