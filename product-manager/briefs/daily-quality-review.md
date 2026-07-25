# Daily Quality Review — 2026-07-25

> **Path map:** `jeff-bridwell-personal-site/` → `directing/clearing`; `messages/{workflow-engine,chorus-sdk,slack-bridge}` → `platform/{workflow-engine,chorus-sdk,pulse}`. `board-client` has no repo equivalent.

## App Tests (`directing/clearing`)
**RED** — BLOCKED: `ts-jest` preset not found (node_modules incomplete). Day 44.
- 0 tests run. No change from 2026-07-24.
- **Action:** `npm ci` needed to restore node_modules across all packages.

## Lint (`directing/clearing`)
**RED** — BLOCKED: `@eslint/js` not found in root node_modules. Day 46.
- Persistent since 2026-06-09. No change.
- **Action:** `npm ci` at repo root (same root cause as tests).

## Build (`directing/clearing` TypeScript)
**RED** — 174 type errors. **UP +17 from yesterday (was 157). NEW REGRESSION.**
- New implicit-`any` params across `server.ts` lines 264, 440, 455, 520, 530, 544, 553; `__dirname` not found at lines 442, 452. Root cause: wren:#3679 (account/change-password routes, merged 2026-07-25) and silas:#3669 (CSS-OIDC edge, WS session auth) added route handlers and new files without explicit Express types.
- **Action:** wren + silas should add `import { Request, Response, NextFunction } from 'express'` types to new route params in `server.ts` and new files (`account.ts`, `solid-auth.ts`, `solid-oidc.ts`, `connection-auth.ts`).

## Board-Client
**N/A** — No equivalent package in this repo.

## Workflow-Engine (`platform/workflow-engine`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 44.
- **Action:** `npm ci` to restore node_modules.

## Chorus-SDK (`platform/chorus-sdk`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 44.
- **Action:** `npm ci` to restore node_modules.

## Slack-Bridge → Pulse (`platform/pulse`)
**RED** — BLOCKED: `ts-jest` preset not found. Day 44.
- **Action:** `npm ci` to restore node_modules.

## Coverage
**N/A** — All suites blocked; no data. Last known: clearing YELLOW, workflow-engine GREEN, chorus-sdk RED (funcs 62%), pulse GREEN (2026-06-09).

## Failure Delta (vs 2026-07-24)
- **NEW REGRESSION:** Build type errors 157 → 174 (+17). Driven by wren:#3679 and silas:#3669 — new route handlers in `server.ts` and new files (`account.ts`, `solid-auth.ts`, `solid-oidc.ts`, `connection-auth.ts`) use implicit `any` params. Largest single-day jump in 2 weeks.
- **UNCHANGED:** All 4 suites blocked by `ts-jest preset not found` — now day 44.
- **UNCHANGED:** Lint blocked (`@eslint/js`) — now day 46.
- **Primary blocker remains:** `npm ci` at repo root. Now **46 days unresolved.**
