# Quality — Service Design

**Kade, 2026-04-18. Draft. Source: `/borg/quality` page (test pyramid reporting surface), `quality-guide.md` (Gathering-scoped pipeline), tarpaulin + jest configs, pre-commit hook, existing gates (gate-code, gate-quality, demo-preflight, TDD gate), loom-principles (`quality-at-source`, `speed-and-quality-correlate`), DEC-1674 (TDD discipline).**

## Promise

Every change ships with confidence proportional to its blast radius. When Quality is healthy, Jeff can ask "does this work?" and the answer is trustworthy: failing tests mean user-visible behavior broke, passing tests mean what Jeff cares about is still intact, coverage numbers reflect protection not ritual. When Quality drifts, tests accumulate without catching bugs, coverage targets get gamed, gates pass work that a thoughtful reviewer would reject, and Jeff becomes the test suite — finding regressions by touching the product and re-teaching what "green" should have meant.

## Overview

Quality is the horizontal discipline that defines what "works" means for Chorus and enforces it at the right moments. It composes four concerns into one service: the test pyramid (what kinds of tests exist, in what proportion, per domain), coverage measurement (what the tests actually exercise), gates (when quality bites), and test-value policy (what makes a test worth committing). Quality does not own individual tests — service owners do — but it owns the shape those tests collectively form.

The pyramid is the load-bearing mental model. Top layer = slowest/broadest, proves Jeff-visible behavior under real conditions (E2E, security, performance). Middle = integration, proves services compose correctly across process boundaries. Bottom = unit, proves individual functions do what they claim — fast, broad, cheap. A healthy system has a real pyramid (broad base, narrowing top). A sick system has a slab (all unit tests, no E2E) or an inverted pyramid (all E2E, no unit tests) — both hide risk.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| Test pyramid taxonomy | REAL | `/borg/quality` page + API at `/api/chorus/quality/summary` crawls files into layer buckets | No written spec of what belongs in each layer; file-name inference, not declarative |
| Coverage measurement (Rust) | REAL | tarpaulin per-crate, configs with per-file + global floors | ops.rs seam work proved feasible (77.69%); some crates still baseline-as-floor (follow-on) |
| Coverage measurement (TS) | PARTIAL | jest per-package, per-file thresholds | Global platform/api threshold at 0%; HTTP-over-wire tests don't instrument server.ts; gameable via mock-farming |
| Pre-commit gate | REAL | `.git/hooks/pre-commit` runs tsc + jest (no coverage) + secret scan | Doesn't run coverage check; no per-layer enforcement |
| Nightly fitness | PARTIAL | `run-all-tests.sh --coverage` aggregates per-service; emits `coverage.measured` spine events | No bug-escape-rate; no pyramid-shape check; aggregate hides per-layer skew |
| Gate-code / gate-quality / demo-preflight | REAL | skills with ACs enforced at acceptance time | Coverage-farm-friendly; no test-value check |
| Stop-the-line | REAL | `stop_on_error.rs` hook blocks on cargo/jest errors mid-build | Reactive; doesn't catch weak tests, only broken ones |
| Test-value policy | **MISSING** | — | No written definition of what makes a test valuable; this gap is the reason #2167 shipped 170 unit tests that added coverage without adding confidence |
| Per-domain per-layer floors | **MISSING** | — | Single global coverage % is a blunt metric; pyramid shape not enforced |
| Bug-escape measurement | **MISSING** | — | No feedback loop from production bugs back to test-suite weakness |

## Sub-Domain Interaction Model

Quality doesn't act directly — it's read by gates and written by test runs. The service is the measurement + model; enforcement is other sub-products consuming it.

| Trigger | Produces | Consumed By | Surface |
|---------|----------|-------------|---------|
| Test run completes | Coverage report per service + pass/fail counts | Borg quality page, nightly fitness, gate-code, gate-quality | `coverage/lcov*`, `coverage-summary.json`, tarpaulin stdout |
| Test run completes | `coverage.measured` spine event (kind, service, coverage %) | Chorus search, fitness dashboards | `chorus.log` |
| File touched matches test glob | Test file classified into pyramid layer | Borg quality page | `/api/chorus/quality/summary` `pyramid[]` |
| Pre-commit staged | `tsc` + `jest --forceExit` per package | Pre-commit hook decision | `.git/hooks/pre-commit` exit code |
| Card reaches gate-code | Test ratio + coverage + lint state | Wren (product) / Silas (arch) gate decision | Skill invocation |
| Card reaches demo-preflight | Integration + E2E green check | Demo go/no-go | `demo-preflight.sh` |
| Service fails in production | (should produce) bug-escape event | (should feed) test-suite weakness report | **NOT WIRED** |

