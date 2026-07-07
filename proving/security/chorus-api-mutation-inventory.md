# #3619 — chorus-api mutation-endpoint inventory (AC1)

**Running-system-grounded** — endpoint list from `server.ts` (grep of `app.post|put|delete|patch`), call volume from Loki `{job="chorus-api"}` over 7 days (2026-06-30 → 2026-07-07). 64 mutation endpoints total.

The point of this inventory: split the 64 into **flip-now** (zero live callers → secure with no credentialing, like #3618's set) and **credential-then-flip** (live callers must carry a token before their gate flips, deploy-before-require). The traffic is the truth-teller.

## Prereq — step 0 (blocks the ALLOW path on every class)
`chorus-api-wrapper.sh` does NOT source `~/.chorus/secrets/chorus-realm.env`, so chorus-api has no `CHORUS_SERVICE_TOKEN_SECRET`. The envelope's REFUSE path (401) works without it (proven live #3618), but the ALLOW path (scoped-token→200) cannot verify until the wrapper sources the realm secret (reference, not value — same pattern as owl-api-launch.sh). **Do this first**, else no credentialed caller can pass any flipped gate.

## CLASS A — zero-caller (flip-now, no credentialing) — ~34 endpoints
Loki 7d = 0 hits. Securing these breaks nothing (same profile as #3618's reindex/discover-pages/discover-endpoints). Verify each 0 before flip (a caller could appear).

| Family | Endpoints | 7d hits |
|---|---|---|
| cards/* | add, done, move, set, tag, view | 0 (CLI writes Vikunja direct; #2544 HTTP wrapper unused) |
| chorus/alert | 1 | 0 |
| chorus/pulse | 1 | 0 |
| chorus/role-state | 1 | 0 |
| chorus/open | 1 | 0 |
| chorus/catalog/* | tags, lineage | 0 |
| chorus/voice/:role | 1 | 0 |
| icd/* | domains/:id/{fields,mappings,providers...} | 0 |
| chorus/embed | 1 | 2 (nightly test only — near-zero; batch with credentialing) |
| chorus/reindex | 1 | 3 — ALREADY SECURED (#3618) |

**Caveat to verify before flipping cards/*:** 0 HTTP hits is suspicious for a live board — confirm the `cards` CLI truly writes Vikunja directly and nothing round-trips `/api/cards/*`. If a hidden consumer exists, it moves to Class B.

## CLASS B — busy, credential-then-flip (one caller at a time) — the real work
| Endpoint | 7d hits | Known caller | Credentialing |
|---|---|---|---|
| chorus/index | 13,638 | reindex-worker's index-worker.js (pokes embed-delta) | mint+send token in index-worker; highest stakes — do last, most careful |
| athena/subdomains/* (~24 eps) | 1,366 | Athena UI writes + discover jobs | UI needs a browser-obtained token OR these stay session-gated; discover jobs credentialed |
| athena/discover-code, discover-tests | 459 | discovery scheduler | credential the scheduler |
| athena/validate | 204 | validation job/UI | identify + credential |
| athena/reload | 36 | GSP bulk-load path | needs the governed bulk-load primitive (#3573 named it), not just a token |

## Flip order (deploy-before-require)
1. **Step 0:** wrapper sources realm secret → ALLOW path works.
2. **Class A wave:** verify-zero → secure all ~34 zero-caller endpoints in one model edit + flip. Big win, no breakage. (cards/* pending the round-trip check.)
3. **Class B, ascending traffic:** reload(36) → validate(204) → discover(459) → subdomains(1366) → index(13638). Each: credential caller live → flip that surface → verify caller green → retire open path with receipt.
4. **Done gate (AC):** automated sweep test asserts zero unauthenticated mutation endpoints.

## Done-state definition
Every one of the 64 either secured (securedBy in the model, gate live) or explicitly exempted with a recorded reason. The sweep test is the truth-teller.
