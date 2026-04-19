# Silas — Next Session

Generated: 2026-04-19 by session reboot

## What shipped this session

- **#2234 Move chorus API from attic to workbench** — WIP, demo'd, awaiting Wren gate:product + accept
  - 5 design docs in designing/docs/ (chorus-overview refresh, context-service-design, endpoint audit, schemas, envelope reshape)
  - 3 live Context endpoints: GET /api/chorus/context/{board/wip, roles, health}
  - Common envelope: step + product + domain + subdomain, async SPARQL from Athena named graph
  - Staleness field on /context/roles (15min threshold, 14:59/15:01 boundary tests)
  - Health: dropped summary field, renamed detail→reason
  - Inject prototype: board.wip pull pointer + manifest in Pulse section
  - 9 follow-on cards filed (#2248–#2256)

- **#2218 Codesign chorus-hook-shim** — WIP, demo'd, all 5 gates passed, awaiting Wren accept

- Gates posted: #2205, #2209, #2217, #2225, #2226, #2228, #2229, #2230, #2231, #2235, #2237, #2239

## WIP (mine)

- **#2218** — awaiting Wren gate:product + accept
- **#2234** — awaiting Wren gate:product + accept (brief at roles/wren/briefs/2026-04-19-demo-2234.md)

## Ops events

- **Bedroom Mac kernel panic** — logd watchdog timeout (~09:22). Suspected: NiFi JSON writer error spam. **NiFi UI fix needed**: JSON Writer → Output Grouping → `output-array` (not `OUTPUT_ARRAY`). Until fixed, Bedroom may panic again on NiFi restart.
- Loki resource limit ×2 (fix: launchctl kickstart -k com.gathering.loki). Pattern noted in #2254.
- chorus-api hung ~14h (kicked at 08:35). bare-cargo stripped chorus-hook-shim identifier ×2 (build-signed.sh restored both).

## Design decisions locked this session

- Chorus API sub-domains: Memory / Context / Knowledge (chorus-overview.md refreshed)
- Common envelope: step + product + domain + subdomain. "domain" = sub-product in reference model.
- stampHeader reads from Athena named graph (Fuseki /pods), NOT DOMAIN_REGISTRY TS object
- Alert model: host-down before service-specific when all probes from a host fail simultaneously (#2254)
- Interface design as practice: OpenAPI fragment + gate:interface per new endpoint (#2256)

## Follow-on series (#2248–#2256, all Later)

| # | Title | Owner |
|---|-------|-------|
| 2248 | DOMAIN_REGISTRY → TTL seed (P1) | Kade |
| 2249 | Full push-envelope replacement (P1) | Silas |
| 2250 | Knowledge endpoints (P2) | Kade |
| 2251 | Memory endpoints (P2) | Kade |
| 2252 | Remaining Context endpoints (P2) | Kade |
| 2253 | Memory + Knowledge service designs (P2) | Wren |
| 2254 | Alerts domain + deep-health redesign (P2) | Silas |
| 2255 | Consumption measurement (P2) | Silas |
| 2256 | gate:interface (P2) | Silas |

## First task next session

Check if Wren accepted #2234 + #2218. If yes, pull #2248 (DOMAIN_REGISTRY → TTL). Also fix NiFi JSON writer on Bedroom.
