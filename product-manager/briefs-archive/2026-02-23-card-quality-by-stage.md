# Brief: Card Quality by Value Stream Stage

**From:** Wren (PM)
**To:** All roles (Wren implements upper third; Kade middle; Silas lower)
**Date:** 2026-02-23
**Card:** C#57
**Priority:** P1
**Depends on:** Product decomposition (`chorus-product-decomposition.md`), spine visualization (`chorus-spine.html`), card lifecycle (`werk-process.html`), Silas gate audit (209 rules / 18 enforced)

---

## Purpose

Define what a high-quality Vikunja card looks like at each stage of the value stream. This is the spec that every other spine improvement depends on: you cannot enforce gates if you haven't defined what "ready" means at each transition.

Jeff said it directly: "I honestly have no idea if you all are following the same gates or not." This brief answers that by making card quality concrete, measurable, and — where possible — machine-enforceable.

---

## Vikunja Fields We Use

These are the fields available per card. Everything we define below maps to these.

| Field | Vikunja API | board-ts access | Notes |
|-------|-------------|-----------------|-------|
| **Title** | `title` | `--title` | Free text. We enforce format conventions below. |
| **Description** | `description` | `--desc` | Markdown. The "what and why" body of the card. |
| **Bucket** (status) | `bucket_id` | `move <id> <status>` | Later, Next, Now, WIP, Blocked, Done, Jeff Tickets, Tech Debt |
| **Labels** | `labels[]` | Auto-parsed on add | `owner:Wren`, `P1`/`P2`/`P3`, `product` label (`chorus`/`gathering`) |
| **Comments** | `/tasks/{id}/comments` | `comment <id> "text"` | Append-only audit trail. Work log lives here. |
| **Relations** | Vikunja supports but board-ts does not yet expose | — | `blocks`/`blocked-by`. Future: expose via CLI. |
| **Created/Updated** | `created`, `updated` | Automatic timestamps | Used for staleness detection and cycle time on Value Stream page. |

**Not available in Vikunja (tracked elsewhere):** Workflow manifests (`workflows/active/`), briefs (`<role>/briefs/`), commit references (git log), test evidence (CI/deploy logs), Clearing transcripts (`chorus/clearing/transcripts/`).

---

## Stage 1: Capturing (seed --> card)

**Transition:** An idea becomes a tracked work item.
**Owner:** Wren (or any role creating a card).
**Board state:** Card lands in **Later** or **Next**.

### Required Fields

| Field | Requirement | Example |
|-------|-------------|---------|
| **Title** | Action verb + noun + context clause. Under 80 chars. | `Card quality by value stream stage — define what a good card looks like at each vertebra` |
| **Owner label** | `owner:Wren`, `owner:Kade`, or `owner:Silas` | — |
| **Priority label** | `P1`, `P2`, or `P3` | — |
| **Product label** | `chorus` or `gathering` (omit = Gathering default) | — |

### Title Format Convention

```
<verb> <what> — <why or scope>
```

Good: `Card quality by value stream stage — define what a good card looks like at each vertebra`
Good: `Clearing voice tuning — /chorus context injection for Haiku sessions`
Bad: `fix the thing`
Bad: `WF-035 step 2` (workflow artifact, not a card title)

### Guidance (not enforced)

- If the card originated from a seed (SMS capture), include `[seed]` prefix or link to the seed in the description.
- If it's a spike, include `Spike:` prefix in the title. Spikes have time-box in the description.
- Description is optional at capture — it gets filled during Directing.

### Enforceable Gates

| Check | Mechanism | Status |
|-------|-----------|--------|
| Card must have owner label | board-ts `add` requires `--owner` | **Enforced today** (CLI rejects without it) |
| Card must have priority label | board-ts `add` requires `--priority` | **Enforced today** |
| Title minimum length (>10 chars) | board-ts validation | **Not enforced** — add to CLI |
| No cards created without title verb | CLAUDE.md guidance | Guidance only |

---

## Stage 2: Directing (card --> Now)

