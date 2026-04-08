---
name: gate-arch
description: Architecture gate — verify system fit, namespace conventions, ICD consistency, domain boundaries. Silas only.
user-invocable: true
---

# /gate-arch — Architecture Gate

Fires at code-complete. Verifies the change fits the system. **Silas only.**

## Arguments

```
/gate-arch <card-id>
```

## Owner Check

Only Silas can run this gate. If another role invokes it:
```
Architecture gate is owned by Silas.
```
Exit — no checks run.

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- `type:fix` (bug fix): **SKIP** — "Arch gate not applicable for bug fix cards." Exit.
- Doc-only / board-process cards: **SKIP**.
- Feature, script/hook, infra cards: **RUN**.

If skipped, emit: `gate.arch.skipped` spine event. Exit.

## Automated Checks (run all, collect results)

### 1. Namespace conventions

```bash
# Check recently changed files for non-canonical URIs
cd /Users/jeffbridwell/CascadeProjects/chorus
CHANGED=$(git diff HEAD~3 --name-only -- '*.ttl' '*.owl' '*.sparql' '*.ts' | head -20)
if [ -n "$CHANGED" ]; then
  # Look for old/non-canonical namespace patterns
  grep -l 'urn:borg:\|http://jeff-bridwell\|xmlns:old' $CHANGED 2>/dev/null
  # Exit 0 = no violations found (pass), exit 1+ = violations (fail)
fi
```

**Pass:** No non-canonical URIs in changed files.
**Fail:** List the files and the offending patterns.

### 2. ICD consistency

```bash
# Query Fuseki for ICD validation status on domains touched by this card
DOMAIN=$(cards view <card-id> | grep -oE 'domain:\w+' | head -1 | sed 's/domain://')
if [ -n "$DOMAIN" ]; then
  curl -s "http://localhost:3340/api/chorus/domain/${DOMAIN}" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(d.get('domain', 'unknown'), ':', d.get('description', ''))
" 2>/dev/null
fi
```

**Pass:** Domain service returns data (domain exists, is mapped).
**Fail:** Domain not found or domain service unreachable.

### 3. Domain boundary scan

```bash
# Check if changed files cross domain boundaries
cd /Users/jeffbridwell/CascadeProjects/chorus
git diff HEAD~3 --name-only | sort > /tmp/gate-arch-files.txt
# Count distinct top-level directories (crude boundary check)
cat /tmp/gate-arch-files.txt | cut -d/ -f1-3 | sort -u | wc -l
```

**Pass:** Changes contained to 1-2 directory subtrees.
**Fail:** Changes span 4+ directory subtrees — warn "broad blast radius, review cross-domain impact."

## Manual Confirm (1 item max)

Only shown if all automated checks pass.

**"Does this change fit the system architecture?"** — Silas reviews the diff, considers domain map alignment, and answers yes/no. If no, provide a reason.

## Result

Print summary:

```
## /gate-arch #<card-id>

  Namespace conventions:  PASS | FAIL (details)
  ICD consistency:        PASS | FAIL | N/A (no domain)
  Domain boundary:        PASS | WARN (details)
  Structural fit:         PASS | FAIL (reason)

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.arch.passed` with card ID
2. Add card comment: "gate:arch-pass — Silas"
3. Nudge self: "arch passed — run /gate-ops after deploy"

## On Fail

1. Emit spine event: `gate.arch.failed` with card ID and failing items
2. Print failing items. Do not nudge forward.
