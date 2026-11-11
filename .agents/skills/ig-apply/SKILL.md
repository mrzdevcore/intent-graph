---
name: ig-apply
description: Build what the ig plan lists - write the code for the boxes and arrows in ig/ that have none yet, and move code into the box it belongs to. Use when asked to apply the plan, implement the architecture, build what ig/ says, or make the code match the map.
---

# Build what the plan says

`ig/` is the contract: you change the code to match it, never `ig/` or `ig.yaml` to match the code.
Without an `ig/`, tell the user to run `ig analyse`; never run it yourself.

1. Call `ig_plan` (or run `ig plan --json`) and take its "to build" and "move" items, or only the
   ids the user named: a box by its id, an arrow as `from->to`.
2. For each box, read it in `ig/*.kdl`: its kind, title, description, tech, `at` globs, `context`
   and the arrows that leave it. Write its code under its `at`; a new block gets its folder and the
   project files it needs, in the stack its `tech` names or, without one, the one the repo uses.
3. For each arrow, make `from` really depend on `to` the way its kind says (`calls`: a client call,
   `sends`: a message on the queue, `reads` and `writes`: a query or a repository, `triggers`: a
   start, `implements`: the port filled, `uses`: an import), following its `label`, `note` and
   `context`.
4. For each move, move the code into the box's new `at`, keeping its behaviour.
5. Build and run the tests the way the repo does, then call `ig_plan` again: an item is done when it
   leaves "to build" or "move". Repeat until every item is done or you are stuck, then say what is
   left and why.
6. Never edit `ig/`: that is the draw skill (`/ig:draw`, `$ig-draw` in Codex), and only when the
   user asks. Drift your code brings stays in the plan for the user to `ig accept` or fix; never
   accept it, commit or push yourself.
