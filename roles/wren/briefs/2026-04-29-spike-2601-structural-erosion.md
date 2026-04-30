# Spike #2601 — Structural-erosion via language-native tools

**Builder:** Kade. **Date:** 2026-04-29 (90-min time-box, ~70 min used).
**Predecessor:** #2584 (AI-SLOP-Detector v3.6.0 closed with skip — cross-language all-in-one didn't work; tool's AST mode was Python-only).

## Position

**Adopt all three.** Language-native mature tooling produces real structural-erosion signal on every chorus surface (Rust / TS / Python) without overlap with what we already gate. Single-direction recommendation, three integration points named.

## What ran (AC1)

- **Rust**: `cargo clippy --bin chorus-hook-shim -- -W clippy::cognitive_complexity` (default threshold 25). Surface: `platform/services/chorus-hooks/`.
- **TS**: `eslint-plugin-sonarjs@2` (cognitive-complexity 15, no-duplicate-string 5, no-collapsible-if, no-identical-functions, no-redundant-jump). Surface: `platform/api/src/server.ts`, `directing/clearing/src/server.ts`, `platform/pulse/src/service.ts`. Configured in `/tmp/2601-spike/ts-test/eslint.config.js`.
- **Python**: `radon cc -s` + `radon mi`. Surface: `platform/scripts/*.py` (13 files, 66 functions).

Tools NOT run for time:
- `tokei` (LOC counts) — informational only, skipped.
- `cargo machete` (unused deps) — adjacent, not structural-erosion.
- `xenon` (CI thresholding wrapper for radon) — would integrate post-adoption, not needed for the read.

## Findings (AC2)

### Rust — `clippy::cognitive_complexity` (threshold 25)

13 hits in `chorus-hooks` bin. Distribution by score:

