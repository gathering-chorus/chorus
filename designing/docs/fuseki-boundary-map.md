# Fuseki Boundary Map — as-built (#3611, from the #3565 SEE spike)

**Status:** as-built, verified against the running system 2026-07-22 (Library).
**Program:** #3564 Govern + secure Fuseki — SEE (#3565, Done 6/22) → **UNTANGLE (#3611, this map)** → LOCK (#3630/#3579, landed) → GOVERN (#3612, CI ratchet).
**Invariants (Jeff, 6/22):** gathering has ZERO runtime dependency on chorus (#6); chorus's Fuseki framework must not live in or depend on gathering's tree (#7); reduce coupling, add none (#5).

Every claim below was verified against the live box (launchctl, the running Fuseki, live probes) — not code reading alone.

## The three buckets, as built

### 1. SHARED INFRA — the store both products stand on (owner: **Silas/ops**)

| Piece | Live location (verified) |
|---|---|
| Fuseki server 5.1.0 | `~/.gathering/data/fuseki-5.1.0/fuseki-server`, service `com.gathering.fuseki`, `:3030` |
| FUSEKI_BASE | `~/.gathering/data` |
| Dataset config | `~/.gathering/data/configuration/pods-simple.ttl` → `/pods` (query/sparql/update/data) |
| TDB2 store | `~/.gathering/data/fuseki-pods` (both products' graphs: `urn:chorus:*`, `urn:gathering:*`, `urn:jb:*`) |
| Auth | `~/.gathering/data/shiro.ini` (0600) — reads open, writes localhostFilter+authcBasic (#3630/#3579) |
| **Write credential** | `~/.gathering/data/fuseki-write.env` (0600) — **new this card**, provisioned by `fuseki-shiro-deploy.sh` |
| Logs | `~/Library/Logs/Gathering/fuseki.log` |

Canonical deploy sources live in the **chorus tree** (shared infra is ops-owned, and ops tooling is chorus):
`platform/scripts/launchagents-canonical/{com.gathering.fuseki.plist, fuseki-config-pods.ttl, fuseki-shiro.ini}`, `fuseki-shiro-deploy.sh`, `fuseki-backup.sh` + its plist.

The shared-infra pieces sit under `~/.gathering/data` for historical reasons; that is a **runtime home, not gathering's repo tree**. Renaming the service/dir to a product-neutral identity was considered and deferred — a live service identity migration buys no coupling reduction the declarations here don't already buy.

### 2. CHORUS — the framework that reads/writes the store

owl-api (`com.chorus.owl-api`, :3360) · chorus-api's ~14 writers via the two client factories (`fusekiWriteAuthFromEnv`, #3566) · shell write door `fuseki-auth.sh` (#3566/#3630) · chorus-hooks guards (`sparql_guard`, `icd_write_gate`) · model/shapes/DAL.

**As of this card, none of it reads gathering's repo tree.** The write credential comes only from the shared-infra home (or an explicit `FUSEKI_WRITE_ENV` override).

### 3. GATHERING — the product's own use of the store

The app's writers carry their own `FUSEKI_*` keys in its own `.env` (its copy, its repo) · pods surface (`:3000/pods` via app/CSS) · NiFi harvest writers (Bedroom) · its retired repo-local Fuseki config (`data/fuseki/configuration/pods-native.ttl`) is **not what runs** and must not be redeployed.

## Chorus→gathering dependency ledger and disposition

Enumeration method: the #3612 ratchet markers (`jeff-bridwell-personal-site`, `:3000`, `com.gathering.*`) over `platform/` non-test files — 102 files at card start, the Fuseki-relevant subset triaged below. Dispositions: **MOVED** (this card), **DECLARED** (legitimate, named owner), **RESIDUAL** (declared, pointered follow-on).

| Dependency | Evidence | Disposition |
|---|---|---|
| Write credential read from `jeff-bridwell-personal-site/.env` | `fuseki-auth.sh`, `owl-api-launch.sh`, `chorus-api-wrapper.sh` (Fuseki lane), `check-seeds.sh` (hand-rolled sed), `fuseki-shiro-deploy.sh` | **MOVED** → `$FUSEKI_BASE/fuseki-write.env`, provisioned by `fuseki-shiro-deploy.sh` (bootstrap only via explicit `GATHERING_APP_ENV`). Verified live in a clean env: cred loads from the new home, canary INSERT/DROP = 204/204, anon write = 401. `check-seeds.sh` now goes through the one door (its old sed matched USER *or* PASSWORD, file-order luck). |
| Canonical Fuseki plist pointed into gathering's repo (`--config=…/jeff-bridwell-personal-site/data/fuseki/configuration/pods-native.ttl`, homebrew binary, `/tmp` logs) | `launchagents-canonical/com.gathering.fuseki.plist` vs the live LaunchAgent | **MOVED/RECONCILED** — canonical re-grounded to the live service (FUSEKI_BASE discovery, no repo-tree config); live dataset config canonicalized as `fuseki-config-pods.ttl`. The drift was dangerous: deploying the old canonical would have repointed shared infra INTO gathering's checkout. |
| Ops/monitoring references to `com.gathering.*` names and `:3000` health | `agent-state.sh`, `app-state.sh`, `chorus-health`, `deep-health.sh`, `infra-alert.sh`, `nightly-suites.sh`, `perf-baseline.sh`, `service-registry.conf`, `chorus-ops.sh`, hooks `shim.rs` probe table, `ops.rs` labels | **DECLARED** — observation/lifecycle surface, ops-owned (DEC-022). Monitoring a product is not a runtime dependency of the framework; gathering serves fine if chorus's monitors vanish (membrane test). |
| ICD write gate hardcodes gathering's repo path | `icd_write_gate.rs:57` | **DECLARED** — the gate exists to govern writes INTO gathering's tree (DEC-095); the reference is its subject, not a dependency. |
| chorus-api sources the whole app `.env` for Twilio + Cost | `chorus-api-wrapper.sh` | **RESIDUAL** (non-Fuseki) — declared; the Fuseki lane no longer needs it (shared-infra file wins over same-named keys). Follow-on if we want full env independence. |
| Seeds tooling residuals | `seed-css.sh` (CSS account cred), `seed-probe.sh` (Twilio) | **RESIDUAL** (identity/comms, non-Fuseki) — candidates to migrate home to gathering per the convergence-boundary rule (#3599 pattern). |
| owl-api phase-1 static WebIDs at `:3000/pods/...` | `auth.rs chorus_agent_webids()` (+ tests) | **RESIDUAL** — pointer #3613/#3573 phase-2 (durable identity already mints `:3001` WebIDs; the static set swaps to model-projected). |
| ICD/convergence TTL load paths | Kade's #3392 (Next) | **RESIDUAL** — declared, owned, sequenced. |
| Non-Fuseki general coupling (docs tooling, dashboards links, product maps, …) | remaining ~60 ledger files | Outside #3611 scope — burns down under the #3612 ratchet (baseline 103; **100 after this card**, guard on `wren/3612`). |

## Membrane test (AC2) — gathering serves with chorus fully stopped

Script: `platform/scripts/test-product-membrane.sh` — stops all running `com.chorus.*` LaunchAgents, probes gathering as a user, restarts chorus under an EXIT trap.

**Live run 2026-07-22 11:30 (Jeff's go): MEMBRANE FAIL — and the failure is a real finding, not a test defect.**

| Probe (chorus down, 19 services stopped) | Result |
|---|---|
| `:3000/health`, `:3000/` (front door) | **DEAD** (no listener) |
| `:3002/health` (gathering app, direct) | 200 — the product process never blinked |
| Fuseki ping + `/pods` read | 200 / 200 |

**Root cause:** since #2122, `:3000` — gathering's front door — was **chorus's caddy edge proxy** (`com.chorus.caddy`): `/borg/*` and `/api/chorus/*` → chorus-api `:3340`, everything else → the gathering app at `:3002`. Stop chorus and the product's public surface disappeared even though the product itself was healthy. Invariant #6 violated in the running topology by the #2122 design decision.

**Fixed same day (Jeff's direction, 13:05): the edge is now gathering-owned.** `com.chorus.caddy` retired (plist archived); **`com.gathering.edge`** runs the same caddy routes from the runtime home `~/.gathering/data/caddy/Caddyfile` (canonical: `launchagents-canonical/{com.gathering.edge.plist, gathering-edge-Caddyfile}`; old repo-path Caddyfile removed). Chorus pages reach in via routes the product's edge GRANTS; with chorus down those routes 502 while the product serves — the correct failure direction.

**Re-run 2026-07-22 13:09 with the fix: MEMBRANE OK.** All five probes 200 with all 15 running chorus services stopped — front door, app-direct, and both Fuseki probes. AC2 holds as written.

**Restore incident from the first run (fixed in the script):** the restore left `com.chorus.hooks` unloaded (transient launchd bootstrap failure post-bootout) → brief team-wide fail-closed hook lockout (#2790), recovered by hand in ~1 min. The script now retries bootstrap 3× per service and NAMES anything still down — on the second run it caught `com.chorus.session-watcher` exactly as designed (restored by hand seconds later).

## Load-path verification (AC3)

Verified on the running server 2026-07-22: `/pods` serving (200s in fuseki.log), dataset resolved via `FUSEKI_BASE/configuration/pods-simple.ttl` → TDB2 `fuseki-pods` (38,074 photo triples counted post-crash the same morning), shiro at `~/.gathering/data/shiro.ini` (the `data/run/shiro.ini` decoy is dead), credential file present 0600. No running reference into `jeff-bridwell-personal-site/` remains on the Fuseki lane; the one dangling **canonical** reference (the stale plist) is fixed above.
