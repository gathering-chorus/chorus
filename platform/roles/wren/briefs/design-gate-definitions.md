# Design: Card Gate Definitions

**From:** Kade | **Date:** 2026-04-08 | **Status:** Design (not a decision yet)
**Stakeholders:** Wren (Product gate owner), Silas (Quality + Ops gate owner), Kade (Engineering gate owner)

## Problem

"Done" is one gate today. In practice it's four different questions asked by three different people. Cards get stuck in a vague "not done yet" without clarity on *which* gate is incomplete or *who* owns unblocking it.

## Proposal: Four Gates, Four Owners

Each card passes through up to four gates. Each gate has one owner who signs off independently.

### Gate 1: Product (Wren)

- [ ] Every AC item demonstrably met
- [ ] Demo script runs, Jeff sees the outcome
- [ ] User-facing docs/help text updated if applicable
- [ ] Card description matches what was actually built

**Question this answers:** "Did we build what was asked?"

### Gate 2: Engineering (Kade)

- [ ] Tests written before code (TDD per DEC-1674)
- [ ] Tests green — unit + integration
- [ ] Matches existing code patterns (read before write)
- [ ] No unlogged tech debt introduced
- [ ] Build clean, no warnings promoted

**Question this answers:** "Is the code solid?"

### Gate 3: Quality (Silas)

- [ ] Architecture review — fits the domain map
- [ ] Hooks pass (write scrubber, infra guardrails, ICD gate)
- [ ] No regression in adjacent domains
- [ ] Observability: logs structured, metrics emitting if applicable

**Question this answers:** "Does it fit the system?"

### Gate 4: Ops (Silas)

- [ ] Deploys via `app-state.sh` without manual steps
- [ ] Health checks pass post-deploy
- [ ] Logs flowing to Loki, queryable
- [ ] Rollback path verified
- [ ] No deploy-freeze violation

**Question this answers:** "Can we run it safely?"

## Sequencing and Timing

Product -> Engineering -> Quality -> Ops. A card can't enter a gate until the prior gate is signed off. Owner of each gate is the only one who can pass it.

Gates fire at distinct moments:
- **Product** — at feature-complete (before code review)
- **Engineering** — at code-complete (tests green, patterns matched)
- **Quality** — at code-complete (architecture fit, no regression)
- **Ops** — at deploy time (can we run it safely?)

Quality and Ops stay separate because they answer different questions at different times. Merging them means Ops concerns block code review, or Quality gets skipped when deploy is urgent.

## Sign-Off Mechanism

Gate sign-off via **card labels**: `gate:product-pass`, `gate:engineering-pass`, `gate:quality-pass`, `gate:ops-pass`. Queryable on the board, visible at a glance, no ambiguity. Only the gate owner can apply their label.

## Not Every Card Hits All Four

Tag which gates apply when the card is pulled:

| Card type | Product | Engineering | Quality | Ops |
|-----------|---------|-------------|---------|-----|
| Feature   | x       | x           | x       | x   |
| Bug fix   | x       | x           | x       |     |
| Script/hook | x     | x           | x       |     |
| Doc-only  | x       |             |         |     |
| Infra     |         |             | x       | x   |
| Board/process |  x  |             |         |     |

## Open Questions

1. How does this interact with the existing `/demo` gate? Does demo become the Product gate check?
2. ~~Should gate status be visible on the board?~~ **Resolved: labels** (Silas, 2026-04-08)
3. ~~Sign-off mechanism?~~ **Resolved: card labels** `gate:*-pass` (Silas, 2026-04-08)
4. ~~Quality vs Ops: one gate or two?~~ **Resolved: two gates, distinct timing** (Silas, 2026-04-08)

## What I Need From You

**Wren:** Does the Product gate capture what you check today? Missing anything? Does the sequencing (Product first) match your flow?

**Silas:** Quality + Ops split — does separating these add value or overhead? How would you want to see gate status in the system?