| Cog complexity | Function (sample) | File |
|--:|---|---|
| 65 | `pub async fn check` | hook orchestrator |
| 57 | `pub async fn check` | another orchestrator |
| 47 | `pub async fn check` | |
| 42, 40, 33, 32, 29, 27, 26 | mostly `fn check` family | hooks/* |

The pattern is concentrated on hook entrypoints (`check`, `post_check`, `run`, `run_gate_smoke`, `check_pre_bg`) — orchestrators that enumerate multiple branching cases. Some legitimately do (gate-chain dispatch); others have grown organically over many cards.

### TS — `eslint-plugin-sonarjs`

Sample (3 files, ~1500 LOC):

| Rule | Hits | Where |
|---|--:|---|
| `sonarjs/no-duplicate-string` | 6 | `api-server.ts` (mostly) — repeated string literals appearing 5+ times |
| `sonarjs/cognitive-complexity` | 1 | one function over threshold 15 |
| (parser errors on `pulse-service.ts`) | 6 | sample-config issue, not signal — fixable in real integration |

`no-duplicate-string` is the dominant signal. Catches the agent-coded pattern of inlining the same literal string ("application/json", error message templates, etc.) across handlers instead of extracting a constant.

### Python — `radon`

66 functions across 13 scripts:

| Rating | Cyclomatic | Count | Examples |
|---|--:|--:|---|
| E | 31-40 | 1 | `validate_manifest` (34) in `claudemd-gen.py` |
| D | 21-30 | 1 | `main` (23) in `doc-inventory-reconcile.py` |
| C | 11-20 | 7 | `main` in clippy-ratchet, coverage-ts, doc-inventory-freshness, doc-inventory-state, doc-relocate-plan; `collect_counts` in clippy-ratchet |
| A-B | 1-10 | 57 | rest |

Maintainability Index: 12 of 13 files at A; 1 at C (`claudemd-gen.py`).

The pattern: most complexity is in `main()` functions of script-shaped Python (which is expected for one-off scripts that do parse-args → process → emit). The E-rated `validate_manifest` is a legitimate target for refactor.

## False-positive analysis (AC3)

**Real-signal hits** (rough estimate based on sampling):

- **Rust**: ~9/13. The 65/57/47 cog-complexity cases are real — orchestrator functions that have grown over time and would benefit from extraction. Some lower-scored hits (26-30) are borderline — single big match arms that are conceptually one decision but parsed as many branches.
- **TS**: ~5/7 real. The `no-duplicate-string` catches genuine drift (string literals copy-pasted instead of extracted). Cognitive-complexity hit was on a real router function with deep branching.
- **Python**: ~5/9 above-A. Script `main()` functions naturally cluster at C (parse-args + dispatch). `validate_manifest` E-rating IS real.

False-positive rate ~25-35% — acceptable for a structural-erosion signal because (a) it's surfaced as warning not blocker and (b) the tool itself catalogs the patterns, the human classifies.

## Overlap analysis (AC4)

| Tool | Already covered by | Net-add |
|---|---|---|
| `clippy::cognitive_complexity` | clippy already in pre-commit, but this lint is OFF by default | NEW signal — clippy default lints don't include cognitive_complexity. Just need to enable in `[lints.clippy]`. |
| `eslint-plugin-sonarjs` | ESLint defaults: `complexity` (cyclomatic), `max-depth`, `max-lines-per-function` already in our config | NEW patterns: `no-duplicate-string`, `no-identical-functions`, `no-collapsible-if`, `no-redundant-jump`, plus cognitive-complexity (different metric than cyclomatic). Sonarjs's value is the structural-pattern detection ESLint defaults don't have. |
| `radon` | We have NO Python complexity tooling today | All net-add — Python is a blind spot. |

Net per tool: each adds patterns the existing layers don't catch. Sonarjs is the strongest TS upgrade; radon fills a gap; clippy::cognitive_complexity activates a dormant capability.

## Recommendation (AC5)

**ADOPT ALL THREE** — single-direction position.

Reasoning:
1. **Real signal** in every language sample. The "agent-coded chorus structurally decays over time" hypothesis surfaces measurably — orchestrators grow cognitive complexity, agents inline duplicate strings, Python script `main()` accretes branches.
2. **Net-additive** to existing pre-commit and gate layers. None of these patterns are caught today.
3. **Mature tooling** — clippy is rustc-bundled; sonarjs is the SonarSource flagship for JS; radon is the de-facto Python complexity tool. No bespoke infrastructure.
4. **Threshold-tunable** — start at sonarjs cognitive-complexity 15, clippy 25, radon D-rating warn / E-rating block. Tune as we learn what's real-signal vs false-positive in our codebase.

The opposite reading (skip): each tool adds noise to gate output, ratchet baselines need maintenance, false-positive rate is real. But these are the costs of **measurement** — and the cost of NOT measuring is exactly the structural-erosion question this card was meant to answer. Skipping leaves us blind.

## Integration points (AC6)

If adopt = yes:

| Tool | Integration point | Mechanism |
|---|---|---|
| `clippy::cognitive_complexity` | `platform/services/chorus-hooks/Cargo.toml` + `chorus-inject/Cargo.toml` | Add `[lints.clippy] cognitive_complexity = "warn"`. Pre-commit `cargo clippy` already runs; warnings surface. Promote to `deny` once existing 13 hits are addressed or `#[allow]`'d per-site. |
| `eslint-plugin-sonarjs` | `eslint.config.js` (root) | `npm install --save-dev eslint-plugin-sonarjs`, add plugin + rules block (cognitive-complexity, no-duplicate-string, no-identical-functions, no-collapsible-if). Pre-commit ESLint ratchet picks them up. Address the `no-empty-function` rule conflict by NOT importing the recommended preset. |
| `radon` | New pre-commit step OR scheduled drift signal | `radon cc -na -nc <changed-files>` returns non-zero on D+ functions. Add as a ratchet (warn on D, block on E) similar to existing pre-commit checks. Or surface as a nightly drift signal under #2527 nightly drift lane. |

The third option (radon as nightly drift instead of pre-commit) is the cleaner architectural fit — Python scripts in `platform/scripts/*.py` aren't the hot path; surfacing complexity as a card-on-red signal matches their criticality.

## Time-box note

90-min time-box, ~70 min used. Investigation IS the deliverable per `feedback_scope_is_the_work`. If implementation is approved per AC5, separate cards per tool.

## Open question (out of scope but flagged)

Does the structural-erosion signal correlate with which CARDS introduced complexity? Would need to cross-reference `git blame` output against card refs in commit messages. That's a different spike — "agent-coded structural decay measurable per card lineage." Not for tonight.

## Connects to

- #2584 (predecessor — closed with skip; named language-native as the right next-spike)
- #2528 (sensor consumer registry — radon as nightly drift would land here)
- #2561 / Move 4 (gate ceremony at agent velocity — structural-erosion sensor feeds in)
- Memory: `feedback_scope_is_the_work` — investigation IS the deliverable
