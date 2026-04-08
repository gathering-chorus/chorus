# Memory File Audit — Card #106

**Date**: 2026-02-22
**Author**: Wren
**Purpose**: Identify stale, duplicate, and bloated entries across all state files. No edits — report only.

---

## File Inventory

| File | Lines | Budget | Status |
|------|-------|--------|--------|
| MEMORY.md | 196 | ~200 (truncation risk) | CRITICAL — at capacity |
| self-stories.md | 161 | No limit | Healthy |
| decisions.md | 338 | No limit | Growing — needs archival strategy |
| projects.md | 58 | No limit | Stale in spots |
| backlog.md | 198 | No limit | Moderately stale |

---

## 1. MEMORY.md Audit (196 lines — at truncation threshold)

MEMORY.md is the highest-priority file. At 196 lines, it is at the ~200-line truncation boundary. Every line needs to earn its place. The file carries context for ALL sessions across ALL roles, so bloat here has the highest cost.

### P1 — Causes Wrong Behavior

**1.1 Kade section: stale "IN PROGRESS" status (lines 160, 182)**
- Line 160: "Test suite audit COMPLETE, fixes IN PROGRESS (card #128): ... 2 of 3 placeholder tests fixed"
- Line 182: "Card #128 IN PROGRESS: Test suite gap fixes"
- **Reality**: Kade's `current-work.md` shows card #128 is COMPLETE. All 3 placeholders fixed, 2268 unit tests, 32/32 handlers covered.
- **Risk**: Next Kade session picks up stale work. P1.

**1.2 Kade section: stale "Next Priority" references (line 184)**
- Line 184: "Feature candidates: Claude vision auto-annotation (#124), music artist browse (#56), book import location-optional (#46)"
- These are accurate as candidates but the framing "Next Priority" is misleading since card #128 is shown as current. Kade has no card in Now per current-work.md.

**1.3 "DEC-029/DEC-030" on line 185 references superseded decision**
- DEC-029 is superseded by DEC-030. Line 185 says "Chorus = Silas leads, Kade reviews" which is DEC-029's framing. DEC-030 says each role owns their Chorus vertical. Keeping the superseded framing could cause Kade to defer Chorus presentation-layer work to Silas.
- **Fix**: Remove DEC-029 reference, keep only DEC-030.

**1.4 Contradictory Chorus identity (lines 56 vs 57)**
- Line 56: "Chorus = the nervous system (Wren named this — DEC-019)"
- Line 57: "Chorus = shared awareness (2026-02-22, reframed from 'nervous system')"
- Both are in the same section. The nervous system metaphor was explicitly superseded by "shared awareness" on 2026-02-22. Having both will confuse roles about which framing to use.
- **Fix**: Remove "nervous system" from line 56 naming vocabulary. Keep "shared awareness" entry.

### P2 — Wastes Context (tokens burned on stale/duplicate info)

**2.1 SHIPPED items that are now in CLAUDE.md or code — can be removed (saves ~25 lines)**

These items exist in MEMORY.md as reminders but are now enforced by generated CLAUDE.md files, scripts, or code:

| Line | Entry | Now Enforced By |
|------|-------|-----------------|
| 11 | Card-first gate (DEC-033) | CLAUDE.md "Card-First Gate" section |
| 12-13 | CLAUDE.md generator details + open questions | CLAUDE.md `<!-- GENERATED -->` header; generator is the tool, not a memory item |
| 83 | DEC-035: Signal, don't narrate | session-start.sh, chorus-audit.sh already implement this |
| 84 | session-start.sh SHIPPED | CLAUDE.md "Starting a Session" section |
| 85 | chorus-audit.sh retrofitted | Implemented in hook; not a memory item |
| 86 | Startup procedure | CLAUDE.md "Starting a Session" section (verbatim) |
| 87 | chorus-prompt.sh SHIPPED + FIXED (including OSC bug) | chorus-prompt.sh in code; the bug fix detail is an engineering note, not ongoing memory |
| 88 | team-scan.sh bug fixed | Fixed in code; bash syntax detail is not ongoing context |
| 125 | Infra guardrails (PreToolUse hook) | Hook is in code; settings.json enforces |
| 79 | Permission profiles LIVE | settings.json files ARE the profiles |

**2.2 Duplicate entries (same info stated two ways)**

| Lines | Duplication |
|-------|-------------|
| 56 + 60 | Chorus naming: line 56 has "Chorus (DEC-019): Wren suggested the name..." and line 60 repeats "Chorus (DEC-019): Wren suggested the name. Silas shipped ontology v0.1.0" |
| 62 + 52 | Context service: line 62 describes full context service, line 52 says "/chorus skill location: Now in chorus repo, symlinked to ~/.claude/skills/chorus/" — this is a detail of the context service, not a separate item |
| 146 + 87 | Chorus prompt: line 146 "Chorus prompt standard SHIPPED" duplicates line 87 "chorus-prompt.sh SHIPPED + FIXED" |
| 144 + 151 | Clearing: line 144 mentions Chorus spine+roadmap shipped, line 151 describes Clearing shipped — both are valid but the spine entry (line 144) is purely a SHIPPED announcement that could live in backlog.md Done section |

