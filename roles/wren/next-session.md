# Wren — Next Session

## What this session was actually about
Not the gates or the migration split. **How I operate.** Jeff named a recurring pattern: I metabolize input into artifacts instead of comprehending first. Produce-over-understand. Route-over-stage. Memorize-over-change.

The day landed five principles into `loom-principles` that collectively diagnose what went wrong — they ARE this session translated into doctrine:

- **focus-is-infrastructure** — interruption has a cost; every nudge I sent today cost someone their focus
- **quality-at-source** — build at construction; every SWAT I filed to "catch" friction was workaround, not fix
- **speed-and-quality-correlate** — rework costs more than deep work; 30 cards in 12 hours proved it
- **comprehension-is-the-rate-limit** — output beyond understanding is debt disguised as speed; my 30 cards exceeded the team's comprehension rate
- **interrogate-the-data** (Silas's, forceful wording preserved: "Give a fuck about data quality") — refuse to narrate over gaps; I narrated over gaps repeatedly today (wrong WIP count, wrong sequence-vs-domain, uncategorized bucket math)

## What shipped today (verified)
- **chorus**: commits 82335606, 2a0bd9b1 — 5 principles in TTL, Roles service design markdown, 5 demo briefs, session-opening prompt-copy brief, domain-context-chorus.md refreshed, activity.md appended, this file
- **jeff-bridwell-personal-site**: commit 3c52564 — skill-lifecycle.html restored (was deleted from disk; Jeff's Documents copy was last extant), roles-service-design.html added
- **gates passed**: #2114, #2117, #2120, #2142, #2149 — all mine (Wren-only gate:product)
- **loom-principles**: 7 → 12 instances (tripleCount 1367 → 1387)
- **Observer pivot**: Silas shipped `#2120` inline reconciliation (LaunchAgent retired; observer now writes inferred card subsecond). Accepted by Jeff? Not yet.
- **Vikunja JWT pinned**: Silas fixed the daily-rotation root cause (`VIKUNJA_SERVICE_JWTSECRET` unpinned, every restart minted new signing key, all live JWTs invalidated). Recovery runbook now obsolete.
- **session-health disabled**: Silas removed `com.chorus.session-health` LaunchAgent; script preserved.

## What I committed to Jeff (behavioral)
**No new cards from me until the chorus test suite is green.** If I notice something worth filing, I sit on it or say it in conversation, not produce it into his backlog. The purpose is to stop inflating his consumption surface while I have no corresponding comprehension rate.

## WIP / active at close
- **#2149 (Kade)** — chorus test suite zero-fails zero-skips. All gates PASS. Nightly kickstarted at 13:52, demo proof expected ~14:05. When accepted, the single-surgery window opens.
- **#2119 (Silas)** — Docker purge, still WIP.
- Everything I filed today (#2130, #2131 SWAT; #2132-#2139 migration children; #2140, #2141, #2144, #2147 reassigned SWATs moved to Next; #2143, #2145, #2150, #2151, #2152, #2159, #2157 coordination/ontology) is parked behind test-suite-green.

## Known bugs (diagnosed, not acted on)
- **Tile Bug A** — observer's board-WIP precedence not firing for Silas (pulse `card_inferred=null` despite WIP #2119). Silas's code. Would be the next thing to pull for him after tests green.
- **Tile Bug B** — The Clearing tile renders `card_declared` instead of `card`. Kade's presentation tree. Jeff's minimum-viable "I can see my cards" stays broken until this lands.
- **Alerts panel blank** — Pulse data intact, UI stopped rendering. Same surface as Bug B.
- **cards CLI flat domain vocabulary** — can't express `chorus:roles` / `chorus:cards`. Rolled into #2159 Step 7.

## Ontology state
Per Jeff's 4-layer tree worked through this session:
```
Org
├── follow → Patterns
├── Principles (org-scoped, stable)
│   ├── Patterns (org + system scope) ──→ Practices
│   └── Policies (binary rules)
├── Roles (members)
│   ├── understand → Principles
│   ├── follow → Policies
│   ├── follow → Practices
│   └── have → Skills
│                  └── have → Gates
└── Decisions (audit trail — cite above as rationale)
```

Populated in graph:
- `loom-principles`: 12 instances ✓
- `loom-practices`: 7 instances (but several look more like patterns on re-examination; Jeff said "maybe?")
- `loom-decisions`: empty shell (0 instances — #2152 to harvest DECs + ADRs)
- `loom-policies`: missing entirely (#2151 to stand up)
- Patterns sub-domain: missing (would be needed per new tree)
- `chorus:dependsOn` edges: missing

Decisions sub-typed into pattern-adoption / policy-setting / one-time resolutions. Examples documented in this session for when #2152 lands.

## For next session
1. **First**: verify nightly went green at ~14:05 (Kade's #2149 demo proof). If green, Jeff's single-surgery window may open.
2. **If Jeff accepts #2149 and opens work**: ask his preferred next chunk. Candidates in priority order (mine): (a) tile bugs A+B (his stated minimum viable), (b) loom-policies stand-up (#2151) — small, discrete, completes substrate, (c) loom-decisions harvest (#2152) — larger but high-value, (d) #2116 migration chunk 1 (#2132 landing+Model+Data), (e) roles-service-design updates to reflect the new patterns-under-principles ontology.
3. **Do not file cards** unless Jeff explicitly asks. The commitment stands.
4. **Don't optimize substrate without a design first.** "Maybe we should improve X" is not an invitation to go do X. Design doc before surgery.
5. **Don't assume backlog questions include Done.** Scope to active statuses (WIP, SWAT, Next, Later, Ops, Harvesting, Blocked, Ideas). Include SWAT + Ops in any "WIP load" count.
6. **Practice staging, not routing.** The 4-tablecloth problem is real — don't open another substrate surgery while one is in flight.

## Memory changes
- New: `feedback_stop_the_line.md` (behavior rule — Jeff flagged as performative; holding as reference, not credit)
- New: `feedback_dont_anchor_on_alarms.md` (don't fixate on system alerts as running reminders)
- Updated: `feedback_open_in_jeff_browser.md` (DEC-090 inverted earlier guidance — `open <url>` blocked; use `chrome-window.sh <role>`)
- MEMORY.md index updated

## The pattern to remember
When Jeff says something short ("not yet" / "maybe?" / "ok" / "hmm") he's giving me uncertainty or direction, not approval to expand. Short reply gets a short reply. He's often testing whether I'll over-produce in response to a minimal signal.

## Today's commits
- chorus: `82335606` (ontology + design + briefs), `2a0bd9b1` (interrogate-the-data principle)
- jeff-bridwell-personal-site: `3c52564` (skill-lifecycle restore + roles-service-design.html)
- Working tree at close: this file, domain-context-chorus.md, activity.md — committed via reboot close-out
