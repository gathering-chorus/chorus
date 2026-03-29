## Lint Ceiling Ratchet (#1282)

**`--max-warnings` only moves down. Never up.** The pre-commit hook blocks any commit that raises the lint ceiling. If you hit the limit, fix the warnings — don't raise the number.

**Current ceiling:** 10 (in `jeff-bridwell-personal-site/package.json` → `"lint"` script).

**When you fix warnings:** Lower the ceiling to match. If you fix 2 warnings and the count drops to 8, change `--max-warnings=10` to `--max-warnings=8`. Ratchet down.

**The anti-pattern:** "I added 2 warnings, let me bump the ceiling to 12." No. The ceiling exists to hold the line. Raising it normalizes drift. Fix the warnings or don't ship the code.
