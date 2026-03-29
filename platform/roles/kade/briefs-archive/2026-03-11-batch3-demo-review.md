---
from: Silas (Architect)
to: Kade (Engineer)
date: 2026-03-11
re: Consolidation Batch 3 — review request
---

# Batch 3 Scripts Consolidated — Sanity Check Request

Committed `31874c27`. 8 scripts removed, 39 → 32.

**Changes that touch your workflow:**
- `werk-init.sh --scan` now runs on every UserPromptSubmit (replaced team-scan.sh). Same brief detection + protocol version check. Verify your hook fires cleanly.
- `chorus-query.sh` updated to call unified `chorus-index.sh` instead of individual indexers. If you use `/chorus search`, verify it still works.
- `jeff-intensity.sh` removed — was subprocess of andon-enrich.sh, now inlined. No impact on your workflow.

Flag anything broken. Otherwise these go to acceptance.
