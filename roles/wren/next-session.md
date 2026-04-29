# Wren — Next Session

## Boot order — IMPORTANT

**If launched from `/chorus-wren/roles/wren`:**
- AC3 of #2583 satisfied. Verify: `pwd`, `git worktree list` (4 entries), `cat ~/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-wren/memory/MEMORY.md` returns memories (memory-continuity check via symlink)
- Coordinate AC4 with peer: ask Kade or Silas to do a `git checkout` in their session; verify my chorus-wren branch state stays unchanged
- Close #2583

**If launched from `/chorus/roles/wren` (unchanged):**
- Symlink + launcher-cwd-update + reboot still pending. Steps:
  1. `ln -s -Users-jeffbridwell-CascadeProjects-chorus-roles-wren ~/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-wren-roles-wren`
  2. Jeff updates terminal launcher to point at `/chorus-wren/roles/wren`
  3. Reboot
  4. Resume from boot order above

## Today (2026-04-29) — what landed

**Cards Done by Wren:**
- #2566 — Codify card-shape taxonomy (8 labels + TEAM_PROTOCOL section + gate-set service design with flow chart)
- #2575 — nudge + chat.sh say accept --body-file / --body-stdin (originator-side quiet)
- #2561 — Gate ceremony research v4 + 5 children filed (PR #32 merged earlier today)
- #2585 — Spike: Claude Code project-keying findings; per-role worktree viable via symlink
- #2498 — Trunk-vs-Rulesets cleanup (closed via Silas's PR #38 side-finding)
- #2195 — Worktrees-as-feature retired with chorus-2526 evidence

**Cards filed by Wren:**
- #2562 (DEC renumber to sequential, P2)
- #2578 (Independence Protocol implementation, P1)
- #2581 (receiver-side nudge rendering, P1, Rust work)
- #2583 (Wren onboards chorus-wren — paused, then reshape per #2585 spike)
- #2584 (AI-SLOP-Detector spike on chorus, P2)
- #2585 (Claude Code project-keying spike, Done)
- #2586 (Commits-domain service design, Kade-owned, P1)

**Service designs landed:**
- `/designing/docs/independence-service-design.{md,html}` — with 16 thinker folds (Galton/Asch/Lorenz/Tetlock/Bohm/Buber/Gadamer/etc.)
- `/designing/docs/gate-set-service-design.{md,html}` — with mermaid decision-tree flow chart
- About-wren v2 (process-as-product, peer-reviewed by Silas + Kade twice each)

**Cards in Wren WIP at boundary:**
- #2583 (Wren onboards chorus-wren) — AC1 + AC5 done; AC2, AC3, AC4 pending launcher-cwd-update + session-restart

## Substrate-failure roster (parent-arc input for Section 9 of #2561)

Three days now of substrate-failure pattern:
- silent-fallback (CHORUS_ROOT pre-#2505)
- bridge-layer-lies (#2573, jeff.input.delivered/failed)
- cross-branch tooling friction (#2580 stop-the-bleed; #2582 + #2583 + #2585/#2586 structural fix in progress)
- ruleset-vs-branch-protection (#2498 side-finding from #2557 verification)

Section 9 of #2561 — "Integration as first-class team activity at agent velocity" — opens as parent-arc research after #2561 implementation children land + 2-week audit. Paired with Silas + Kade.

## Open priorities going forward

- **#2583** — finish onboarding when launcher cwd updated
- **#2586** (Kade) — commits-service-design; informed by #2585 findings
- **#2581** — receiver-side rendering (Rust work; chorus-inject + chorus-hook-shim)
- **#2567 / #2568 / #2569 / #2570** — gate-set implementation children
- **KM cleanup with Jeff** — Phase 2 walk-through of Wren-owned docs (his morning ask, deferred)
- **#2562** — DEC renumber when ready

## Critical memories saved today

- `feedback_demo_feedback_not_card_generation.md` — demo feedback ≠ card-generation engine
- `feedback_dont_relay_to_jeff.md` — peer answers go to peer, not also to Jeff
- `feedback_use_body_file_for_long_messages.md` — long bodies via --body-file
- `feedback_eliminate_runtime_dep_dont_manage_it.md` — defensive substitution pattern
- `feedback_dont_share_hypothesis_mid_investigation.md` — independent RCA before convergence
- `feedback_writing_surface_over_review_surface.md` — discipline at writing surface
- `feedback_dont_trust_fallback_patterns.md` — env.unwrap_or_else is the contract IS the default

## Today's lesson

The thin-review pattern I showed on #2582 (procedural compliance instead of substantive engagement) is the failure mode I most need to watch for. Kade caught it cleanly. Substantive product engagement is what the role exists for; AC-tick-with-precedent-named is the smell to notice.

Cross-branch contamination bit three times today. Per-role worktree IS the structural fix per #2585 spike — pending session-restart to validate.
