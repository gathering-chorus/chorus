# Quality Guide — Personal Site

Written by Kade for Jeff. Last updated: 2026-02-13.

This explains what keeps the system healthy, how to read the signals, and where the gaps are.

---

## The Big Picture

Think of quality safeguards as a series of gates between "code changed" and "code running in production." Each gate catches a different class of problem, and they're ordered from fastest/cheapest to slowest/most thorough.

```
Code Change
    |
    v
[TypeScript Compiler] --- catches: structural mistakes, type mismatches
    |
    v
[ESLint + Security Scan] --- catches: style violations, complexity, known vulnerability patterns
    |
    v
[Unit Tests (608 tests)] --- catches: individual functions not doing what they claim
    |
    v
[Integration Tests] --- catches: services not working together correctly
    |
    v
[Security Tests] --- catches: missing headers, rate limiting gaps, auth bypasses
    |
    v
[Performance Tests] --- catches: operations that got slow
    |
    v
[E2E Tests (Playwright)] --- catches: user-visible workflows that broke
    |
    v
Production
```

## What Each Layer Does

### 1. TypeScript Compiler (Strict Mode)

**What it is:** The type system checks that all the pieces fit together structurally — if a function expects a User object, you can't accidentally pass it a string.

**What it catches:** Mismatched data shapes, missing required fields, undefined values where they shouldn't be. These are the cheapest bugs to catch — they're caught before the code even runs.

**Strict mode is ON**, which means: no shortcuts, no implicit "any type is fine" — every piece of data has a declared shape.

**How to read it:** If the build (`tsc`) passes, the structural plumbing is sound.

### 2. ESLint (Code Quality)

**What it is:** A rule engine that enforces coding standards. Not about correctness — about consistency and maintainability.

**Key rules that matter to you:**
- **Complexity limit: 20** — No single function can have more than 20 decision paths. This prevents the kind of tangled logic that breeds bugs.
- **Function length limit: 80 lines** — Forces code to stay modular and testable.
- **Nesting depth limit: 4** — No deeply nested if/else chains.
- **Zero warnings policy** — We don't accumulate "known" warnings. Clean or fix.

**How to read it:** If lint passes with 0 warnings, the code follows the team's standards. If it fails, something needs to be simplified or restructured.

### 3. Security Scanning (Trivy + npm audit)

**What it is:** Automated tools that check our dependencies for known vulnerabilities and scan for secrets, misconfigurations.

**What it catches:** A library we depend on has a published vulnerability. A config file accidentally contains a secret. A container image has a known exploit.

**How to read it:** Blocks on HIGH or CRITICAL findings. If it passes, our supply chain is clean at the time of the scan.

### 4. Unit Tests (576 passing / 32 failing — see Issues below)

**What they are:** Each test proves one piece of the system does what it claims. "Given this input, this function returns that output." Fast to run (seconds), and they're the foundation of confidence.