**2.3 Historical context that no longer drives behavior (~20 lines)**

| Line | Entry | Why Stale |
|------|-------|-----------|
| 18 | Security audit (2026-02-19): ALL Docker services bound to 0.0.0.0 | Resolved — ADR-012 activated, all 14 services fixed. Line 127 confirms. Remove the problem statement (line 18), keep the solution (line 127). |
| 24 | Group conversation shipped (2026-02-19) | @team bridge is being deprecated. The Clearing replaces group conversations. This is historical, not actionable. |
| 27 | Memory audit layer (2026-02-19) | Shipped. Git hook works. Not an active memory item. |
| 28 | Board.sh fragility (2026-02-20) | Resolved. board-ts replaced board.sh. Line 9 already says "board-ts CLI". Remove the problem statement. |
| 119 | Disk recovered (2026-02-19) | Crisis resolved 3 days ago. Not ongoing context. |
| 136 | Deploy lock (2026-02-20): "Change is UNCOMMITTED in Kade's repo" | Need to verify if this was committed. Either way, "uncommitted" status is stale. |
| 138 | Auth bugs found (2026-02-20): "Brief sent to Kade" | Kade's memory section says "Auth session bugs SHIPPED" was in an earlier version. The brief exists at `engineer/briefs/2026-02-20-auth-session-bugs.md`. Need verification if shipped. Either way, "brief sent" is not the current status. |
| 147 | Team feedback on roadmap (2026-02-21) | Feedback received 2 days ago. Either acted on or not — stale as a memory item. |
| 171 | Silas has uncommitted changes in app repo | From 2026-02-21. Either committed by now or still pending. "Uncommitted" is a transient state that belongs in next-session.md, not permanent memory. |

**2.4 Kade section: Test Suite Audit Findings too detailed (lines 174-179, 6 lines)**
- Full test coverage percentages, specific untested handlers, anti-patterns list
- This belongs in `engineer/current-work.md` or `engineer/tech-debt.md`, not in cross-project memory
- Card #128 is now COMPLETE — these findings are historical

**2.5 Kade section: Loki consolidation context (line 186)**
- Engineering implementation detail about Terraform workspaces and Promtail filters
- Belongs in engineer notes or architecture docs, not cross-project memory

### P3 — Cleanup Opportunities

**3.1 Line 9: stale path warning can be simplified**
- "NOT `messages/board-client/dist/cli.js` — that path is stale" — the stale path warning was needed months ago. Now just say where it is.

**3.2 Infrastructure section (lines 115-153) is 38 lines — largest section**
- Many entries are SHIPPED announcements (dashboards, pipelines, scripts). These could move to a `chorus-infrastructure-log.md` or similar.
- The section mixes actively-needed reference data (Loki port, primary Mac specs) with historical shipped-item announcements.

**3.3 Express route ordering lesson (line 145)**
- One-time engineering lesson. Belongs in engineer notes, not cross-project memory.

### Recommended Splits to Reduce MEMORY.md

If all P1+P2 items are addressed, MEMORY.md drops from 196 to approximately 130-140 lines. That is within budget. However, if the file continues growing, these topic files would help:

| Proposed File | Content | Lines Saved |
|---------------|---------|-------------|
| `memory/infrastructure-shipped.md` | All SHIPPED infrastructure items (dashboards, pipelines, security fixes) | ~15 |
| `memory/kade-engineering-notes.md` | CSP lesson, deploy lessons, Express gotchas, test findings | ~20 |
| Move to `engineer/current-work.md` | Kade's "Current State" and "Next Priority" sections | ~10 |

**Recommendation**: Do NOT split yet. Prune the P1 and P2 items first. That should bring MEMORY.md to ~135 lines with room to grow. Splitting adds file count, which adds session-start read time.

---

## 2. decisions.md Audit (338 lines)

### Healthy Overall
- 38 decisions (DEC-001 through DEC-038), all with date, context, decision, rationale, status
- Format is consistent
- No duplicates

### Issues Found

**P2: DEC-029 should show "Superseded by DEC-030" more prominently**
- Line 257 says `Status: Superseded by DEC-030` which is correct
- But MEMORY.md line 185 still references DEC-029 as if current (see P1 item 1.3 above)

**P2: DEC-005 (shared meeting docs) may be obsolete**
- Meeting docs were the original cross-role mechanism. Briefs and The Clearing have replaced this in practice.
- Status still says "Accepted" — should it be "Superseded by briefs + Clearing"?

**P2: DEC-012 (Slack integration) status says "Planned"**
- Slack is shipped and now being deprecated. Status should be "Accepted — deprecation in progress"

**P3: Growing length**
- At 338 lines, this file will eventually need an archival strategy (e.g., move decisions older than 30 days to `decisions-archive.md`)
- Not urgent — decisions are append-only and referenced by number. Archiving prematurely risks broken references.

---

