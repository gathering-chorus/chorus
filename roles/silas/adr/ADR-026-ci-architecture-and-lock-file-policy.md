# ADR-026: CI architecture + lock-file policy

**Status:** Accepted — 2026-04-25. **Layer 3 (hosted-CI merge-to-main authority) SUPERSEDED by [ADR-053](ADR-053-merge-gate-authority-on-main.md)** — hosted runners cost-killed 2026-04-29 (#2600), per-PR triggers disabled, branch-protection required-checks emptied; the authoritative merge gate is the local werk stack (werk-merge content-verify + act/werk.yml + role gates). Layers 1–2 and the lock-file policy stand. Wren PM-review APPROVED with 3 edits applied. Kade impl-review APPROVED with 6 §b deltas applied. Jeff final pending repo state (CI workflow lives on `kade/2481-ci-ratchet` branch).
**Card:** unblocks #2481 (CI ratchet enforcement); informed by #2475 (MCP observability).
**Supersedes:** establishes the rule for the first time.

## Context

#2481 was reflectively filed with AC referencing "the CI pipeline." Pulled by Kade, who wrote `.github/workflows/quality.yml` and opened PR #1. CI went red on first push because `package-lock.json` is in `.gitignore` — `npm install` on the runner resolved a slightly newer `@typescript-eslint/eslint-plugin`, fired a `no-floating-promises` rule that doesn't fire locally, and lint-ratchet failed. Local-vs-CI divergence on a clean repo. Jeff stopped the line: no design existed for what CI is for, what it owns, what it doesn't, and how it stays reproducible.

This ADR makes four decisions. The bar is **decisions made**, not pretty prose.

## Decision (a) — Layer relationship

Three quality layers. Each owns a different question with a different threat model.

| Layer | Question it answers | Threat model | Authority | Skippable? |
|---|---|---|---|---|
| Pre-commit hooks | "Will this commit obviously break something?" | Honest mistakes (typo, regression, missing import) | Local fast feedback (1–10s) | Yes (`--no-verify`) |
| Role gates (gate-product / -code / -quality / -arch / -ops) | "Is this card team-acceptable?" | Cross-role coordination, demo-readiness | Card-level done | No — recorded on card |
| CI (GitHub Actions) | "Does main build cleanly from scratch?" | Reproducibility, deliberate bypasses, environment drift | Merge-to-main authoritative | No — branch-protected |

**Redundancy is intentional where threat models differ.** lint-ratchet runs in both pre-commit (catches the honest case) and CI (catches the `--no-verify` case + the local-vs-CI divergence Kade hit). Same check, two threat models, two layers. Document the redundancy with the threat model that justifies it; otherwise eliminate.

**Contract:** all three layers passing means the change is good. Two-of-three is not enough — pre-commit alone misses environment drift; CI alone misses cross-role acceptance; role gates alone miss reproducibility.

## Decision (b) — Check distribution

Two columns per layer: **today** = what's actually wired right now (per Kade's impl-review 2026-04-25); **target** = where ADR-026 says it should land. Gaps are explicit so the migration list is honest.

