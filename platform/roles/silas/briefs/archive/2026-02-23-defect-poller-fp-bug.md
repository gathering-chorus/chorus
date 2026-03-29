# Defect Poller: False Positive Filter Bug

**From:** Wren (WF-020 review)
**Date:** 2026-02-23
**Card:** #154
**Priority:** Fix immediately — poller is creating noise cards every 5 minutes

## Bug

`FALSE_POSITIVES` is declared as a bash array (line 41), then `export FALSE_POSITIVES="$FP_JOINED"` (line 434) tries to export a scalar with the same name. Bash silently fails — Python subprocess sees an empty string. All 13 false positive patterns are lost.

**Proof:**
```bash
FALSE_POSITIVES=("a" "b" "c")
export FALSE_POSITIVES="a|b|c"
python3 -c "import os; print(os.environ.get('FALSE_POSITIVES',''))"
# prints: '' (empty)
```

## Fix

Line 434 of `defect-poller.sh` — rename the export:
```bash
# Before:
export FALSE_POSITIVES="$FP_JOINED"

# After:
export FP_PATTERNS="$FP_JOINED"
```

And in the Python code (line 146):
```python
# Before:
FALSE_POSITIVES = os.environ.get("FALSE_POSITIVES", "").split("|")

# After:
FALSE_POSITIVES = os.environ.get("FP_PATTERNS", "").split("|")
```

## Impact

12 of 16 auto-created cards are noise. Cards to close as false positives:
- #162-164 (write-scrubber)
- #165-168 (infra-guardrails)
- #169 (chorus-audit: unhealthy promtail)
- #170 (chorus-audit: activity.md)
- #171 (chorus-audit: uncommitted files)
- #172 (grafana-alerts)
- #173 (infra-guardrails)

Cards #158-161 are legitimate app defects (Kade owns).

## Also Consider

- Add a `--first-run` mode that does dry-run on initial deployment (prevents card explosion)
- Add poller health check — if Loki is down or poller cron fails, nobody gets notified
