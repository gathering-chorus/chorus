---
name: gate-code
description: Code gate — verify tests green, build clean, no new warnings. Kade only.
user-invocable: true
---

# /gate-code — Code Gate

Fires at code-complete. Verifies the code is solid. **Kade only.**

## Arguments

```
/gate-code <card-id>
```

## Owner Check

Only Kade can run this gate. If another role invokes it:
```
Code gate is owned by Kade.
```
Exit — no checks run.

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- Doc-only / board-process cards: **SKIP** — "Code gate not applicable for this card type."
- All cards with code changes: **RUN**.

If skipped, emit: `gate.code.skipped` spine event. Exit.

## Automated Checks (run all, collect results)

### 1. Tests green

```bash
# Run the test suite for the affected codebase
# Detect which codebase from changed files
cd /Users/jeffbridwell/CascadeProjects/chorus
CHANGED=$(git diff HEAD~3 --name-only)

# Rust tests (chorus-hooks)
if echo "$CHANGED" | grep -q 'chorus-hooks'; then
  cd platform/services/chorus-hooks && cargo test 2>&1
fi

# TypeScript tests (cards, app)
if echo "$CHANGED" | grep -q 'cards/\|services/'; then
  cd directing/products/cards && npm test 2>&1
fi
```

**Pass:** All test suites exit 0.
**Fail:** Any test failure — list the failing tests.

### 2. Build clean

```bash
# Rust build
if echo "$CHANGED" | grep -q 'chorus-hooks'; then
  cd /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks
  cargo build --release 2>&1 | tail -5
fi

# TypeScript build
if echo "$CHANGED" | grep -q '\.ts$'; then
  cd /Users/jeffbridwell/CascadeProjects/chorus/directing/products/cards
  npx tsc --noEmit 2>&1
fi
```

**Pass:** Build exits 0.
**Fail:** Compilation errors.

### 3. Warning count diff

```bash
# Compare warning count before and after
# Rust: count warning lines in build output
cd /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks
WARNINGS_NOW=$(cargo build --release 2>&1 | grep -c '^warning:')
# Compare against baseline (last known count from prior build)
WARNINGS_PREV=$(cat /tmp/gate-warning-baseline 2>/dev/null || echo "0")
if [ "$WARNINGS_NOW" -gt "$WARNINGS_PREV" ]; then
  echo "FAIL: warnings increased from $WARNINGS_PREV to $WARNINGS_NOW"
  exit 1
else
  echo "$WARNINGS_NOW" > /tmp/gate-warning-baseline
  echo "PASS: warnings $WARNINGS_NOW (was $WARNINGS_PREV)"
fi
```

**Pass:** Warning count did not increase.
**Fail:** New warnings introduced — list them.

### 4. Pattern match (read before write)

```bash
# Check that changed files follow existing patterns
# Crude: verify no new files with names that don't match existing conventions
cd /Users/jeffbridwell/CascadeProjects/chorus
git diff HEAD~3 --diff-filter=A --name-only | head -10
# New files listed for manual review if any
```

**Pass:** No new files, or new files follow naming conventions.
**Fail:** New files with unexpected naming — warn.

## No Manual Confirms

All code checks are automated.

## Result

Print summary:

```
## /gate-code #<card-id>

  Tests green:       PASS | FAIL (N failing)
  Build clean:       PASS | FAIL (errors)
  Warning diff:      PASS | FAIL (N new warnings)
  Pattern match:     PASS | WARN (new files listed)

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.code.passed` with card ID
2. Add card comment: "gate:code-pass — Kade"
3. Prompt: "Run /gate-quality next."

## On Fail

1. Emit spine event: `gate.code.failed` with card ID and failing items
2. Print failing items. Fix before re-running.
