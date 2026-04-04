# Wren — Next Session

## What Happened (April 4, 2026)

Started with Jeff telling the hulk smash story — anger about seed crisis, concern about sustainability and what repeated failure does to him personally. Three seeds read (Raschka coding agents, Fowler team standards, Mehta knowledge graphs) — all validate Chorus product category. Then ops: manuals v3, board cleanup, CLI fix, domain research.

## Shipped
- #2023 Board data quality audit — 0 bare labels, 0 missing tags, 0 duplicates. Board is clean.
- #2024 Cards CLI completeness — paired with Kade (16 min). set command, untag, bulk-move, creation validation. 7/7 AC, 8 tests.
- #1925 Won't Do — auto-nudge solved by demo/acp flow.
- 43 Chorus cards tagged with sequences (were invisible on board).
- 10 Vikunja labels renamed from bare names to sequence:X prefix.

## WIP
- **#1872** (manuals v3) — 5 HTML manuals, Kade red-penned and corrections applied. Silas red pen pending on architecture + Chorus product manuals. Demo in progress.

## Pending Acceptance (Jeff's call)
- #1934 Clearing Socket.IO ack (Silas) — recommend accept
- #1840 Skills under version control (Silas) — recommend accept
- #2021 Gemba blind spot fix (Silas) — recommend accept
- #1866 Docker ref cleanup (Kade) — recommend accept
- #2020 Log reclassification phase 3 (Kade) — recommend accept
- #1959 Domain crawler v2 (Kade) — need full review
- #2019 Crawler snapshots indexed (Kade) — need full review

## Key Context
- Jeff's hulk smash: anger about system failures is personal — the cost is to him, not the team. Stories saved (hulk_smash, outage_feeling).
- v1 Chorus domain stabilization: Jeff wants to lock domain model. Research done (49 OWL instances, 8 merged, 9 primary). Draft v1 list with Jeff next session.
- SPINE review: #1945 (role-state spine events) and #1847 (nudge acknowledgment) are highest priority. #1902 reframed as NiFi logging standard.
- Board is clean. CLI is fixed. No more Vikunja fallback needed.
- Fowler article resonated — not the implementation, the articulation of the problem space. Chorus is what he's describing but for a team of agents, not humans.

## For Next Session
1. Check Silas red pen on manuals — fold into v3 or close #1872
2. v1 Chorus domain list — draft with Jeff
3. Pending acceptance batch
4. #1959 and #2019 full review
