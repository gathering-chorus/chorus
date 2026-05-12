# Kade — Next Session

**Last session: 2026-05-10 17:31 → 2026-05-12 07:43 (~14h, spanned date roll)**

## State at close

- WIP: **none** (idle)
- Werk: kade/2899 was closed at /acp; canonical clean
- Outstanding stash: `pre-pull #2899 stale lifecycle state` on kade werk (activity.md +9, next-session.md mods) — pre-existing handoff drift, not authored this session, can drop on next pull

## Shipped this session

- **#2899** — chorus-hooks types.rs: rename `gate.bypass.fix_card_override` → `gate.test_override.checked` + drop hardcoded "kade" role (caller_role_for_event reads CHORUS_ROLE → DEPLOY_ROLE → "unknown"). Integration test verifies attribution end-to-end via spine tail. f59d0dc0, PR #228.
- **#2882, #2883** — closed wontdo (taxonomy already in chorus_acp; BDD coverage already in features/gates/accept.feature)
- **#2773, #2837, #2813** — premise-corrected (blast radius, count drift, BDD count)
- **#2892** — closed wontdo on Silas's clock-recalibration design (>12h threshold + system-reminder injection inherits the failure mode it's supposed to fix)

## Gates run for peers

- **#2891** (Silas, observer.error) — gate:code FAIL on +3 dead-code warnings → re-run PASS after #[allow(dead_code)] fix; gate:quality PASS retroactively
- **#2900** (Silas, chorus_design_refresh) — gate:code + gate:quality PASS; re-validated after autoConform scope expansion (27/27)
- **#2908** (Silas, three subtractive bugs) — gate:code + gate:quality PASS

## Open threads to pick up

- **chorus_acp false-positive fast-path bug** — Wren filing as P1 in version-control domain (my domain). Repro: card reopened post-merge, new commits on fresh branch, /acp returns `already-merged: true` keying on card-id-ever-merged not `git rev-list origin/main..HEAD`. Worse: fast-path also runs branch-close → orphans new commits (Wren reflog-recovered 3x in an hour). AC sketch in my reply nudge: detection switches to rev-list count, branch-close gated on three conditions including reflog-recency check.
- **3-min /acp observation** — Jeff named this. Hypothesis: same bug as above (false-positive fast-path doing expensive `gh pr list` lookups). Suggest measuring via Loki chorus_acp.* timestamp deltas before splitting into separate card.
- **Pre-existing flake** — `tests/server-unit.test.ts:84 POST /api/chorus/embed no body` times out at 5s under full-suite parallel load (LanceDB embed timeout). Confirmed on main and Silas's werks. Worth a follow-on to bump timeout or move embed-touching tests to serial.
- **Schema gap** — `designing/schemas/spine-events.json` doesn't register `gate.test_override.checked` (nor the old `gate.bypass.fix_card_override`). Pre-existing, surfaced by Silas's gate:arch on #2899. Easy follow-on card.

## Long career-arc conversation with Jeff

Jeff reflected on the WMS/EXE Dallas Systems work + Anzo/Cambridge Semantics + canonical-model 25 years of priors being reenacted at AI-agent speed. Substrate failures compound at agent-emit rate that humans never hit. Memory file `feedback_recalibrate_clock_on_date_roll.md` came out of the date-roll incident (both Wren and I missed the 19:30 ETA crossing midnight). Saved. Pattern this session: substrate is finally observable enough that the variance shows — not new variance, just newly visible.

## Boot recommendation

Pull the chorus_acp fast-path fix when Wren files it (P1, my domain). It's small surgical work — change detection from card-id-ever-merged to `git rev-list count = 0`, gate branch-close on three conditions, regression test. AC will be self-evident from Wren's repro.