The core pattern: Quality is a **model + measurements**, not an enforcer. Gates and hooks consume Quality state and bite. Consistent with Pulse ("assembler not generator") and Roles ("source not enforcer") — the horizontal services define what's true; other sub-products act on it.

## Dependencies

Per Wren's 4-layer pattern (#2159): Quality sits on principles + practices + policies + decisions.

| Dependency | Sub-domain | Status | Instances | Notes |
|-----------|------------|--------|-----------|-------|
| Principles | loom-principles | POPULATED | 2 quality-relevant | `quality-at-source`, `speed-and-quality-correlate`. `comprehension-is-the-rate-limit` also applies — tests that don't describe Jeff-visible behavior erode comprehension |
| Practices | loom-practices | POPULATED | 3 quality-relevant | `tdd` (test-first), `actor-bdd` (tests describe actor intent not implementation), `production-ready` (tests enforce shippability). `api-first` and `domain-first` constrain where tests live |
| Policies | loom-policies | MISSING | — | Sub-domain blocked on #2151 stand-up. Quality will depend on: coverage floors per domain, pyramid-shape constraints, test-value policy, bug-escape budget, gate bite-points |
| Decisions | loom-decisions | SHELL | 0 populated | DEC-1674 (TDD discipline — AC → tests → code → green → demo) and DEC-095 (ICD gate — no harvester code without provider section) are Quality-load-bearing but un-harvested; #2152 covers harvest |

**Why dependencies matter to Quality:** the pyramid is a shape derived from principles (quality-at-source says the test lives with the code it proves), practices (TDD says tests arrive first), policies (when they land: coverage floors, test-value rules), and decisions (why specific gates bite where they do). When any layer is missing, Quality's posture drifts. Observed 2026-04-17/18: without a written test-value policy, Kade shipped 170 unit tests on #2167 that added coverage without adding confidence. No policy, no gate, no catch.

## Components

### Test pyramid taxonomy
Five conceptual layers, top to bottom: E2E (user-visible behavior, full system), security (controls verified under attack), performance (operations under load), integration (services composed across process boundaries), unit (pure logic, fast). The `/api/chorus/quality/summary` endpoint crawls test files by filename convention into buckets and renders a width-proportional view at `/borg/quality`. No declarative layer assignment — inference by filename pattern is the current mechanism. Per-domain pyramid visibility exists; per-domain pyramid *floors* do not.

### Coverage measurement
Two runners: tarpaulin (Rust crates) and jest (TypeScript packages). Each config has a global floor (`fail-under` / `coverageThreshold.global`) and optional per-file thresholds. tarpaulin runs `--test-threads=1` for crates with shared state (role-state files, hook env vars). `run-all-tests.sh --coverage` is the aggregation driver; it emits `coverage.measured` spine events per service with the percentage. Coverage is a proxy for protection, not protection itself — and coverage numbers can diverge from protection when tests game the metric (pure-function redundancy, mock-wiring assertions, implementation-detail tests).

### Gates
Five gate surfaces, each biting at a different point in the change lifecycle:
- **Pre-commit hook** (`.git/hooks/pre-commit`): runs tsc + jest + secret scan + werk auto-bump. Does not currently run coverage threshold check. Immediate; blocks commit.
- **TDD gate** (`tdd_gate.rs` in chorus-hooks): blocks edits to production code if no test was written first in the session. Enforces DEC-1674. Per-session state.
- **gate-code**: owner-invoked at card acceptance; checks lint, build, test pass/fail, no test deletions, no console.log in request-path. Kade-owned skill.
- **gate-quality**: owner-invoked at card acceptance; checks structural quality (no new hook shim rebuilds without permission, no removed gates, no silent bypasses).
- **demo-preflight**: pre-demo check; blocks if integration tests fail.

### Test-value policy (currently absent)
The missing core piece. A test is valuable when its failure would cause Jeff to change his behavior — fix code, revert a deploy, escalate. A test is non-valuable when its failure means only "implementation changed." Concrete signals: tests whose names don't describe observable behavior; tests that assert on mocks rather than real dependencies; tests that survive rewriting the implementation with a different algorithm; tests added purely to raise a coverage number. The policy belongs in loom-policies once #2151 lands; until then, this gap is the single biggest reason Quality drifts.

