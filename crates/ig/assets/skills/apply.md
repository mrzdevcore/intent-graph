---
name: apply
description: Implement an approved ig plan step by step, checking the graph after every step, then have the reviewer agent judge the change against rules.md. Use when asked to apply, implement or build a planned change.
---

# Apply a plan with ig

Never do any of these without the user's yes: edit `.ig/baseline.json`, `ig.yaml`, `rules.md` or the plan; edit a checker tool's config file; add a suppression or disable comment (`eslint-disable`, `@ts-ignore`, `stylelint-disable`, `nosemgrep`, ...); weaken or delete tests.

1. Read `rules.md`. Re-run `ig plan check <plan>`, then run `ig apply start [<plan>]` (without the argument it uses the current plan) and announce the plan's steps.
   **UI from a design** (`plans/<slug>/screens/`): build it only from the project's existing UI components and design tokens; the mockup is context, never code. A mockup element with no matching component is a question for the user, not a new component. A design-system linter in `ig.yaml` `checkers` runs with `ig check --agent`; if none is configured, say so to the user.
2. For each step:
   - implement it;
   - run `ig check --agent`;
   - fix everything blocking before the next step.
3. An `ig/plan` *unplanned* finding means stop and ask the user. Never edit the plan unasked.
   An `ig/tamper` finding means a guarded file changed: stop, show the user which one, and run `ig apply accept` only after the user says yes.
4. When stuck (the same finding survives 3 fixes, or you believe the rule is wrong), stop and ask the user with the finding and four options:
   - another approach (you may do this yourself);
   - update the plan, then `ig plan check` and `ig apply accept`;
   - accept an exception with `ig except <fingerprint> --reason "<user's words>"` (tool and review findings only: an `ig/plan` finding needs option 2, an `ig/tamper` one the user's yes and `ig apply accept`);
   - change the rule (the user edits it, or approves your edit, then `ig apply accept`).

   Options 2-4 are carried out only after the user picks them. In Claude use the question tool; in Codex ask in chat and end the turn.
5. When the tools are clean, run the reviewer:
   - Claude: the `ig:reviewer` subagent, given the plan path (and, for a design, `plans/<slug>/screens/` and the design's `bridges` as context); pipe its JSON array into `ig apply review`:
     ```
     ig apply review <<'JSON'
     [...the reviewer's JSON...]
     JSON
     ```
   - Codex (flags from memory, unverified): `codex exec --sandbox read-only "$(cat .agents/skills/ig-apply/reviewer.md) Plan: plans/<slug>.md. Run ig brief --md to see the change." | ig apply review`

   Fix the review findings (the 3-fix rule above applies), then rerun the reviewer.
   If you believe a reviewer finding is wrong, ask the user; on their ruling record it with `ig except <fingerprint> --reason "<user's words>"`.
6. `ig apply finish` must exit 0; it lists the exceptions added during the apply, so show them to the user. If it reports stale apply state, tell the user and run `ig apply abort` on their yes. If it does not, go back to the fix loop (ask the user if stuck). Then show `ig brief` and say "run /ig:view".
7. Codex has no Stop hook: run `ig check --agent` yourself before ending any turn.
