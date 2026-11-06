# `ig diff`: a change on the map

Date: 2026-11-06. Status: proposed. Journey 3 (review a change), project 2 of 4. Project 3 turns the
same diff into GitLab MR and GitHub PR comments.

## Intent

A merge request touches 40 files; the reviewer wants to know, in seconds, what it does to the
architecture: which boxes changed, which dependencies appeared or went away, whether the change
breaks a rule or drifts from `ig/`, and whether `ig/` itself (the intent) changed. Then they read
the code where it matters. `ig diff` shows that on the map, locally.

## Goals

- `ig diff [<base>] [<head>]` draws the map of `head` with the change against `base` on it, and
  writes nothing but `.ig/`.
- The same diff as JSON (`--json`), stable enough for project 3 to turn into a comment.
- Works on any repo `ig view` works on, with or without `ig/`.

## Non-goals

- MR/PR comments, Mermaid output, CI snippets (project 3).
- Line-level diffs: the map links to files, the reviewer reads lines in their usual tool.
- A diff of a folder of projects (landscape): refused with a one-line message for now.
- Connectors (Incus, Traefik): left out of both sides.

## Anti-goals

- A diff that calls something changed when it is not (unstable ids, a base built differently from
  head). Same code at both ends must give an empty diff.
- Writing to `ig/`, the working tree, or the user's git state (branches, index, stash).
- Slow on a big repo: building the base must not cost much more than a second `ig view`.

## The command

```
ig diff [<BASE>] [<HEAD>] [--json] [--no-open]
```

- `BASE`: a commit-ish. Default: the existing `Vcs::resolve_base(None)` (the CI base, else the merge
  base with origin/HEAD, main or master), with its notices.
- `HEAD`: a commit-ish. Default: the working tree, uncommitted changes included (like `git diff
  <base>`). Giving a `HEAD` requires a `BASE` (CI passes both): the default base is resolved
  against the checked-out HEAD, which may not be the head asked for. *(my call)*
- Writes `.ig/view.html` (the map of head with the diff on it) and opens it unless `--no-open`;
  prints the page path on stdout. `--json` prints the diff object instead and writes no page.
- Exit 0 whatever the diff holds (`check` is the CI gate); 2 on usage errors, a rev that is not a
  commit, outside a git repository, or a folder of projects.
- `-C` and `-q` as for every command.

## How the two maps are built

- **Head:** the working tree, `view::run` (non-writing) as `ig view` does, or a checkout of `HEAD`
  when one is given.
- **Base:** the code at `BASE` checked out with `git worktree add --detach` into a cache directory
  under `.ig/` (removed or reused afterwards; never the user's tree), made buildable with each
  frontend's existing `prepare_checkout`, then `view::run` on it with the `ig/` of that commit.
  The removed checker had this machinery: `git show 4bea9e2^:crates/ig-app/src/base.rs` (base
  graph with a cache keyed by `cache_inputs`) is the starting point.
- Both sides use the head's `ig.yaml`, so a config change never shows as an architecture change.
  *(my call)*

## What the diff holds

The diff is of the **architecture level**: the boxes and arrows of the map before the code level
(folders, files, functions, component parts) is added. The code level regroups itself by count
(more than 12 items get group boxes), so its ids are not stable across commits; a box's changed
files are listed in the box's entry instead. *(my call)*

Boxes and arrows are matched by id. Box ids come from `ig/` when it exists and from paths when it
does not, so both sides must use the same source:

- Both sides have an `ig/` that reads: ids from `ig/`, and the intent and drift sections are filled.
- Otherwise (`ig/` added or removed by the change, or the base's `ig/` does not read with today's
  reader): both sides use the map of the code alone for boxes and arrows; the intent section says
  `ig/ added (N boxes)`, `ig/ removed` or `base ig/ does not read: <first error>`; drift is left
  out. *(my call)*

- **Boxes:** `added`, `removed`, `changed` (a file in its `madeOf` changed between the two sides,
  from `git diff --name-only`), with the count of changed files. A change is also counted on every
  box above it, so the reviewer sees "web (3 files)" at the system level and walks down.
- **Arrows:** `added` and `removed` dependencies between boxes, with their kind (`calls`, `imports`,
  …).
- **Intent (`ig/`):** read from the two `ig/` trees, node by id: boxes and arrows added or removed,
  boxes moved (same id, new parent), and boxes changed, with the names of the fields that changed
  (title, description, kind, tech, `at`, pattern, `runs`, …). A renamed id is a removal plus an
  addition. *(my call)*
- **Rules and drift:** rule breaks (the `rules` layer) and drift items present at head and not at
  base: the new problems, not the old ones. Drift ids carry a counter, so drift is matched by its
  sorted `madeOf` globs; a rule break by its rule text and the ids of the two boxes it joins.
  *(my call)* Problems gone at head are listed too, as fixed.
- **Summary line:** e.g. `3 boxes changed, 1 box added, 1 new arrow web → db, ig/ unchanged,
  1 new rule break, drift +1`.

## On the map

The look goes through Claude Design before any viewer code (the project's rule for UI work), then
follows the viewer's tokens and themes. What the design must cover, decided:

- Changed, added and removed boxes and added and removed arrows are told apart at a glance; the rest
  fades, as an `ig_show` answer does.
- A card, like the answer card, lists the changes in order (intent first, then new rule breaks and
  drift, then boxes and arrows by size of change), each one flying to its place; ←, → and Esc as for
  answers.
- Removed boxes do not exist in head's layout: they are listed in the card and marked on their
  parent ("2 removed here"); a removed arrow whose two ends still exist is drawn as a ghost arrow.
  *(my call; drawing removed boxes in place would need a merged layout)*
- `ig view` afterwards shows the plain map; the diff belongs to the `ig diff` page only.

## Constraints

- POSIX conventions and the exit codes of the CLI design (docs/superpowers/specs/2026-11-06-ig-cli-journeys-design.md).
- stdout carries results only; notices on stderr.
- The base checkout honours the secret rules ig already has (never read `.env*`, `*.tfstate*`,
  keys).
- No `.rs` file over 600 lines; few code comments.

## Order of work

1. Engine and CLI: the base build, the diff object, `ig diff --json`, tests on small git repos
   made in temp directories.
2. Claude Design canvas for the diff on the map; the user picks the direction.
3. Viewer: the chosen look, the card, `ig diff` writing the page.

## Left to the builder

- Where the diff code lives (a `diff` module in `ig-app/src/map/`, likely) and the JSON field names,
  kept camelCase like the view.
- The base-checkout cache's location and eviction.
- Ordering inside each group of the card.
