# athena-gen: what Jeff said it builds vs what it builds (#4742, AC1)

Measured 2026-10-25 18:2x–18:4x (Silas) on werk silas-4742 (origin/main at pull).
"Measured" = run or read in code today. "Inferred" = not yet run.

| # | Jeff's words | What athena-gen builds today | Gap |
|---|---|---|---|
| 1 | 09-18 "we generate owl and run athena-* to make an api" | `athena-gen generate --product pulse` regenerates `directing/products/pulse/api` byte-identical to what is committed (only the chorus-oidc path differs by output dir). Measured. | **Only one product is generated (pulse).** Every other domain is served by the generic door (athena-make), not by a generated API. |
| 2 | 10-10 "generated apis must run without fuseki" | Store kind `embedded` (oxigraph 0.5.11, on disk or in memory), beside `sparql` and `door`; `runs_own_server_4576` test. Measured in code. | None found. |
| 3 | 10-23 crawl: one door for writes | Store kind `door` (#4716): reads SPARQL, writes through the door with a service token. Measured in code. | **Live pulse-api still writes the old way:** its installed launcher (10-10) lacks `PULSE_STORE_KIND=door`; the install script from #4716 was never run. Measured 06:59. |
| 4 | card: "every refusal named" | Every refusal is `error(status, kind, message)` (`bad-name`, `no-name`, `exists`, `not-found`, `bad-value`, `unknown-route`, …). Measured. | None found. |
| 5 | 10-23 #4713 "the door can't write a boolean" | Generated store types each literal from the field's datatype (`term()`); the door was fixed in #4717. Measured. | #4713 card still sits in Later with its after-land re-PATCH of 14 platform rows not done (board state). |
| 6 | 10-23 #4709 "facts with no verb to remove them" | Generated API: PATCH with `null` removes a field; DELETE removes a row. Measured. | None in the generator. |
| 7 | 10-24 #4717 lesson (TDB2 cannot delete a decimal spelled "7") | The door writes decimals canonical (#4717). The generated store's `term()` writes the value as given. Measured in code. | **A generated API on `sparql` kind would stack decimal values exactly as the door did before #4717.** |
| 8 | 10-23 #4708 "a shape change 404'd /v1/domains" | That route loss happened in the door (athena-make route derivation from shapes). athena-gen builds routes from model classes; no test proves a model change cannot silently drop a class's route. Inferred. | **No guard: a class that falls out of the model drops its generated route without a red test.** |
| 9 | 10-23 #4702 "retired facts re-added" | That was athena-deploy re-adding source facts, not athena-gen. | Not athena-gen's gap. |

| 10 | 10-25 18:27 "the top level route is /pulse and each associated domain is a child route" | Routes are `/v1/<domain>/<collection>/<name>`: the version is the top level, the product is not in the path. Generated domains: `awareness` (heartbeats, notes, prompts, readings, rolestates, turns, waitingons) and `streams` (events). Measured. | **Top level is `/v1`, not `/pulse`.** Messages and cards (pulse's radius, Jeff 10-22) are not generated at all: they stay hand-built in platform/pulse (TS). |

## Fixed in the generator (werk silas-4742)
| Row | Fix | Proof |
|---|---|---|
| 10 | Routes are `/<product>/<domain>/<collection>` (c0ddc2e4) | `/v1/...` answers unknown-route; red when the old route is put back |
| 1 | A product's child routes are the domains it owns **and composes** (product-radius); composed = read-only from the owner's graph, writes 405 `composed` (9f9d0443). Pulse now serves awareness, streams, alerts, cards, messages (15 routes). | compose_4742 (3), requirements_tests composed write |
| 7 | Decimals written canonical, non-decimals refused (1919c719) | red on the old generator, green on the new |
| 8 | generate refuses to drop a route the crate serves unless named `--drop <route>` (24ea510d) | route_drop_4742 (4) + live check on a scratch copy |
| 3 | Repo launcher already defaults `PULSE_STORE_KIND=door`; the installed one (10-10) predates it. Resolves when the land's deploy-prod installs the launcher. | live check after land |

Open from Jeff 18:49: query and stream share one filter (actor × domain × time, `follow`); the generated API pages only (`limit`/`offset`).

## Open question for Jeff
"athena-gen is not building what i think it is" — rows 1, 3, 7, 8, 10. Jeff answered 18:27 with the route shape: `/pulse/<domain>/…`, each domain in the product's radius a child route.

## Live receipt (2026-10-25 19:3x, werk build on :3492, door kind, prod graph, read-only)
```
/pulse/awareness/rolestates  rows=1 first=wren
/pulse/streams/events        rows=0
/pulse/cards/cards           rows=3 first=3325
/pulse/messages/messages     rows=3 first=src-40138
/pulse/alerts/alerts         rows=3 first=security-network-exposed
/v1/awareness/rolestates     404
pulse-api 1.0.0 serving 15 classes on 127.0.0.1:3492
```
