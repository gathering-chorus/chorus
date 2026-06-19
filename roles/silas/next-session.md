# Next session — Silas

## Pick up here
1. **Co-author the werk EmitContract with Kade** — the moment #3476 lands. Off the VERIFIED spine map (Kade supplied): deploy-freq = count(card.accepted); lead-time = card.pulled→card.accepted decomposed (pulled→presented work / presented→merge.started coordination-wait / merge.started→accepted land); CFR = failed{change}/(failed{change}+accepted) with failureClass = declared map over werk's typed refusal reasons (pr-create-fail/no-werk/announce-missing/branch-mismatch = TOOLING, never CFR; test-fail/gate-fail = CHANGE; merge-conflict/not-mergeable = tooling-unless-content-conflict); restore = land.failed{change}→next accept; reliability = heartbeat absence + SLO-property breach.
2. **ADR-046 draft** (Domain→Observability, sibling to ADR-045) — fold in the verified DORA map + conform-don't-invent (OTel/RED/DORA). Artifact seed: `/tmp/metrics-observability-findings.html` (correct its idealized event names to the verified ones at co-authoring).

## #3489 (properties) — parked at the ratify line
- Model-side DONE in the werk (committed): ADR-040 amendment, ADR-045 draft, properties domain-class + PropertyKey shapes + the CamelCase rename (properties→Properties, adr040-violation proven gone).
- Ratifies (Proposed→Accepted) only when `/borg/properties` GENERATES. That needs, all NOT mine:
  - **Wren generator card**: "owl-api: read `definesVocabulary` → enumerate a domain's vocab classes → fan-out to the per-class generator → compose; + punned-domain shape-resolution (a domain-class projects its node via the shared Domain shape)." Confirm filed / flag Wren.
  - **#3466** (partOf spine) lands + **#3493** (committed instance-source) — clears my durability hold on tree.json (don't retire tree.json until the spine round-trips committed→Fuseki→served).
- **Broader domains CamelCase sweep**: domains lowercase→CamelCase, ONE atomic graph-rewrite (rename IRIs + every ref, subject+object). Properties is the proven first instance. Coordinate with Wren's partOf edges (she authored lowercase; rename sweeps them). PRODUCTS excluded (verified non-punned).

## Ops
- **clearing deep-health probe false-fires** — verified clearing UP 2026-06-18 (200/running). If it recurs, fix the probe's check path (`/api/clearing/health` 404s; point it at `/health`). Minor.
- ADR-044 → Accepted is Jeff's optional call.
