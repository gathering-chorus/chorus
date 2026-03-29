# Log Coherence Audit — Prioritized Punch List

**From:** Wren | **To:** Silas | **Date:** 2026-03-01

Full HTML audit at `product-manager/log-coherence-audit.html`. Overall score: **B-**. Schema consistency is strong (A), but coverage balance (C+) and lifecycle completeness (C) need work. Here's your punch list, priority order.

## Top 5 (all your vertical — infra/scripts/config)

### 1. Add handoffs.log to Promtail (Small)
317 role-to-role handoff events exist on disk but aren't queryable in Grafana. One scrape job block in `promtail-config.yaml`. Same JSON pipeline pattern as command-errors.

### 2. Add audit logs to Promtail + rotation (Medium)
~22K SOLID compliance events in `data/audit/audit-YYYY-MM-DD.jsonl` — the only log of what Jeff actually does in the app. Not scraped, no rotation, accumulating indefinitely. Glob scrape + daily rotation policy.

### 3. Emit demo/accept spine events (Small)
DEC-048 Proving gate (deploy→demo→accept) is the highest-value moment in the stream and it's invisible. Add to board-ts: `card.demo.started`, `card.accepted`, `card.rejected` via chorus-log.sh.

### 4. Add rotation to chorus.log + permission-prompts.log (Small)
Only command-errors.log has rotation (10K→7.5K). chorus.log is 13K and growing ~500/day. permission-prompts.log is 28K and growing ~2K/day. Disk at 87%. Same tail-truncate pattern.

### 5. Fix defect-poller Fuseki false positives (Small)
defect-poller.sh created 30+ cards for normal Fuseki PUT operations (INFO-level pod writes). Filter pattern needs to exclude INFO PUT lines. I moved 7 false-positive cards to Won't Do today.

## Lower Priority

- **Resolve dual-path Loki dedup** — emitSpineEvent() pushes to Loki AND writes chorus.log (which Promtail also scrapes). Same event can appear twice.
- **Bedroom Mac Promtail** — 178TB media served from an unobserved machine. Large effort, card when ready.

## Board Cleanup Done

25 cards moved to Won't Do today: 7 Fuseki PUT false positives + 18 transient/duplicate ops-agent cards. The noise ratio was high.

Good harvest-monitoring work — all items are small/medium and parallelizable.
