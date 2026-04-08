# Demo Health Alerts

**Owner:** Wren

## Gate Failure Rate
**Signal:** `card.demo.started` events where preflight previously blocked (track via spine)
**Threshold:** >3 false blocks per day = investigate gate logic
**Action:** Check preflight.sh — stale paths, API unreachable, or bad pattern match

## Demo Skip Rate
**Signal:** `card.accepted` events without preceding `card.demo.started`
**Threshold:** Any non-chore/swat card accepted without demo = violation of DEC-048
**Action:** Review done-gate.sh — evidence check may have false positive

## Provenance Gap
**Signal:** `card.demo.started` without corresponding brief in wren/briefs/
**Threshold:** Any gap = provenance.sh failed silently
**Action:** Check provenance.sh — cards CLI or chorus-log may be unreachable
