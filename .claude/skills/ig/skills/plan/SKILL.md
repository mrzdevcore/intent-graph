---
name: plan
description: Go through what the ig plan lists and say, for each drift item, whether to take it into ig/ or fix the code, with the code that shows why. Use when asked what ig plan or ig check says, why the code drifted from the map, what to accept, or how to get ig check green.
---

# Sort the plan

Without an `ig/`, tell the user to run `ig analyse`; never run it yourself.

1. Call `ig_plan` (or run `ig plan --json`). Each drift item ends with its id in brackets: a box
   by its id, an arrow as `from->to`. Say the counts in one line: to build, to move, drift, not read.
2. For each drift arrow, open the code behind it: the imports or calls in `from`'s files (its `at`
   globs in `ig/*.kdl`) that reach `to`. Read what each box is for (its kind, title, description)
   and sort the item:
   - **take into ig/**: the dependency fits the boxes' roles (a page using the UI kit, an API
     reading its own store, a test calling what it tests);
   - **fix the code**: it runs against them (a service or a shared kit reaching into one page or
     feature, two pages reaching into each other, a cycle). Name the file and line, and where the
     code should go instead;
   - **your call**: you cannot tell; say what would decide it.
   A drift box (code in no box, a store or outside system ig found) is taken into ig/ unless it is
   dead or misplaced code.
3. Answer grouped by those three, most important first, each item with its id and one line of why.
   Group items with the same reason instead of repeating it.
4. Ask before taking anything into ig/; then run `ig accept <ids>` with the ids the user agreed to
   (quote arrow ids: `ig accept 'web->api'`), and call `ig_plan` again to show what is left.
5. "To build" and "move" items are for the apply skill (`/ig:apply`, `$ig-apply` in Codex); "not
   read" means ig cannot see that code: add it to a workspace or to `ignore` in `ig.yaml`, and say
   so rather than editing `ig.yaml` yourself.

Never change the code from this skill, never edit `ig/` by hand (that is `/ig:draw`), and never
commit or push.
