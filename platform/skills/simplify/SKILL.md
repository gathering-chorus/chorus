---
name: simplify
description: Review changed code for reuse, quality, and efficiency, then fix any issues found.
user-invocable: true
---

# /simplify — Code Quality Review

Review recently changed code for quality issues. Runs a Claude agent against the current diff and reports findings.

## Usage

```
/simplify [card-id]
```

If card-id given, reviews that card's AC against the code changes. Otherwise reviews the current git diff.

## Steps

1. Get the card AC (if card-id given):
   ```bash
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID}
   ```

2. Get the recent diff:
   ```bash
   cd /Users/jeffbridwell/CascadeProjects/chorus && git diff HEAD~3
   ```

3. Review for:
   - Code that could be simplified or deduplicated
   - Missing error handling at system boundaries
   - Unused imports or dead code introduced by the changes
   - Inconsistencies with surrounding patterns

4. Fix any issues found directly — don't just report them.

5. Run tests after fixing:
   ```bash
   cd /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks && cargo test
   ```
