---
name: gate-quality
description: Quality gate — verify hooks pass, no regression, no console.log in production code. Kade only.
user-invocable: true
---

# /gate-quality — Quality Gate

Fires at code-complete, after /gate-code passes. Verifies quality standards. **Kade only.**

## Arguments

```
/gate-quality <card-id>
```

## Owner Check

Only Kade can run this gate. If another role invokes it:
```
Quality gate is owned by Kade.
```
Exit — no checks run.

## Prerequisite

/gate-code must have passed for this card. Check for `gate.code.passed` spine event or `gate:code-pass` card comment. If not found:
```
WARN: No gate:code-pass found for this card. Run /gate-code <card-id> first.
```
In pilot mode: warn but continue. In enforce mode (future): block.

## Applicability Check

Same as /gate-code — doc-only and board-process cards skip.

## Automated Checks (run all, collect results)

Resolve the diff base before running checks:
```bash
cd /Users/jeffbridwell/CascadeProjects/chorus
# Use card's WIP-start commit if available (from spine event), fall back to HEAD~3
WIP_BASE=$(grep "card=$CARD_ID" /Users/jeffbridwell/.chorus/logs/spine.log 2>/dev/null | grep 'card.wip.started\|role.state.building' | tail -1 | sed 's/.*commit=\([a-f0-9]*\).*/\1/' 2>/dev/null)
DIFF_BASE="${WIP_BASE:-HEAD~3}"
```

### 1. Hooks pass

```bash
# Verify chorus-hooks build is current (tests already ran in /gate-code)
cd /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks
# Check the hook shim exists and is recent
[ -f target/release/chorus-hook-shim ] && echo "PASS" || echo "FAIL: hook shim not built"
```

**Pass:** Hook shim binary exists and is current.
**Fail:** Hook shim missing — build broke or wasn't run.

Note: Full test suite already ran in /gate-code. This check verifies the hooks are deployable, not re-running tests.

### 2. Regression check

```bash
# Verify no test files were deleted or skipped in this change
cd /Users/jeffbridwell/CascadeProjects/chorus
DELETED_TESTS=$(git diff "$DIFF_BASE" --diff-filter=D --name-only | grep -E 'test|spec|\.bats')
if [ -n "$DELETED_TESTS" ]; then
  echo "WARN: test files deleted: $DELETED_TESTS"
fi
```

**Pass:** No test files deleted.
**Warn:** Test files removed — verify intentional.

### 3. Console.log check

```bash
# Check changed production files for console.log (should use logger)
cd /Users/jeffbridwell/CascadeProjects/chorus
PROD_FILES=$(git diff "$DIFF_BASE" --name-only -- '*.ts' '*.rs' | grep -v 'test\|spec\|\.test\.')
if [ -n "$PROD_FILES" ]; then
  grep -n 'console\.log' $PROD_FILES 2>/dev/null
  if [ $? -eq 0 ]; then
    echo "FAIL: console.log found in production code — use structured logger"
    exit 1
  fi
fi
echo "PASS: no console.log in production code"
```

**Pass:** No `console.log` in changed production files.
**Fail:** List files and line numbers with `console.log`. Must use structured logger.

### 4. Observability present

```bash
# Check changed handlers/routes for structured log calls
cd /Users/jeffbridwell/CascadeProjects/chorus
HANDLER_FILES=$(git diff "$DIFF_BASE" --name-only -- '*.ts' | grep -E 'handler|route|service' | grep -v test)
if [ -n "$HANDLER_FILES" ]; then
  # Should have at least one logger call per handler file
  for f in $HANDLER_FILES; do
    if ! grep -q 'logger\.\|console\.error\|chorus-log' "$f" 2>/dev/null; then
      echo "WARN: $f has no logging — consider adding structured logs"
    fi
  done
fi
```

**Pass:** Changed handlers have logging.
**Warn:** Handler files without logging — advisory, not blocking.

### 5. Smoke check

Per ADR-026 §b open-decision (A): gate-quality runs smoke-check on the
changed surface. Skip if Gathering app isn't running locally (smoke-check
needs `localhost:3000`); fail-the-gate if the app is up and smoke fails.

```bash
APP_HEALTH=$(curl -sf --max-time 2 -o /dev/null -w '%{http_code}' http://localhost:3000/health 2>/dev/null)
if [ "$APP_HEALTH" = "200" ]; then
  if bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/smoke-check.sh --all 2>&1; then
    echo "PASS: smoke-check all pages green"
  else
    echo "FAIL: smoke-check found broken pages — see output"
    exit 1
  fi
else
  echo "WARN: Gathering app not reachable on :3000 — smoke-check skipped"
fi
```

**Pass:** All known pages return expected status + content.
**Fail:** One or more pages broken — gate fails.
**Warn:** App down locally — surface but don't block.

## Manual Confirm (1 item)

Only shown if all automated checks pass.

**"Any new technical debt introduced?"** — Kade reviews the diff and answers yes/no. If yes, card the debt.

## Result

Print summary:

```
## /gate-quality #<card-id>

  Hooks pass:          PASS | FAIL
  Regression suite:    PASS | FAIL
  Console.log check:   PASS | FAIL (files listed)
  Observability:       PASS | WARN (files listed)
  Smoke check:         PASS | FAIL | SKIP (app down)
  New debt:            PASS | FAIL (carded)

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.quality.passed` with card ID
2. Add card comment: "gate:quality-pass — Kade"
3. Nudge Silas: "gate:quality passed on #<card-id> — run /gate-arch"

## On Fail

1. Emit spine event: `gate.quality.failed` with card ID and failing items
2. Print failing items. Fix before re-running.
