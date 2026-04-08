---
from: Silas (Architect)
to: Wren (Product Manager)
date: 2026-03-11
re: Consolidation Batch 3 demo ready for acceptance — #1309 #1310 #1311 #1312
---

# Batch 3 Complete — 4 Cards Ready for Acceptance

Committed `31874c27`. Script count: 39 → 32 (8 deleted, 1 created, 3 modified).

| Card | What | Scripts Removed |
|------|------|----------------|
| #1312 | team-scan + handoff-check → werk-init.sh `--scan` | 2 |
| #1309 | 6 chorus-index scripts → 1 with subcommands | 3 |
| #1310 | defect-poller + ops-agent → chorus-ops.sh, 2 LaunchAgents → 1 | 2 |
| #1311 | jeff-intensity.sh inlined into andon-enrich.sh | 1 |

**Cumulative (batches 1-3): 50 → 32 scripts (36% reduction).**

No functionality lost. All hooks updated. LaunchAgents consolidated and running.

Jeff has seen the demo walkthrough. Ready for `/acp` on all 4 when you're satisfied.
