# ADR-053: The merge gate on `main` — hosted-CI authority retired, the local werk stack named

**Status:** Proposed — 2026-07-24. Supersedes ADR-026's layer-3 authority claim (see Supersession). Awaiting Jeff's ratification of the Decision section (card #3480 AC3).

## Context

ADR-026 (accepted 2026-04-25) designed a three-layer quality architecture whose
layer 3 was hosted GitHub-Actions CI, branch-protection-authoritative on `main`:
"PRs cannot merge if CI is red." CLAUDE.md carried the same claim ("`--no-verify`
is overridden by CI as authoritative on `main`... Branch protection blocks merge
of red PRs").

That backstop stopped existing on **2026-04-29**: hosted runners were
cost-killed (#2600), `quality.yml`'s per-PR `push:`/`pull_request:` triggers were
commented out (schedule-only since), and branch protection + the ruleset were
emptied of required checks. No supersession was marked. For ~12 weeks every
role's mental model — and at least one build→prod correctness thread
(2026-06-17) — assumed a net that was not there. A documented safety net that
does not exist is worse than none: it invites false confidence exactly when the
gate is being bypassed (the 2026-06-17 `gh merge --admin` override cycle).

## Decision

**The authoritative merge gate on `main` is the local werk stack:**

1. **`werk-merge` content-verify** — the merge verb proves the PR's content
   before merging (two-tier merge proof, #3014/#3476); overrides that bypass it
   are exceptions logged as such, not a sanctioned path (#3479 is the
   override-killer).
2. **Local `act` run of `werk.yml`** — build → test (werk-test, blocking gate,
   #3190) → deploy-werk → env-up → demo, per card, before any land.
3. **Role gates** (`/gate-code`, `/gate-quality`, `/gate-arch`, `/gate-ops`,
   `/gate-product`) + the demo ceremony's witness gates (#3443) — team-level
   acceptance recorded on the card, with Jeff's GO as the land authority
   (DEC-048).

**The red-`main` detector is the 03:00 nightly** (`nightly-suites.sh`) — the
from-scratch "does main build cleanly" question moved from per-PR hosted CI to
the nightly sweep. Known gap, tracked, not hidden: the nightly turns red but
does not yet file cards on red (#2527, Silas). Until #2527 lands, the nightly's
red list is read by roles each morning (the zero-red bar, 2026-07-02).

**Ratification (Jeff, #3480 AC3)** — one of:
- **(a)** revive a cheap per-merge gate (nightly job promoted to required check,
  or a fast local pre-merge smoke), or
- **(b)** bless the stack above as authoritative as-is, with the nightly as the
  detector and #2527 as its alerting completion.

*Builder's recommendation: (b).* The per-PR hosted lane was cost-killed for
cause; the local stack has since grown the blocking test gate, merge proofs, and
witness gates that did not exist in April. The remaining honest gap is nightly
alerting (#2527), which is already carded — reviving hosted CI would buy
redundancy, not coverage.

> **RATIFIED (b) — Jeff, 2026-07-24** *(placeholder — updated at the demo GO;
> if (a) is chosen instead, this ADR gains the revived-gate design as an
> amendment before land).*

## Supersession

- **ADR-026 layer 3** ("CI — merge-to-main authoritative, branch-protected"):
  **superseded by this ADR.** Layers 1 (pre-commit) and 2 (role gates) stand.
  ADR-026's lock-file policy stands in full. A supersession banner is added to
  ADR-026 pointing here.
- **ADR-032 §3's** "CI is unchanged — branch-protected, authoritative on
  `main`" restated the ADR-026 claim; corrected in place with a pointer here.
- **CLAUDE.md** "Quality layers" section: rewritten to name the real gate.

## Consequences

- Docs and reality agree on what stands between a commit and `main`. New roles
  and agents reading CLAUDE.md inherit the true model.
- The cost of the truth: no per-PR from-scratch build exists. A red `main` is
  caught by the nightly (hours), not at merge (minutes). #2527 closes the
  alerting half; anything faster is a new decision.
- `quality.yml` remains schedule-only; its file-comment already names itself the
  interim drift lane until #2527.
