# Per-Role Worktree Convention — RETIRED 2026-05-01 (`#2640`)

The per-role sibling-worktree convention (`#2582` v1) was retired by `#2640` on 2026-05-01. Sibling worktrees `chorus-kade/`, `chorus-wren/`, `chorus-silas/`, and `chorus-2526/` were removed; all roles operate from canonical `/chorus/roles/<role>/`.

A v2 redesign (`chorus/.worktrees/<role>-<hash>/` + mandatory SessionStart cwd enforcement) was drafted and rejected the same day. The protected primitive: **role-directory IS session-start.** Any mechanism that moves session-start away from `/chorus/roles/<role>/` is in the rejected family.

## Mode (A) read-stale-content — accepted-residual-risk

`/chorus/.git/HEAD` is one file shared across the team. A peer's `git checkout` can rewrite disk content under your reading session. **The team has chosen to absorb this risk** rather than pay the physical-isolation cost (worktree mechanism). Watch via spine events; treat 4-29-class contamination incidents as metric data, not regressions.

## Modes (B) and (C) — defended at the gates

- **(B) cross-role commit:** `git-queue` branch-check (`#2580`) + pre-push hook (`#2598` / `#2625` / `#2639`). Shipped.
- **(C) same-role wrong-card:** active-card-id strict check, sequenced as `#2641` (P1 follow-on).

## Related

- `version-control-service-design.html` — names this retirement (renamed from `commits-service-design.md` by `#2683`).
- `#2640` — implementation of the retire (hook retire + this fragment).
- `#2641` — mode (C) follow-on.
- `#2582` — v1 convention, retired by this fragment.
