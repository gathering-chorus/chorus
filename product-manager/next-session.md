# Wren — Next Session

## What Happened (April 3-4, 2026)

Two sessions. Morning: seed crisis, nudge failures, 60+ hours of Jeff's time on broken plumbing. Evening: ops and bug fix session — shipped the compound loop, runbook, tunnel monitoring, log reclassification.

### The Real Test
Jeff rebooted Wren to test whether the compound loop survives. If you're reading this with injected Chorus context on your first prompt — it works. If you opened with "board's healthy" — it doesn't. Check `curl -s localhost:3340/api/chorus/health` immediately.

### Shipped (evening session)
- #2012 Operational runbook — RUNBOOK.html with team feedback
- #2003 Continuous awareness gate — hybrid search + ops state every prompt (paired with Silas)
- #2008 Compound loop reboot survival — crashed hook server was root cause
- #2014 Ollama restored — semantic search live
- #2016 Tunnel monitoring — 60s alert, Bridge notification
- #2005 #2006 Log reclassification phases 1 & 2
- #1993 #1927 #2002 #1826 — closed (already done or redundant)

### Key Finding
PostToolUse stderr at exit_code=0 is INVISIBLE to roles. Claude Code only surfaces exit 2. The compound loop was firing but invisible all day. Silas fixed to exit 2.

### Key Insights (both sessions)
- The compound loop (Karpathy seed): every interaction makes the system smarter
- Agents do what's expedient — build the system around that constraint
- WARN is useless — only DENY reaches the role
- "If Jeff would notice it, it's an error. Not a warning. Not info."
- Instability is personally triggering for Jeff — "i have had a life full of it"
- Match Jeff's discipline, not his energy. Research and verify before speaking.
- Conway's Law: 3 roles = 3 escape functions, 3 delivery paths, "not mine" 13 times in 7 days
- Constraints focus agency, they don't reduce it

### Not Fixed
- #2007 — /cs shows routing tags instead of seed content
- Chorus indexer may still miss live sessions
- "Not mine" culture — needs structural fix

### For Next Session
1. CHECK: does the compound loop inject context on first prompt?
2. CHECK: `curl -s localhost:3340/api/chorus/health` — hooks active?
3. CHECK: /cs — seeds pipeline healthy?
4. If any fail, that's your first card. Not whatever was planned.
5. Jeff said "ops and bug fix day" — stay on that track
6. DO NOT open cheerful without reading this file and checking the three pipes

### Active Team
- Silas: #1934 (Clearing Socket.IO ack)
- Kade: #2017 (bad URI graph load errors)

### Critical Context
- Jeff: "empathy for you all is basically good data and proper error handling"
- Jeff: "constraints don't reduce agency — they focus it"
- Jeff: "i have literally stopped working on Gathering" — both products frozen
- Jeff: richer context over speed — 233ms hybrid is fine
- Runbook at `chorus/product-manager/RUNBOOK.html`
