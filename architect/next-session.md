# Next Session — Silas

## Shipped
- #2241 — Deep-health full system coverage + Chorus API freeze fix (13 execSync→async)
- #2245 — Overnight nudge inject (do script replaces keystroke, no TCC/display dependency)
- #2246 — Context-cache-weekly Rust subcommand (cruft scan, stale card audit, disk trend)
- #2254 — Role-state.sh path fix in 4 skills (pull, pair, gemba, gemba-tick)
- #2256 — MonitoringService async (cached disk checks, 136K blocking ops eliminated)
- #2260 — Node health instrumentation (perf_hooks histogram, memory tracking, role-targeted alerts)
- #1964 — Cards CLI update desc (already existed, closed)
- #2253 — Won't Do (superseded by #2260)

## Also fixed without cards
- Cloudflare tunnel switched QUIC→HTTP/2, logs out of /tmp into Loki
- 7 alert rules: dual-path (nudge to role + Bridge POST for Clearing)
- Tunnel-down alert: 3-retry over 60s + auto-restart on sustained outage
- Deep-health: auto-heal loki tunnel zombie, weekly log threshold
- Chorus API request logging added
- chorus-inject binary rebuilt with do script

## Open
- #2262 — Restore alerts panel on Clearing streams (carded, not pulled)
- Overnight inject verification — tonight proves #2245 AC2 (display-sleep delivery)
- RSS at 1239MB baseline — threshold set at 1500MB, monitor for growth
- Cloudflare tunnel still drops HTTP/2 connections ~4800/day but auto-recovers + auto-restarts

## What went wrong
- Set memory alert threshold at 500MB when app uses 1200MB — fired 5 alerts into Jeff's terminal
- Deleted a LaunchAgent (context-cache-weekly) without understanding what it did
- Chorus-inject binary not rebuilt after source change — inject failed for 3 hours
- Guessed at causes (network, display sleep) instead of querying Loki/Prometheus first
- Jeff had to tell me 10+ times that alerts go to roles not him

## Jeff's state
Engaged, directive, patient but firm. Pushed hard on root cause discipline, observability tooling usage, and alert routing. Interested in Böckeler's harness engineering framework. Wrestling with refactor vs rewrite — decided refactor. Good collaborative session despite the friction.