| Check | Pre-commit (today) | Pre-commit (target) | Role gate | CI (today) | CI (target) | Cost |
|---|---|---|---|---|---|---|
| Type-check (tsc --noEmit) | board-client + workflow-engine only | All packages with changes | gate-code (Kade) | None | All packages | <30s/pkg |
| Jest tests | board-client + workflow-engine only | All packages with changes | gate-code (Kade) | None | All packages | 5–60s/pkg |
| Cargo test (chorus-hooks) | **gap — not wired** | All packages with Rust changes | gate-code (Kade) | None | Yes | 30s–3min |
| ESLint per-rule ratchet | Yes — when TS staged | (same) | — | None | Yes — same script | 10–30s |
| Doc-coherence ratchet | **gap — not wired** (test exists at platform/tests/doc-coherence-ratchet.test.sh, unwired) | When CLAUDE.md fragments staged | — | None | No (no remote effect) | <2s |
| MCP tool description shape (#2475) | Yes — when mcp/*.ts staged | (same) | — | None | Yes — same lint | <1s |
| Catalog oversize (#2461) | Yes — always | (same) | — | None | Yes — backstop | <1s |
| Principle direct-edit (#2314) | Yes — always | (same) | — | None | Yes — backstop | <1s |
| Compiled-dist matches source (Check 5) | Yes — board-client only | Extend or retire when build moves | — | None | (same) | <2s |
| Smoke check (#2229) | No (heavy) | (same) | **Decision needed — see below** | None | Yes — on PR | 30–90s |
| Alerts (proving/domains/alerts/*.yml) | No | (same) | gate-ops (Silas) | No — LaunchAgent runtime | (same) | 5–15min cadence |
| MCP tool round-trip (mcp-*.test.sh) | No (needs running api) | (same) | gate-quality (Kade) | Conditional — only if api boots in CI; else skip | (same) | 5–10s |
| Demo gate fixtures | No | (same) | /demo skill | No | (same) | Manual |

**Source of truth note:** this table is a draft-time snapshot. Authoritative source is `platform/hooks/pre-commit` (pre-commit) + `.github/workflows/quality.yml` (CI). It will drift; treat as the design intent at sign-time, not the running spec.

## Decision (e) — Smoke-check ownership

Today smoke-check.sh runs cron-driven via `daily-review-quality.sh`, not bound to any gate. Two candidate paths:
- **(A) gate-quality invokes it** — `/gate-quality` skill calls smoke-check on the changed surface; quality gate fails if smoke fails. Aligns with the layer model (gate-quality owns "is this card team-acceptable?").
- **(B) cron-only + CI mirror** — leave the cron, add CI step on PR. Drop the gate-quality claim entirely.

**Decision: (A) for gates + (B) for CI** — gate-quality covers the demo path; CI covers the merge path; cron stays as the operational-heartbeat layer. Sub-AC under #2481: "wire smoke-check into /gate-quality skill."

**Migrations to do** (these become #2481 sub-AC, not new cards):
- Pre-commit: extend Check 1 (tsc) and Check 2 (jest) from `board-client + workflow-engine` to all changed TS packages.
- Pre-commit: wire `cargo test` for staged Rust.
- Pre-commit: wire `doc-coherence-ratchet.test.sh` for staged CLAUDE.md fragment changes.
- Gate-quality: wire smoke-check.sh invocation per the (A) decision above.
- CI: every check in the "Yes" column of CI(target) gets a corresponding step in `.github/workflows/quality.yml`.

**Stay local-only:** alerts (LaunchAgent operational), demo gate fixtures (manual review), MCP round-trips that need a running api (CI conditional only).

## Decision (c) — Lock-file policy [load-bearing]

**Decision: commit `package-lock.json` for every package. Use `npm ci` in CI.**

Reasoning:
- Today's gitignore was anti-pattern. The diff noise was the stated cost; the real cost was non-reproducibility — Kade's CI failure proves it.
- Lock files ARE the reproducibility contract. `npm install` resolves dep ranges fresh; `npm ci` installs exactly what the lock says. CI without a lock is theater.
- Multiple package.json files (12 of them today, of which 6 are active TS projects per the consolidation memory; the other 6 are sibling tooling/SDK/spike directories) means multiple lock files. That's the price of polyrepo-in-monorepo until the TS-project consolidation lands. Renovate / Dependabot manages updates centrally.
- ESLint baseline + lint-ratchet require deterministic dep versions. Without that, the baseline is fictional and the ratchet flickers.

Implementation:
1. Remove `package-lock.json` from `.gitignore`.
2. Run `npm install` in each package once to generate the lock; commit the locks.
3. CI workflow uses `npm ci` (not `npm install`) to install from lock.
4. Renovate config (or Dependabot) manages weekly lock updates as PRs.
5. Lock-file changes are reviewed PRs like any code change.

**Cost:** lock files add ~50–150KB per package, ~1MB total. Diff noise on dependency updates. Real but acceptable. The chorus-api lock alone has ~600 transitive deps; that diff IS the truth of what's running.

**Source-of-truth note:** the 12-files-of-which-6-active count above is a draft-time snapshot. Authoritative source is `find . -name package.json -not -path '*/node_modules/*'`. The count revisits when the project_ts_project_consolidation work closes (memory-tracked); update §c then.

## Decision (d) — Red-main posture

**Two complementary postures:**

1. **Proactive — branch protection on `main`.** *[SUPERSEDED 2026-07-24, ADR-053 — this switch was flipped OFF in the 2026-04-29 cost-kill and required-checks emptied; retained as the historical design.]* PRs cannot merge if CI is red on the PR. Required reviewers can be 1 (Jeff or any role). Flip the switch in GitHub repo settings — admin task; ADR closure asks Jeff to enable.
2. **Reactive — alert if main goes red.** New `proving/domains/alerts/ci-main-red.yml` polls the GitHub Actions API for last main run; fires if status is failure. Routes to silas via the existing nudge action pattern. No auto-revert.

**Out of scope:**
- Auto-revert: clobbers unrelated work on flake. Veto.
- Pager / phone notification: silas + alert pattern is sufficient for the cadence Jeff actually moves at.

## Consequences

- **#2481 unblocks** with revised AC: "implements ADR-026 §a–d." Per Wren's ankle-biters call, all follow-on units fold into #2481's revised AC rather than spawning new cards.

**Sequence** (Kade impl-review 2026-04-25): locks → CI workflow completeness → green run on a real PR → THEN Jeff toggles branch protection. The reactive alert (ci-main-red) and pre-commit-widening sub-AC can land in parallel. Branch protection BEFORE CI is complete creates a worst-of-both window: main is protected by an incomplete CI.

  - sub-AC (1): package-lock.json files generated + committed for each active TS project; `.gitignore` updated to remove the lock-file ignore.
  - sub-AC (2): `.github/dependabot.yml` (or Renovate equivalent) wired for centralized lock-update PRs.
  - sub-AC (3): CI workflow `.github/workflows/quality.yml` has a step for every check listed Yes in the §b CI(target) column.
  - sub-AC (4): green CI run on a real PR proves (1)–(3) end-to-end.
  - sub-AC (5) **[Jeff-action, asynchronous]**: branch protection enabled on `main` requiring the CI workflow to pass — admin task, can't be scripted from a role. Lands AFTER (4). Card closure does NOT block on this — builder marks complete on (1)–(4); the toggle is a separate human step that closes ADR-026 in practice but not the card.
  - sub-AC (parallel): `proving/domains/alerts/ci-main-red.yml` polls GitHub Actions API, fires on red main, routes to silas.
  - sub-AC (parallel): gate-quality wires smoke-check (per §b open decision (A)).
  - sub-AC (parallel): pre-commit Check 1+2 extended from board-client/workflow-engine to all changed TS packages.
  - sub-AC (parallel): pre-commit wires cargo test for staged Rust.
  - sub-AC (parallel): pre-commit wires doc-coherence-ratchet for staged CLAUDE.md fragment changes.
- The CI workflow currently lives on branch `kade/2481-ci-ratchet` (the §c proof-of-concept). Either accepted with the ADR or superseded if the workflow needs a substantive rewrite per the revised AC.

## Dependencies

- **Blocks (now unblocks):** #2481.
- **Informs:** #2475 (the description-shape lint already lives in pre-commit; CI mirror is part of the §b table migration).
- **Future:** workspace consolidation card — 12 package.json files is accretion, not architecture. Not in scope here, but lock-file reproducibility raises the cost of leaving it.

## Review

- Silas: drafted, 2026-04-25.
- Wren: PM-review APPROVED 2026-04-25 with 3 edits — (1) §b drift-warning line; (2) follow-on chores fold into #2481 sub-AC, not new cards; (3) reconcile §c "12 package.jsons" with the project_ts_project_consolidation memory's "6 active TS projects." All applied.
- Kade: impl-review APPROVED 2026-04-25 with 6 §b deltas — (1)(2) tsc + jest narrowed to actual today-state (board-client + workflow-engine only); (3)(4) cargo test + doc-coherence-ratchet labeled as gaps not wired; (5) smoke-check open-decision section with recommendation; (6) Compiled-dist Check 5 added. All applied.
- Jeff: final sign-off + branch-protection toggle pending.

## Action on close

When ADR is signed:
1. #2481 returns to Next with AC rewritten as 4 sub-AC implementing ADR-026 §a–d (per Consequences above).
2. Jeff toggles branch protection on `main` (admin task, can't be scripted from a role).
3. Builder of #2481 picks up all four sub-AC; closure of the card closes ADR-026 in practice.

## Addendum 2026-04-25 — Smoke-check CI ownership revised (Jeff + Kade, during #2487)

§b CI(target) listed smoke-check as "Yes — on PR." Revised: **No — chorus CI does not run smoke-check.**

The Gathering app lives in `jeff-bridwell-personal-site` (separate repo). Booting it inside chorus CI would require either checking out a sibling repo or maintaining a contract-test mock — both couple chorus CI to a surface chorus does not own. The app's own CI is the right place for smoke-check; chorus CI stays focused on chorus-native artifacts (tsc, jest, cargo, lint-ratchet, MCP shape, MCP round-trip).

Decision (e) "(A) for gates + (B) for CI" reduces to **(A) for gates only**: `/gate-quality` invokes smoke-check locally when the app is up. CI does not.

Consequence: #2487 (api-boot-in-CI) scope reduces to MCP round-trip + ephemeral Fuseki seed only. Smoke-check + cross-repo coordination drops out of #2487 AC #4–5.
