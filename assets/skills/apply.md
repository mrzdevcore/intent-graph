---
name: apply
description: Implement an approved ig plan step by step, checking the graph after every step, then have the reviewer agent judge the change against rules.md. Use when asked to apply, implement or build a planned change.
---

# Apply a plan with ig

Never edit `.ig/baseline.json`, `ig.yaml`, `rules.md` or the plan without the user's yes.

1. Run `ig apply start` (it uses the current plan) and announce the plan's steps.
2. For each step:
   - implement it;
   - run `ig check --agent`;
   - fix everything blocking before the next step.
3. An `ig/plan` *unplanned* finding means stop and ask the user. Never edit the plan unasked.
4. When stuck (the same finding survives 3 fixes, or you believe the rule is wrong), stop and ask the user with the finding and four options:
   - another approach;
   - update the plan, then `ig plan check` and `ig apply accept`;
   - accept an exception with `ig except <fingerprint> --reason "<user's words>"`;
   - change the rule (the user edits it, or approves your edit, then `ig apply accept`).

   In Claude use the question tool; in Codex ask in chat and end the turn.
5. When the tools are clean, run the reviewer:
   - Claude: the `ig:reviewer` subagent, given the plan path; pipe its JSON into `ig apply review`;
   - Codex: `codex exec --sandbox read-only "<reviewer instructions>: review the change against rules.md and the plan; print only a JSON array of {rule,file,line,message,severity}" | ig apply review`.

   Fix the review findings, then rerun the reviewer.
6. `ig apply finish` must exit 0. Then show `ig brief` and say "run /ig:view".
7. Codex has no Stop hook: run `ig check --agent` yourself before ending any turn.
