# Daily Quality Review — 2026-08-25

> **Path map:** `directing/clearing` → app; `platform/{workflow-engine,chorus-sdk,pulse}` → suites. `jeff-bridwell-personal-site` and `messages/*` do not exist in this repo.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). **Day 74.**
- 0 tests run. No change from yesterday.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. **Day 76.**
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**YELLOW** — 234 type errors. **No change from yesterday (234). Plateau continues.**
- Error trend: …→ 235 → 235 → **234** → **234**. Decrease yesterday did not continue.
- **Action:** Monitor. One-day dip may be noise.

## Board-Client
**N/A** — No `messages/board-client` in this repo. Maps to `platform/` suites.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 74.**
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 74.**
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. **Day 74.**
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-08-23)
- **BUILD:** Type errors held at **234** (no change). Yesterday's −1 did not continue.
- **UNCHANGED:** All 4 test suites blocked by `ts-jest preset not found` — now **day 74**.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now **day 76**.
- **Primary blocker:** `npm ci` at repo root. **76 days unresolved. Escalation overdue.**