**Coverage: 72.25% overall** (threshold is set at 80% — we're below it, see Issues).

**What the test names tell you:** A well-named test reads like a spec:
- "should render profile page when user is authenticated"
- "should return 403 when user lacks permission"
- "should store book data to pod in Turtle format"

When I do walkthroughs, the test names are the best starting point — they describe what the system *should do* in plain language.

**How to read it:** Green = the behaviors we've specified still work. Red = something changed that broke a promise.

### 5. Integration Tests

**What they are:** Tests that verify multiple services work together. The unit tests check each piece alone; integration tests check the assembly.

**Current scope:** Pod Service + Auth Service interaction — verifying that SOLID authentication flows correctly into pod storage operations.

**How to read it:** These confirm the data flow pipelines work end-to-end within the backend.

### 6. Security Tests

**What they are:** Tests that specifically verify security controls are in place.

**What they check:**
- HTTP security headers are set correctly (Content-Security-Policy, etc.)
- Rate limiting is enforced per IP/endpoint
- Cookies are secure (HttpOnly, SameSite flags)
- CORS headers are correct

**How to read it:** If these pass, the app is correctly hardened against common web attacks.

### 7. Performance Tests

**What they are:** Tests that verify operations complete within acceptable time bounds.

**Current scope:** Gallery service with large datasets (1000+ images), tag mapping performance.

**How to read it:** These catch regressions — if an operation that used to take 100ms now takes 2 seconds, we know immediately.

### 8. E2E Tests (Playwright)

**What they are:** A browser automation tool that simulates real user interactions. These are the slowest but highest-confidence tests — they prove the entire system works from a user's perspective.

**17 test specs covering:**
- Login and authentication flows
- CRUD operations (books, properties, ideas, collections)
- Role-based access control
- Privacy/visibility controls
- CSRF protection
- Dashboard navigation

**How to read it:** If E2E tests pass, a real user would be able to perform these workflows successfully.

---

## The Gates: When Do These Run?

### Pre-Commit Hook (before any code is saved to git)

This runs automatically and **blocks the commit if anything fails:**

1. Trivy security scan
2. npm audit (HIGH/CRITICAL)
3. ESLint (zero warnings)
4. TTL validation (RDF syntax check on Turtle files)
5. Unit tests

**This is the tightest gate.** Nothing gets committed without passing all five checks.

### CI/CD Pipeline (GitHub Actions, on push/PR to main)

Runs on the server after code is pushed:

1. Unit tests
2. Integration tests
3. Security tests
4. Performance tests
5. npm audit + CodeQL analysis
6. ESLint
7. Terraform validation
8. Build (TypeScript compilation)

**See Issues below** — the CI pipeline currently allows failures to pass through.

---

## How to Read Test Output

When I run tests, here's what the output means:

```
Test Suites:  40 passed, 8 failed, 48 total
Tests:        576 passed, 32 failed, 608 total
```

- **Test Suites** = groups of related tests (usually one per file)
- **Tests** = individual behaviors verified
- **Passed** = behavior still works as specified
- **Failed** = something broke or changed

Coverage summary:
```
Statements   : 72.25% (12028/16646)
Branches     : 85.19% (1583/1858)
Functions    : 70.54% (400/567)
Lines        : 72.25%
```

- **Statements** = percentage of code lines that tests exercise
- **Branches** = percentage of if/else paths tested (our strongest metric at 85%)
- **Functions** = percentage of functions called during tests
- **Threshold** = 80% statements/lines, 75% functions, 60% branches

---

## Current Issues (Honesty Section)

These are real problems I found during exploration. Per our agreement — no sweeping things under the rug.

### 1. CI Pipeline is Non-Blocking

**What:** Every test/lint/security step in GitHub Actions uses `|| echo "skipped"` or `continue-on-error`. This means builds succeed even when tests fail.

**Why it matters:** The pre-commit hook is solid locally, but if someone (or something) pushes code that skips the hook, CI won't catch it. The gate is open.

**Recommendation:** Make test failures block the build in CI. The pre-commit hook is the fast feedback loop; CI is the safety net. A safety net with holes isn't one.

### 2. Coverage Below Threshold

**What:** Overall coverage is 72.25%, but the configured threshold is 80%. Some critical areas are low:
- ACL handler: 20.62%
- Dashboard handler: 24.8%
- Collection handler: 19.25%

**Why it matters:** These are authorization and data management handlers — exactly the code where bugs hurt most.

**Recommendation:** Prioritize coverage in ACL, Dashboard, and Collection handlers. These aren't vanity numbers — these are the areas where a regression could mean data exposure or broken workflows.

### 3. 32 Tests Currently Failing

**What:** 8 test suites with 32 failing tests out of 608 total.

**Why it matters:** Failing tests erode trust in the test suite. If some tests are "always red," people stop looking at test results. That's how real failures hide.

**Recommendation:** Fix or remove every failing test. If a test is failing because the feature changed, update the test. If it's testing dead code (like the Ghost proxy tests — Ghost was replaced by WordPress), remove it. Zero tolerance for known-red tests.

### 4. Test Scripts Swallow Failures

**What:** Several npm scripts use `|| true` (e.g., `test:integration`, `test:security`, `test:performance`, `lint`). This means they always report success even when they fail.

**Why it matters:** Same trust erosion as above. If a script always says "success," it's not a check — it's theater.

**Recommendation:** Remove `|| true` from test and lint scripts. If a test category is genuinely optional, make that explicit in documentation, not hidden in a silent success override.

### 5. Open Test Handles

**What:** 36 open handles detected after test runs (likely unclosed server connections or timers).

**Why it matters:** Minor but indicates tests aren't fully cleaning up. Can cause flaky test runs and resource leaks in CI.

**Recommendation:** Add `--detectOpenHandles --forceExit` to Jest config temporarily to identify them, then fix the cleanup.

---

## Summary: What "Green" Actually Means

| Signal | Meaning |
|--------|---------|
| TypeScript builds | Structural plumbing is sound |
| ESLint passes (0 warnings) | Code follows standards, complexity is managed |
| Security scan passes | No known vulnerabilities in dependencies |
| Unit tests pass | Individual behaviors work as specified |
| Integration tests pass | Services work together correctly |
| Security tests pass | Security controls are in place |
| Performance tests pass | No regressions in speed |
| E2E tests pass | User-visible workflows work end-to-end |
| Coverage meets threshold | We've tested enough of the code to be confident |

**When ALL of these are green, we have high confidence the system works correctly, securely, and performantly.**

Right now, we're not there — and that's the first thing to fix.
