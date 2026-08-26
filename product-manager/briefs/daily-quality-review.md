# Daily Quality Review — 2026-08-26

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (package-level node_modules incomplete). **Day 75.**
- Root-level jest discovers 439 suites repo-wide: **438 failed, 1 passed, 4 tests run.** (New signal — root jest now loading suites.)
- Package-level jest in `directing/clearing` still blocked: 0 tests run.
- **Action:** `npm ci` across all packages to restore node_modules.

## Lint
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. **Day 77.**
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root.

## Build (TypeScript)
**YELLOW** — 238 type errors (root tsconfig). **Up +4 from yesterday's 234.**
- Error trend: … → 235 → 235 → 234 → 234 → **238**. Small uptick; watch for continuation.
- **Action:** Monitor. If trend continues upward tomorrow, investigate new commits.

## Board-Client
**N/A** — No `messages/board-client` in this repo. Maps to `platform/` suites.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 75.**
- **Action:** `npm ci` in `platform/workflow-engine`.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 75.**
- **Action:** `npm ci` in `platform/chorus-sdk`.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 75.**
- **Action:** `npm ci` in `platform/pulse`.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-25)
- **BUILD:** Type errors +4 (234 → **238**). First increase after two-day plateau. Watch.
- **NEW SIGNAL:** Root-level jest now surfaces 439 suites repo-wide (438 failing). Previously reported as 0 run. Root ts-jest may be partially resolving.
- **UNCHANGED:** All 4 package-level test suites blocked by `ts-jest preset not found` — **day 75**.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — **day 77**.
- **Primary blocker:** `npm ci` at repo root + all sub-packages. **77 days unresolved. Escalation overdue.**
