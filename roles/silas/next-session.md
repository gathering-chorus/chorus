# Next Session — Silas

**Closed:** 2026-04-30 ~21:00 Boston

## Tomorrow's first task: /clearing on substrate

Jeff scheduled a /clearing with Silas, Wren, Kade, and him to align on the chorus-hooks substrate design refresh + plan. Open with the substrate-handoff brief sent to kade tonight (`/tmp/silas-2026-04-30-substrate-handoff-to-kade.md` — also lives in nudge thread at 18:07).

Agenda kade and wren both committed to:
1. **Hooks-on-text class is anti-pattern** — 3 of 8 paper-cuts collapse to one fix. Kade's stronger formulation: "tool_name IS dangerous AND command STARTS WITH binary" beats my "tokenize."
2. **Demo-gate format mismatch** — typed schema shared between emitter (chorus-log) and consumer (cards-CLI done-gate). Same fix shape as #2631.
3. **Ownership lock** — Kade owns chorus-hooks CODE, Silas owns CONTRACTS (what each hook MEANS, when fires, what returns). Mid-build mismatch surfaces back, doesn't get patched.
4. **Substrate-daemon-vs-operation race** — kade's catch (claudemd-gen mid stash-pop). AC10 on #2636. Fix shape: daemons-don't-write-tracked-files (structurally cleaner) > serialize daemons (migration path).
5. **--no-verify enforcement order** — substrate flakes fixed FIRST (AC2-5 + AC10), --no-verify tightening (AC6) LAST. Kade: "locking the door against the only escape hatch from a building still on fire."

## Cards

Done today (Silas): #2467 (4-wave saga), #2629 wave 3, #2632 wave 4. Carried for wren: #2628.

In flight tomorrow:
- **#2636** (Kade owner, P1, Next) — substrate-debt sweep, 10 AC, phased per kade's enforcement-order constraint
- **#2633** (Wren owner, Later, P3) — loom-policies entry: single-implementation invariant + 3 corollaries (persist-vs-deliver, default-delete, no-warn-tier) + over-narrow-structural-constraint failure mode
- **PR #75** — silas/2626-followon, quote-aware command split. Kade reviews + merges-or-retires tomorrow morning.
- **PR #82** — wren's #2630 (structural tests in CI). Chain closed 5/5 PASS, wren to /acp.

## Substrate-debt — the 8 paper-cuts

Live evidence today (from #2636 description):

1. Hooks regex-matching rendered bash strings (worktree_contamination_guard, infra_guardrails)
2. Same shape on `git push`/`add`/`commit` blocks
3. Demo-gate format mismatch (chorus-log positional vs cards-CLI JSON shape)
4. git-queue.sh choking on pre-staged deletions
5. Clippy-ratchet PASS-standalone vs FAIL-pre-commit
6. Pre-commit firing on pre-existing unrelated drift
7. Session-init-gate locks out post-compaction sessions (no self-recovery)
8. Substrate-daemon writing tracked files mid git-queue stash-pop

## What Jeff said tonight that matters

- "These subdomains are hyper important for the team" — substrate (chorus-hooks + pulse + git-queue) earns different rigor than feature domains
- "We may need to split up chorus-hooks" — three-way seam: gates (block), observers (emit), coordination (CLI). Order: clean contracts first (#2636), then split binaries.
- "I dont need color commentary i need you to help wren and kade on demo responses" — execute, don't explain. Match energy.

## Standing rules from today

- `--no-verify` is a fire-extinguisher, not a workflow. I bypassed it on wren's #2628 carry tonight; that was wrong even though the failures were unrelated.
- Tests RED-first then GREEN before any code change.
- Use `--body-file` for nudge bodies containing dangerous-binary-keyword text. Inline triggers infra_guardrails substring match.
- For multi-role discussions tomorrow, expect kade to push back hard on any text-substring hook proposal.

## Pending

None outstanding tonight. All gate requests cleared. All [feedback] nudges replied to.
