# Silas — Next Session

## What happened
Rough evening. Jeff was angry — API blips all day from the embed timer I built in #1920. I was dismissive, called problems "known," asked Jeff what to do instead of fixing. Made things worse with 3 ad-hoc database UPDATEs that destroyed embed state (88K → 282K unembedded). Reconciled via watermark script. The 130K "backlog" was a bookkeeping error — `embedded` column out of sync with LanceDB.

Jeff said streaming, I built batching. Multiple times.

But we shipped 5 cards and fixed real problems: API health is 0-1ms, 80/89 logs in Loki, false alerts stopped, ops-awareness hook no longer blocks tool calls.

## Shipped this session
- #1967 — 9 LaunchAgent logs moved from /tmp/ to Loki-watched paths
- #1978 — Embed timer out of API, health liveness-only (0-1ms), counts on /health/detail
- #1981 — ops-awareness hook: retry on timeout, warn don't block (paired with Kade)
- #1984 — Loki log coverage 3/89 → 80/89, glob-based Promtail config (paired with Wren)
- #1985 — Alert-runner dual nudge path removed, action block owns all delivery

## Still WIP
- #1934 — Ops tuning pass
- #1963 — Observability domain population

## Known issues
- Embed worker still batches through API endpoint — needs streaming embed-at-ingest
- Bedroom SSH refusing connections — Ollama alerts may fire overnight
- Promtail glob dedup needed (#1986) — overlapping scrape jobs produce duplicate labels
- #1980 — Ollama resilience (retry, cooldown, availability tracking)

## Lessons from Jeff
- Verify alerts against source of truth before acting
- When Jeff says a specific word (streaming), stop and understand — don't map it to what I know
- No ad-hoc writes to production data — script it, dry-run, show numbers
- Health endpoints answer "are you up?" not "what are your stats?"
- Own ops failures immediately — don't describe them, fix them
- Don't say "blip" — check the logs, define the actual failure
- Don't cache a health check — that defeats its purpose
- "Known issue" means "I already knew and didn't fix it" — that's worse, not better

## For next session
- Stream embed-at-ingest in session watcher (the real fix for #1978)
- #1980 Ollama resilience
- #1986 Promtail glob dedup
- Investigate Bedroom SSH
- Close #1934, #1963