### Fitness functions
Non-test measurements: lint warning count (ratcheted-down only via `--max-warnings`), test count (monotonic modulo deletions), bug-escape rate (not yet measured), test duration (not tracked), flake rate (not tracked). Fitness functions are where Quality should surface "is the shape right?" distinct from "do the tests pass?"

### Stop-the-line (reactive)
`stop_on_error.rs` hook intercepts cargo / npm / jest errors mid-build and surfaces them to the role immediately. Reactive — depends on failures surfacing. Does not catch weak tests or silent regressions.

### Measurement surfaces
- `/api/chorus/quality/summary` — crawls repos for test files, returns layered counts per repo + totals + pyramid.
- `/borg/quality` — HTML rendering of the above.
- Per-service coverage reports (`coverage/` dirs, tarpaulin output).
- `coverage.measured` spine events.

## Surfaces

Quality exposes one API and one UI of its own; most of its surface is contributed into other services' configs.

- **API**: `/api/chorus/quality/summary` (served by platform/api) → JSON of `{total, pyramid[], repos[]}` for the Borg dashboard.
- **UI**: `/borg/quality` (platform/api/public/borg/quality/index.html) → pyramid + per-repo visualization.
- **Configs contributed**: tarpaulin.toml per Rust crate, jest.config.js per TS package, pre-commit hook checks.
- **Spine events**: `coverage.measured`, `test.run.completed`, (future) `bug.escaped`, (future) `pyramid.shape.drifted`.
- **Skills**: gate-code/SKILL.md, gate-quality/SKILL.md, tdd-gate enforcement (hook-side).

## Consumers

| Consumer | Uses | Status |
|----------|------|--------|
| Pre-commit hook | `tsc` + `jest` exit codes | WIRED (no coverage check yet) |
| CI pipeline / nightly | `run-all-tests.sh --coverage` aggregation | WIRED |
| gate-code | coverage + lint + test counts at acceptance | WIRED (coverage-farm-friendly) |
| gate-quality | structural quality at acceptance | WIRED |
| demo-preflight | integration + E2E green check | WIRED |
| Borg quality dashboard | `/api/chorus/quality/summary` | WIRED |
| Fitness dashboards (Pulse, fitness page) | `coverage.measured` spine events | PARTIAL — pulse reads some; no pyramid-shape surfacing |
| Stop-the-line (`stop_on_error.rs`) | mid-build cargo/jest errors | WIRED |
| Bug-escape feedback loop | — | **NOT WIRED** |
| Test-value gate | — | **NOT WIRED (policy undefined)** |

## Gaps

1. **No written test-value policy.** The largest gap. Without a definition of what makes a test worth keeping, gates can't reject weak tests, coverage targets incentivize the wrong work, and horizontal-quality reviews (this one) are the only protection. Observed cost: #2167 shipped 170 unit tests over a day, most of which test implementation details or string matches in argv. Coverage went up 20 points; chorus-wide protection did not.
2. **Coverage metric not aligned with pyramid design.** "80% coverage" is a scalar; the pyramid is a shape. Two systems with identical coverage numbers can have opposite test health — one with integration-heavy protection, one with unit-test-farming. Current gates read the scalar; the design requires the shape.
3. **Per-domain per-layer floors undefined.** Every domain gets the same global threshold, but a coordination domain (chorus-hooks) has different pyramid needs than a CRUD domain (platform/api handlers). No written spec maps domains to layer ratios.
4. **Platform/api global threshold at 0%.** HTTP-over-wire integration tests (serving :3340) don't instrument server.ts — jest sees separate-process code as uncovered. The structural fix (`require.main` guard + in-process test harness) is scoped as a separate card but the hole it leaves in Quality is: 84% of platform/api LOC is unmeasured. Chorus-wide coverage is ~40% today largely because of this single gap.
5. **HTTP-test-to-in-process conversion is coverage-farmable.** When the `require.main` refactor lands, the obvious next move is to mock downstream dependencies (Fuseki, Loki, Vikunja) so handlers return canned responses. Coverage jumps, real-behavior confidence does not. A test-value gate must be in place before that conversion, not after.
6. **No bug-escape measurement.** No feedback from production incidents back to the test suite. A regression that Jeff catches by using the product should produce a spine event that's triaged into either (a) a new test of the specific behavior, or (b) a pyramid-shape signal (e.g., "we're missing a whole E2E layer here").
7. **Pyramid inference by filename is brittle.** `/api/chorus/quality/summary` classifies test files by name patterns. A test in `tests/foo.test.ts` could be a unit test or an HTTP integration test; the shape report can't tell. Declarative layer assignment (a frontmatter tag, a config block, or per-test metadata) would make the pyramid trustworthy.
8. **Pre-commit hook doesn't run coverage.** The hook runs `tsc` + `jest --forceExit` (no `--coverage`). So jest's `coverageThreshold` — even set correctly — is never enforced at commit time. It only enforces when someone explicitly runs `npm test -- --coverage`. That's a gate that looks real but doesn't bite.
9. **No role-level quality review cadence.** Jeff currently reviews quality posture by asking for numbers and catching the drift himself. The horizontal quality role (Kade) should be producing a weekly or per-session quality report that names drift before Jeff does. Observed 2026-04-17/18: Kade reported per-file wins and hid the chorus-wide number until Jeff asked. That's a process gap, not just an individual lapse.

