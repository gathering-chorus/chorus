# Brief: CI Pipeline Enforcement — Build Scope

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Priority**: High — next after pod backup
**Context**: Priority stack item #4. ADR-003 shipped with 1,613 unit tests + 73 E2E tests. The pipeline should enforce what you built.

## Why This Matters

The test suite is now security-critical. `collectionVisibilityMiddleware` is a security boundary verified by tests. If CI lets failures pass silently, the security guarantee is only as strong as the last developer who remembered to run tests locally.

Jeff flagged this earlier: "A test that doesn't block is a suggestion."

## What to Fix

### 1. Remove permissive test execution

Audit the CI/CD config for any `|| echo`, `|| true`, `--passWithNoTests`, or `continue-on-error: true` patterns that let test failures pass. Test failures should fail the build.

### 2. Enforce coverage thresholds

With ACL service at 100% and the middleware tested, set a coverage floor. Suggested thresholds:
- **Global minimum**: 70% (prevents regression from current levels)
- **Security-critical files**: 90% minimum for `acl.service.ts`, `collectionVisibilityMiddleware`, `pod-storage.middleware.ts`

These are suggestions — use your judgment based on where the codebase actually sits. The point is: coverage can't silently drop.

### 3. Verify E2E tests run in CI

The 73 Playwright tests should run as part of the pipeline, not just locally. If they're not wired in yet, add them. If they need a running Fuseki + app to execute, document the CI environment requirements.

### 4. Fitness test integration (stretch)

The fitness test template (`../architect/fitness-test-template.md`) defines SPARQL queries that verify data quality. If CI has access to a Fuseki instance, running the pipeline health checks (Layer 1) after sync would catch ingestion regressions. Not a blocker — but a natural extension.

## What to Skip

- Don't restructure the entire CI/CD pipeline. Fix the permissive patterns and add enforcement. Keep it simple.
- Don't add deployment gates or staging environments. That's future work.
- Don't add linting enforcement if it's not already there — one thing at a time.

## Definition of Done

- [ ] Test failures block the CI build (no silent pass-through)
- [ ] Coverage thresholds enforced (build fails if coverage drops below floor)
- [ ] E2E tests run in CI
- [ ] Document any CI environment requirements for Playwright tests

## After This

Next on the priority stack is **Visualization Tooling** (ADR-004) — embedding YASGUI in the dashboard to replace the SPARQL textarea. That's the first feature-facing work after the foundation is solid. I'll brief you separately when you're ready for it.

— Silas
