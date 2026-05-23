# chorus-api Event-Loop *Sync Audit (#3039)

chorus-api is the single Node process every role's MCP / cards / search / athena /
nudge call routes through. A synchronous call on the event loop blocks **every**
route for **every** role at once. This audit classifies all synchronous I/O in
`platform/api/src` and records what was converted vs. what stays sync and why.

The card's rule, honored here: **convert request/background-path blockers; do not
bulk-convert.** A sub-millisecond local-SSD `existsSync` guard is not the same
threat as a multi-second subprocess spawn, and churning all of them would risk the
exact regression class (broad untested rewrites) we are trying to avoid.

## The actual freeze (measured)

A 6.24s stall on an idle `/api/athena/tree` request (a ~1ms file read) with no
concurrent load. Root cause was **not** that route — it was `execSync('git log',
unbounded history)` on the loop in `/api/chorus/domain/:name/releases`. Under
git-lock contention on the shared canonical repo it blocked the whole loop; every
in-flight route (athena/tree included) froze together. **One sync subprocess call
poisoned the shared process.**

## Converted (request-path, real blast radius)

| Site | Was | Now |
|------|-----|-----|
| `server.ts` `/api/chorus/domain/:name/releases` | `execSync('git log', UNBOUNDED)` | `await execAsync('git log -n 300 …')` — off the loop **and** bounded |
| `server.ts` `/api/chorus/open` | `execSync('open "…"')` (also shell-injection via string interp) | `await cardsExecFileAsync('open', [resolved])` — off the loop, no shell |
| `session-replay.ts` + `handlers/sessions.ts` | `readFileSync` of rrweb files (can be **MB**) | async reads; `/sessions`, `/sessions/:id`, `/:id/log` routes now `await` |

Result: **zero live `execSync` calls remain** in `src/`. Subprocess spawns — the
only calls with multi-second, lock-contended blast radius — are all async.

## Justified sync — left deliberately, by class

**1. Batch / CLI context (not on the API request loop).**
`index-all-sources.ts` (33), `discover-code.ts`, `discover-tests.ts`,
`discover-pages-*.ts`, `seed-loom-decisions.ts` — indexers/seeders run by
LaunchAgents (`com.chorus.crawler-index`) or as CLI entrypoints, not from Express
handlers. Sync I/O there blocks the *batch* process, never the API loop.

**2. Startup / module-load (runs once at boot).**
Path resolution (`SCRIPTS_DIR` candidate-finding, `DB_PATH` existence), config
reads. Executed at import time before the server accepts traffic; converting them
buys nothing.

**3. Cheap, bounded request-path reads (sub-ms, local SSD, small/bounded files).**
`handlers/athena-tree.ts` — one small JSON read, `statSync`-gated, dependency-
injected (testable). `handlers/doc-catalog.ts` — `readFileSync(...).slice(0,2000)`
(bounded to 2 KB), small registry/links JSON, bounded `readdirSync` listings.
`existsSync` guards throughout (a `stat`, microseconds). Left sync deliberately:
high churn, low value, and bulk-converting risks regressions. Revisit a specific
site only if profiling shows it hot.

**4. Background interval (periodic, brief, off the request critical path).**
`health-cache.ts` sync reads run on the 30s `refreshHealthCache` tick, not per
request. Brief and periodic; acceptable.

## Why this is enough (and where the real guard belongs)

The blast-radius math is what matters: a subprocess spawn under lock contention
blocks for **seconds**; a local-SSD `existsSync`/small `readFileSync` blocks for
**microseconds-to-low-ms**. The seconds-class calls are gone. The remaining sync
I/O is documented above by class, not left implicit.

The durable defense is **not this doc** — docs rot (cf. ADR-031). It is the new
visibility: `nodejs_eventloop_lag_p99_seconds` is now scraped by Prometheus every
15s, and the chorus alert `chorus-api-eventloop-lag` (proving/domains/alerts/)
queries `max_over_time(...[6m])` and **nudges Silas's Claude session** (ops owner,
DEC-022 — not a macOS notification) the moment any *new* loop-blocker — sync or
otherwise — pushes p99 over 1s. We will *see* the next freeze instead of guessing
at it for an afternoon. A CI guard rejecting new `execSync`/`spawnSync` in request
handlers is a reasonable follow-on, but is out of this card's scope.