## Next Steps

| # | Action | Impact | Owner | Status |
|---|--------|--------|-------|--------|
| 1 | Write test-value policy — publish in loom-policies once #2151 lands, draft in this doc until then | Makes "is this test worth keeping" a reviewable gate, not a taste call | Kade (draft) + Wren (policy home) | Unfiled |
| 2 | Define per-domain per-layer floors — map each Chorus sub-product to target unit/integration/E2E ratios | Turns "80% coverage" into "pyramid-shaped 80%" | Kade | Unfiled |
| 3 | Wire `coverage.measured` + new `pyramid.shape` events into Borg quality page | Pyramid drift becomes visible as a trend, not a snapshot | Kade + Silas | Unfiled |
| 4 | Add coverage-threshold check to pre-commit hook — make the configured threshold actually bite at commit time | Closes the "gate that looks real but doesn't bite" hole | Silas (hook) + Kade (threshold values) | Unfiled |
| 5 | Add bug-escape spine event + feedback surface — when Jeff catches a bug, write it as an event that feeds a test-gap report | Closes the loop between production reality and test-suite weakness | Kade + Silas | Unfiled |
| 6 | Extract handlers from platform/api/src/server.ts into per-handler modules so unit tests at the bottom of the pyramid are feasible without mock-farming | Makes the pyramid shape achievable for 84% of Chorus LOC | **NOT KADE ALONE** — architecture conversation with Silas first | Unfiled |
| 7 | Add Quality as a Chorus sub-product entry in Athena — parallel to Pulse, Roles, Observer | Makes the service queryable, owned, and accountable | Wren | Unfiled |
| 8 | Express `chorus:dependsOn` edges from Quality to loom-{principles,practices,policies,decisions} | 4-layer dependency becomes graph-queryable per Wren's pattern | Wren | Unfiled |
| 9 | Declarative layer assignment for tests — per-test metadata or config block replaces filename inference | Pyramid becomes trustworthy, not a heuristic | Kade | Unfiled |
| 10 | Weekly quality posture report — Kade-authored, published to `roles/kade/quality-review.md`, scans chorus-wide shape + coverage + drift and names the top 3 things to fix | Kade names drift before Jeff does; closes the horizontal quality accountability gap | Kade | Unfiled |

## Not in scope

- **Enforcement mechanisms themselves.** Quality defines the model and measures it; gates (gate-code, gate-quality, pre-commit, demo-preflight) are separate sub-products that consume Quality state and bite. If a gate needs to change, that's a card against the gate, not against Quality.
- **Individual test authoring.** Service owners (Kade for presentation, Silas for infra, Wren for coordination) write tests for their domains. Quality defines the shape those tests collectively should form.
- **Security scanning depth.** When security becomes a first-class sub-product (Trivy / npm audit / CodeQL integration), that's its own service design. Quality surfaces security-test presence in the pyramid; it doesn't own the scanning.
- **Performance SLOs.** Performance tests live in the pyramid; SLO definitions (what latency is acceptable at which scale) are a separate product/ops conversation.
- **Test runtime optimization.** Slow tests are a pain point but not a quality issue; they're a developer-experience issue that lives in test tooling work.
- **Coverage tooling choice.** Tarpaulin vs nyc vs c8 vs jest built-in — tool choices are implementation details under this design, not part of it.
