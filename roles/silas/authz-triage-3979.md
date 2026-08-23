# authz triage (#3979) — the gap was mostly a measurement artifact

## The correction
#3977's tool reported "26 of 85 routes declared → 30% coverage, 59 open by construction." Rigorous triage shows that number was **wrong in two ways**, and the real authz posture is far healthier.

**1. The envelope matches by PREFIX, not exact path.** `security-envelope.ts:56`: `req.method === s.method && req.path.startsWith(s.pathPrefix)`. So one declared surface `POST /api/athena/subdomains` covers all ~15 of its `/:id/actors`, `/code`, `/consumes`… sub-routes. Likewise `POST /api/cards/` covers add/done/move/set/tag, `/api/chorus/catalog/` covers lineage/tags, `/api/icd/domains/` covers fields/mappings. #3977 counted every sub-route as undeclared. It isn't.

- Recomputed with prefix-matching: **62 of 81 mutating routes are prefix-covered** by the 26 scope declarations — and enforcement is proven (15/15 under-scoped probes → 403).

**2. The other 19 aren't on chorus-api's scope model at all.** They live on Clearing and pulse, which have their OWN auth:

| count | routes | mechanism | verdict |
|---|---|---|---|
| 10 | `/api/chat/*`, `/api/message`, `/api/room/bind`, `/api/upload`, `/api/voice`, `/set-name` | Clearing bridge-token / CSS session (#3966, #3743) | protected, not scope |
| 4 | `POST/DELETE /mcp`, `/nudge`, `/drain` | loopback-only (mcp #3390, pulse #3967, DEC-093) | accepted trust |
| 4 | `/api/account/password`, `/api/board-event`, `/api/jeff-input`, `/api/restart` | on Clearing (bridge) + pulse (loopback) — verified NOT chorus-api | protected by those models |

**Zero of the 19 are unprotected chorus-api routes.** Every mutating route is guarded by one of three mechanisms: scope envelope (chorus-api), bridge-token/session (Clearing), or loopback-trust (pulse/mcp).

## What the real gap is
Not unprotected routes — **fine-grained authz on the Clearing.** Bridge-token is coarse (it proves you hold the shared secret, i.e. authn, not that you're authorized for a specific action). The Clearing has no per-action scope tiers. That frontier is already carded: **#3645** (passkey sign-in with guest/reviewer/admin tiers). That is the genuine authz work, and it's known.

## What this card fixes
`authz-coverage.sh` must stop over-reporting: (1) prefix-match declared surfaces (so covered = 62, not 26); (2) classify Clearing/pulse routes as `bridge` / `loopback-trust` (justified, like authn's bind-scope `trust`), not gaps. Then the tool tells the truth: chorus-api scope authz is ~well-covered and enforced; the frontier is Clearing tiers (#3645), not 59 holes.

## AC status
- [x] Each uncovered route classified (10 bridge, 4 loopback, 4 verified-on-other-service)
- [ ] `authz-coverage.sh` prefix-matches + scores bridge/loopback as justified (the tool fix)
- [ ] Negative proof: a genuinely unprotected chorus-api mutating route (fixture) still scores GAP
- [ ] The real frontier (Clearing fine-grained authz) is pointed at #3645, not re-carded