## 3. projects.md Audit (58 lines)

### Issues Found

**P2: "Last updated: 2026-02-21" — 1 day stale (line 2)**
- Cost dashboard shipped, external traffic monitoring shipped, shared observability hardened — none reflected here.

**P2: Gathering health section is outdated (line 14)**
- Says "1613 unit tests" — Kade's current-work.md shows 2268 as of today.
- Says "Coverage 82%" — likely higher now after card #128 test fixes.

**P2: shared-observability section is stale (lines 41-44)**
- Says "Recent: Added canvas infrastructure diagram, blackbox-exporter, mysqld-exporter (2026-02-12)"
- Reality: Resource limits shipped, Alertmanager pinned, Promtail persistence added, external traffic monitoring, cloudflared Prometheus scrape — all in last 3 days. None reflected.

**P2: Chorus section missing recent work (lines 22-31)**
- Missing: Clearing voice tuning (C#37), Seeds pipeline (#126), manifest handoffs (C#43/WF-009), permission prompt logger (C#38), workflow engine (all 7 complete)
- Says "Slack deprecation in progress — /team page (C#25) ships next" — verify if C#25 has shipped

**P3: wordpress-blog section (lines 33-38)**
- Says "Health: Stable" with last recent item from 2026-02-13. Probably accurate but worth verifying.

---

## 4. backlog.md Audit (198 lines)

### Issues Found

**P1: "Now" section may not match board**
- Lists Clearing voice tuning (C#37) and Seeds capture flow (#126) as Now
- Need to verify against `board-ts list` and `board-ts --chorus list` — this file should mirror the board

**P2: "Last updated: 2026-02-20" (line 4) — 2 days stale**
- Multiple cards have moved since then (card #128 done, WF-007 done, new cards created)

**P2: Whisper voice/video capture in Next (line 32)**
- Says "deploy failed on first attempt" — this status is from days ago. Either retried or still stuck.

**P2: Done section stops at 2026-02-20 (line 161)**
- Missing all 2026-02-21 and 2026-02-22 completions: Clearing UX (DEC-037), CLAUDE.md generator (#104), session-start.sh, chorus prompt, all 7 workflows, card #128 test fixes, WF-007 Loki consolidation, cost dashboard, external traffic monitoring, shared observability hardening

**P2: Permission prompt logger (C#38) listed as both Next and P1 (lines 82-86)**
- It's in the "Next" section but marked P1. If it's P1, should it be in Now? Or is it blocked waiting on Silas consultation?

**P3: Tech debt table (lines 149-157)**
- References cards #33-39 — need to verify these still exist on the board and haven't been addressed by card #128's test suite work

---

## 5. self-stories.md Audit (161 lines)

### Healthy
- 15 stories organized by 5 themes
- Each has date, narrative, and "What this tells us" analysis
- Most recent entry: 2026-02-22 (Meditation to Kitchen)
- No duplicates, no staleness

### One Note
**P3**: The "Revenue & Independence" section (lines 154-161) overlaps with the "Career Vision" story (lines 101-112). Not a duplicate — one is the original vision document, the other is the current revenue thinking — but they could be consolidated into a single "Career & Revenue" section.

---

## Summary: Prioritized Action List

### P1 — Fix Now (causes wrong behavior)
1. MEMORY.md line 160/182: Update Kade card #128 status to COMPLETE
2. MEMORY.md line 185: Remove DEC-029 reference, keep DEC-030 only
3. MEMORY.md lines 56 vs 57: Resolve Chorus identity contradiction (keep "shared awareness")
4. backlog.md: Verify Now section matches actual board state

### P2 — Fix Soon (wastes context tokens)
5. MEMORY.md: Remove ~10 SHIPPED items now enforced by code/CLAUDE.md (lines 11, 83-88, 125, 79)
6. MEMORY.md: Deduplicate 4 repeated entries (Chorus naming, context service, chorus prompt, Clearing)
7. MEMORY.md: Remove ~10 historical entries that no longer drive behavior (lines 18, 24, 27, 28, 119, 136, 138, 147, 171)
8. MEMORY.md: Move Kade test audit findings (lines 174-179) to engineer/current-work.md
9. projects.md: Update all sections to current reality (test counts, recent work, shared-observability)
10. backlog.md: Update Done section through 2026-02-22; update stale statuses
11. decisions.md: Update DEC-005 and DEC-012 statuses

### P3 — Cleanup When Convenient
12. MEMORY.md: Simplify board-ts stale path warning (line 9)
13. MEMORY.md: Move Express route ordering lesson to engineer notes (line 145)
14. self-stories.md: Consider merging Career Vision + Revenue sections
15. decisions.md: Plan archival strategy for when file exceeds 500 lines

### Estimated Impact
- P1 fixes: prevent 3 wrong-behavior scenarios
- P2 fixes: reclaim ~45-50 lines in MEMORY.md (196 → ~145), bringing it well within budget
- P3 fixes: minor cleanup, no urgency

---

*This is a read-only audit. No files were edited. Wren will execute approved changes in a follow-up session.*
