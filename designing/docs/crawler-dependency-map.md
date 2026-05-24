# Crawler / index / embed pipeline — dependency map (#3071)

Built 2026-05-24 by two subagent traces verified against the running system (outputs + consumers), NOT inference. Supersedes the wrong "it writes one SQLite row" claim.

## The big corrections this map made

1. **The crawler DOES write the graph.** `crawler-hydrate-graph.sh` writes `chorus:File` triples (filePath, fileSha, fileLastModified) into the `urn:chorus:instances` Fuseki graph + a `chorus_files` SQLite table. **Jeff's instinct ("I thought crawler wrote to graph") was right; my "it only reads the graph" was wrong.** It's a *separate script* from index-crawler-snapshots.sh — which is why I missed it reading only the one file.
2. **"Would anyone notice if we turn it off?" — YES.** The outputs have real load-bearing consumers, above all `context_inject` which reads crawler data on **every prompt**. Turning the crawler off would degrade per-prompt grounding for all three roles AND trip 4 alerts. **Jeff's "keep it on, we have deps we don't understand" was vindicated by the trace.**
3. **The flood was `WatchPaths`, not StartInterval** (which is correctly 1800/30min). Routed to Silas to gate. Safe because it removes no data.
4. **The `traces` table is the real volume + the waste candidate:** the crawler's `trace_hop()` writes **108 rows/minute** (4 hops × 27 domains), **144K of the 301K total trace rows** — and no load-bearing consumer was found for them. High write cost, unclear payoff.

## Outputs → consumers → what breaks if it stops

| Output (store) | Load-bearing consumers | If it stops |
|---|---|---|
| **messages `source='crawler'`** (27 rows, delete+insert) | `context_inject` hybrid grounding (EVERY prompt) · `/api/chorus/search` FTS/hybrid/unified · `embed-delta`→LanceDB | per-prompt domain-snapshot grounding lost; search loses domain layer; semantic loses domain vectors |
| **spine events `crawler.*`** | `crawler-stale.yml` (5-min alert→Kade) · `hydration-divergence.yml` · `context_inject` Spine + Loki-error blocks (every prompt) · pain rollup · `indexSpine` (re-indexed into messages) | crawler-stale fires in 5min; per-prompt context loses crawler signal; pain board loses attribution |
| **`/tmp/crawler-domain-status.json`** | `crawler-error.yml` (5-min→Kade) · `crawler-domain-failure.yml` (hourly→Silas) | both alerts go silent (designed: no file = treated non-failing; crawler-stale covers "not running") |
| **LanceDB vectors** | `semanticSearch` → `/search` semantic/hybrid/unified · `context_inject` (indirect) | semantic search → FTS-only (graceful); index-freshness alert fires |
| **Fuseki RDF graph** (crawler-hydrate-graph.sh) | `/crawl/:domain` collectRdf/collectOwl · stale/deletes reconciliation | crawl RDF bucket empties; stale-flag detection stops |
| **`traces` (108/min, 144K rows)** | **none load-bearing found** (trace-viewer/diagnostics only) | likely nothing notices — the waste candidate |
| messages other sources (spine/brief/…), watermarks | search, self, freshness monitoring | per-source; freshness alert |

## Circular dependency (note for decomposition)

`/crawl/:domain` → `collectMentions` READS `messages_fts` (all sources incl. existing crawler rows) → builds the snapshot → written back as a NEW `source='crawler'` row → re-indexed → read again. The crawl reads the index, then writes to the index. (#2323 "eating its own tail.") Blast-radius-relevant for #3069.

## Implications for #3069 / ADR-033 (the decomposition)

- **Decomposition must preserve the load-bearing consumers** — especially `context_inject`'s every-prompt reads (crawler rows + crawler spine events + LanceDB) and the 4 alerts. These are the "runtime dependencies we don't understand" — now mapped.
- **The graph-write (`crawler-hydrate-graph.sh`) is the to-be done right already** — structure into the graph; the code/test-metadata exemplar. Extend that pattern; retire the flat-summary-to-messages where the graph can hold it.
- **The `traces` 108/min is the first thing to question** — highest write volume, no found consumer. Likely cut or sample, not decompose.
- **Safe vs needs-care:** frequency throttle (WatchPaths) = safe, removes no data. Turning off any OUTPUT = needs care (this map shows which break). Turning off the `traces` writes = likely safe (no consumer found) — verify.