**Transition:** Jeff pulls a card to Now. This is the moment where "should we build this" becomes "we are building this."
**Owner:** Jeff (the pull signal) + Wren (card must be ready before Jeff pulls).
**Board state:** Card moves to **Now**.

### Required Fields (before Jeff pulls)

| Field | Requirement |
|-------|-------------|
| **Description** | Must contain "what and why" — what are we building, why does it matter. Minimum: 2 sentences. |
| **Acceptance criteria** | At least one sentence in the description defining "done." Can be as simple as "Users can see X on the Y page." |
| **Work type** | Description must tag the work: `[spike]` (time-boxed, exit = learning), `[discovery]` (learning through doing, exit = decision), or `[commitment]` (defined outcome, exit = shipped). |
| **Dependencies** | If the card blocks or is blocked by another card, state it in the description. Format: `Depends on: #N` or `Blocks: #N`. |
| **Effort signal** | Description should include a rough size: `[small]` (< 1 session), `[medium]` (1-3 sessions), `[large]` (> 3 sessions, consider splitting). |

### Example Card Ready for Now

```
Title: Card quality by value stream stage — define what a good card looks like at each vertebra
Owner: Wren | Priority: P1 | Product: chorus

Description:
[commitment] [medium]

Define what a high-quality card looks like at each value stream stage (Capturing,
Directing, Building, Proving). This is the spec that C#56 (spine rewrite) and #222
(manifest-first gate) depend on — you can't enforce quality gates if you haven't
defined what quality means.

Acceptance criteria: Brief shipped to all roles with stage-by-stage field requirements,
enforceable gates identified, and gap analysis of current vs target state.

Depends on: chorus-product-decomposition.md, Silas gate audit brief
Blocks: C#56 (spine rewrite quick wins)
```

### Enforceable Gates

| Check | Mechanism | Status |
|-------|-----------|--------|
| Description non-empty before move to Now | board-ts `move` pre-check | **Not enforced** — add to CLI |
| Acceptance criteria present (scan for "criteria" or "done when" in description) | board-ts `move` pre-check or session-start warning | **Not enforced** — add to CLI |
| Work type tag present | CLAUDE.md guidance + session-start lint | **Not enforced** — start as warning |
| Manifest auto-created on move to Now | WorkflowEngine trigger (C#52) | **Enforced today** |
| Brief auto-routed to owner | Workflow engine on manifest create | **Enforced today** |

---

## Stage 3: Building (Now --> WIP --> workflow advancing)

**Transition:** A role picks up the card and does the work.
**Owner:** The assigned role (usually Kade for features, Silas for infra, Wren for product artifacts).
**Board state:** Card moves **Now --> WIP** when role starts. Stays in WIP until work complete.

### Required Actions During Build

| Action | Where it lives | Required? |
|--------|---------------|-----------|
| **Move to WIP** | `board-ts move <id> WIP` | Yes — immediately on starting work |
| **Card comments as work log** | `board-ts comment <id> "..."` | Yes — at minimum: start comment, completion comment. Intermediate updates for multi-session work. |
| **Commit references** | Git commit messages | Yes — every commit includes `(#N)` for Gathering cards or `(C#N)` for Chorus cards |
| **Brief to reviewer** | Auto-generated by workflow engine on `workflow.sh advance` | Automatic |
| **Test evidence** | In the advance notes or brief | Yes for code changes. Format: `Tests: 2305 passing, 38 new` |
| **Artifacts list** | In the advance notes or completion comment | Yes — list files changed, briefs written, decisions made |

### Comment Convention During Build

```
[start] Picking up C#57. Reading decomposition doc and gate audit brief.
[progress] Stage definitions drafted for Capturing and Directing. Building still in progress.
[done] Brief shipped to all roles. 4 stages defined, 12 enforceable gates identified, gap analysis complete.
```

### Enforceable Gates

| Check | Mechanism | Status |
|-------|-----------|--------|
| Card must be in WIP (not Now) while role is working | Session audit: stale Now detection (>48h) | **Enforced today** (staleness warning) |
| Workflow manifest must exist | Manifest-first gate (#222) | **Building** — auto-trigger on Now works, manual fallback needed |
| Commit messages reference card number | Pre-commit hook (team repo) or git log lint | **Not enforced** — add as pre-commit check |
| Workflow step advanced before handoff | `workflow.sh advance` required for next step to unlock | **Enforced today** |
| Card not stuck in WIP >48h without comment | Session audit staleness | **Enforced today** (warning only) |

---

## Stage 4: Proving (WIP --> Done)

**Transition:** Work is complete, reviewed, verified, deployed (if applicable), and the card is closed.
**Owner:** Reviewer (usually Silas for code, Wren for product artifacts). Final `done` by the role or Jeff.
**Board state:** Card moves to **Done**.

### Required Evidence for Done

| Evidence | Where | Required? |
|----------|-------|-----------|
| **Review brief received and read** | Reviewer's `briefs/` directory | Yes — for multi-role work. Solo work: self-review comment on card. |
| **Acceptance criteria verified** | Comment on card: "AC met: [restate criteria] — verified [how]" | Yes |
| **Deploy evidence** (for code changes) | Comment: "Deployed via app-state.sh. Health check passing. LAN smoke test passing." | Yes for code |
| **Workflow complete** | All steps advanced, manifest archived to `workflows/archive/` | Yes — automatic on final advance |
| **Card moved to Done** | `board-ts done <id>` | Yes — immediately when done, not at session close |

### Anti-Patterns This Stage Catches

- Card sits in WIP after work is done (should be in Done immediately)
- No review comment — card just moved to Done with no audit trail
- "Done" but no deploy evidence for code changes
- Workflow still active when card is already Done (state mismatch)

### Enforceable Gates

| Check | Mechanism | Status |
|-------|-----------|--------|
| Workflow must be complete before Done | board-ts `done` pre-check: warn if active workflow exists | **Not enforced** — add to CLI |
| Card must have at least 1 comment before Done | board-ts `done` pre-check | **Not enforced** — add to CLI |
| No retroactive cards (created + moved to Done in same session) | Session audit-close diff | **Enforced today** |
| Done cards must have updated timestamp after created timestamp (work actually happened) | Session audit | **Enforced today** (implicit via staleness) |

---

## Cross-Cutting: Every Stage

These apply regardless of where the card is in the pipeline.

| Concern | How | Status |
|---------|-----|--------|
| **Comments as audit trail** | Every significant action gets a card comment. Not a journal — just enough to trace what happened and why. | Guidance |
| **Clearing transcript links** | If a Clearing session discussed the card, add a comment: `Clearing session: YYYY-MM-DD — [decisions summary]` | Guidance |
| **Brief references** | If a brief was written for the card, add a comment: `Brief: <role>/briefs/<filename>` | Guidance |
| **Time badges on Value Stream page** | Lead time (created → Done), wait time (Now → WIP), cycle time (WIP → Done) — all computed from Vikunja timestamps | **Shipped** (#233) |
| **Card-first gate** | No work without a card. Session audit enforces. | **Shipped** (C#44) |
| **Chorus index** | Card-related artifacts (briefs, transcripts, commits) are indexed and searchable via `/chorus search` | **Shipped** (#140 ambient indexing) |

---

## Gap Analysis: Today vs Target

### What We Have Today (working)

1. **Card-first gate** — session audits catch missing cards and retroactive cards
2. **Staleness detection** — Now >48h, Next >7d flagged with age labels
3. **Manifest auto-creation** — WorkflowEngine triggers on move to Now
4. **Brief auto-routing** — workflow engine writes briefs on step advance
5. **Workflow state tracking** — active/archive lifecycle, step advancement
6. **Time badges** — lead time, wait time, cycle time on Value Stream page
7. **Ambient indexing** — all session artifacts searchable within 3 seconds
8. **Owner and priority labels** — board-ts CLI enforces on `add`

### What's Missing (the gap)

| Gap | Stage | Fix | Effort |
|-----|-------|-----|--------|
| No description check on move to Now | Directing | board-ts `move` pre-check | Small — CLI change |
| No acceptance criteria detection | Directing | board-ts lint or session-start warning | Small |
| No work type tag convention | Directing | CLAUDE.md guidance + session-start lint | Small (guidance first) |
| No commit-to-card reference enforcement | Building | Pre-commit hook regex for `(#N)` or `(C#N)` | Small |
| No minimum comment check on Done | Proving | board-ts `done` pre-check | Small — CLI change |
| No workflow-complete check on Done | Proving | board-ts `done` warns if active workflow exists | Medium — needs workflow.sh query |
| No card relations (blocks/blocked-by) | Cross-cutting | board-ts `relate` command wrapping Vikunja relation API | Medium |
| Title format linting | Capturing | board-ts validation or session-start lint | Small |
| Effort signal / size tagging | Directing | CLAUDE.md convention, not enforced | Guidance only |

### What's Actively Harmful (fix first)

1. **Pre-push hook runs 2300 tests** — blocks all roles for 1-2 min, has corrupted sessions.db twice. Slim to lint + critical tests. (C#56 scope)
2. **Gate inconsistency across roles** — Kade has infra-guardrails, others don't. Single hook manifest needed. (C#56 scope)
3. **191 CLAUDE.md rules with zero enforcement** — identify the ~20 highest-impact ones that can become hooks or CLI checks. (C#56 scope)

---

## Head-to-Base Sequencing

The spine has three thirds. We fix them in order: top down. One surgery at a time.

### Phase 1: Upper Third (Wren) — Capturing + Directing

**Owner:** Wren
**Timeline:** This week
**Goal:** Every card that enters Now is well-formed: title, description, acceptance criteria, work type, effort signal, owner, priority, product label.

Actions:
1. Update CLAUDE.md card-first gate fragment with title format, description requirements, work type tags
2. Add board-ts `move` pre-check: warn if description is empty when moving to Now
3. Add board-ts `add` validation: title minimum length
4. Backfill: audit current Now/Next cards and fill gaps
5. Ship this brief as the reference spec

**Exit criteria:** Every card that moves to Now this week meets the Directing quality bar. If it doesn't, Wren catches it before Jeff pulls.

### Phase 2: Middle Third (Kade) — Building

**Owner:** Kade (after Phase 1 delivers clean inputs)
**Timeline:** Next week
**Goal:** Every card in WIP has a comment trail, commit references, and workflow advancement.

Actions:
1. Pre-commit hook: regex check for `(#N)` or `(C#N)` in commit messages (team repo)
2. CLAUDE.md fragment: comment convention (`[start]`, `[progress]`, `[done]`)
3. board-ts: show warnings on `audit-close` if WIP cards have zero comments
4. Workflow engine: verify manifest exists when role moves card to WIP

**Exit criteria:** Every completed card has at minimum a start and done comment, commit references, and a workflow that advanced through its steps.

### Phase 3: Lower Third (Silas) — Proving

**Owner:** Silas (after Phase 2 delivers cards with build evidence)
**Timeline:** Week after next
**Goal:** Every card that moves to Done has review evidence, deployment verification, and workflow completion.

Actions:
1. board-ts `done` pre-check: warn if no comments, warn if active workflow exists
2. CLAUDE.md fragment: review comment convention ("AC met: ..., verified: ...")
3. Deploy verification: LAN smoke test reference in done comment for code cards
4. Workflow archive validation: manifest must be in `archive/` when card is Done

**Exit criteria:** No card reaches Done without a review trail. Workflow state and card state are always in sync.

---

## Success Metrics

After all three phases ship:

1. **Zero retroactive cards** — every card exists before work starts (already enforced)
2. **100% of Now cards have description + acceptance criteria** — measured by session audit
3. **100% of Done cards have at least 1 comment** — measured by board-ts audit-close
4. **Commit-to-card reference rate >90%** — measured by pre-commit hook pass rate
5. **Workflow-card state sync: 100%** — no cards in Done with active workflows, no archived workflows with cards still in WIP

---

## Decision Requested

Jeff: Approve this as the card quality spec. Roles will implement in phase order (upper third first, then middle, then lower). Board-ts CLI changes are small and non-breaking — they add warnings, not hard blocks, during the rollout period. Hard enforcement comes after one week of warning-mode operation.

---

*Wren | Product Manager*
