# ADR-052: Identity via Solid-OIDC (CSS) — asymmetric service + human auth verified at the owl-api door, retiring HS256 + self-declared DEPLOY_ROLE

**Status:** Proposed
**Date:** 2026-07-14
**Author:** Silas
**Card:** #3613 (implements) · grounded by spike #3643 (landed 2026-07-14, proven live)
**Extends:** ADR-042 §7 (commits the asymmetric upgrade as the open-source gate — this ADR *names the mechanism*) · ADR-048 §5 (owl-api is the only writer to the graphs — this ADR is the authN at that one seam)
**Decides for:** #3611 (writer migration), #3612 (scope enforcement rides on this), #3614 (other-store doors reuse the pattern)

## Context

ADR-042 landed model-driven auth and, in §7, made a specific promise it did not yet keep:

> "Asymmetric keys ⇐ open-sourcing. `SERVICE_TOKEN_SECRET` is HS256 — anyone who can verify can mint. Fine for one box on a LAN; wrong when every generated API verifies in the wild. ADR commits the asymmetric upgrade (signer holds private key; generated APIs verify with public) as the open-source gate."

That commitment sat with the mechanism unspecified. Two facts make it urgent now:

1. **Identity today is forgeable.** An agent proves who it is by setting `DEPLOY_ROLE=silas` in its own shell — a stamp anyone can wear (#3356). The write path, spine attribution, and object-level authz (ADR-048 §2 `ownedBy`) all key on that self-declared string. The audit trail's "silas wrote this" is an honor-system claim by the thing being audited.
2. **Two auth planes.** chorus verifies HS256 shared-secret JWTs (`CHORUS_SERVICE_TOKEN_SECRET`); gathering already runs Solid-OIDC. `SOLID-AUTHENTICATION.md` flags that gathering even decodes the CSS JWT **without verifying its signature** ("local trust — CSS is our own container"). Two planes, one of them not actually verifying.

The open question #3643 was pulled to answer: **build a custom asymmetric signer to satisfy ADR-042 §7, or reuse the Solid-OIDC issuer we already run?**

## Decision

**Adopt Solid-OIDC / CSS as the single identity issuer. Reuse the standard; do not roll our own** (`chorus:principle-no-competing-implementations`).

We already run CSS (`com.gathering.css`, :3001) as the OIDC issuer + pod host. It **is** the asymmetric signer ADR-042 §7 asked for:

- **Signer holds the private key** — CSS signs with ES256; the private key never leaves CSS.
- **Verifiers hold the public key** — published at `/.oidc/jwks` (`kty=EC, alg=ES256, crv=P-256`). Anyone verifies; only CSS mints. This is precisely §7's "signer holds private key; generated APIs verify with public."

Six parts:

### 1. One issuer, two grants
- **Services** (agents, daemons, the 6am cron tier) mint headless via `client_credentials` → an ES256 token carrying a `webid` claim. No browser. **Proven live in #3643.**
- **Humans** (Jeff, Mark, future users) mint via the browser OIDC grant on the **same** issuer. Same WebID primitive. This is the human-login half — a different grant on one door, not a separate system.

### 2. Verification at the one seam
owl-api verifies the token at its single write seam (ADR-048 §5) — `jwtVerify` against the CSS JWKS (public-key), extract the `webid`, then feed the **existing** model-driven authz (ADR-048 scopes + `ownedBy`). During migration it verifies HS256 **and** ES256 side-by-side (§8); at rollout end it **replaces** the HS256 `mod auth` check. The spike's `gatedWrite` is the reference implementation.

**JWKS caching (the decision Wren flagged): fetch-with-TTL + kid-keyed cache, fail-closed on no usable key.** Use `createRemoteJWKSet` semantics — cache keys, cooldown between refetches, refetch on an unknown `kid` — *not* a one-shot cache-at-boot (that can't pick up key rotation). Two hardenings: (a) a **boot warm-fetch** so CSS-down-at-boot surfaces as a loud warning early, but does **not** block boot; (b) when the token's `kid` is already cached, serve it **without** a live refetch, so a brief CSS blip does not fail otherwise-valid writes. **Fail-closed** only when the token's `kid` has no cached key *and* CSS is unreachable — a genuinely unverifiable token, never a transient outage masquerading as one. This is the `JWKS-unreachable` case in the spec.

### 3. WebID is the actor, everywhere — seam stays thin (pass-through)
Door authz **and** spine attribution derive the actor from the **verified WebID**, never from `DEPLOY_ROLE`. `DEPLOY_ROLE` is demoted to display-only, then removed (#3356 closed). "silas wrote this" becomes cryptographically true: a key only silas holds signed it.

**The seam does authN only — it passes the verified WebID through; authz owns the WebID→role / `acts-as` resolution** (Wren's call, and the right one). The seam's single responsibility is "prove this token and yield its WebID." Mapping that WebID to a role, honoring the `acts-as` edge, and applying scope all live in the authz layer (ADR-048), which already owns that model. Keeping the seam thin means the crypto boundary has one job and the policy boundary has the other — they evolve independently.

### 4. Fail-closed, no fallback
An absent / malformed / expired / wrong-issuer / wrong-audience / signature-invalid token → **401 + a spine event**. There is no fallback to `DEPLOY_ROLE`, no local-trust decode, no log-and-continue. The #3643 negative control (a one-byte-tampered token → `JWS signature verification failed` → refused, no write) is the required test, promoted from manual script to the automated suite. Named cases (full spec: `roles/silas/adr/ADR-052-test-spec.md`): **valid-allows · forged-401 · expired-401 · wrong-issuer-401 · wrong-audience-401 · JWKS-unreachable→fail-closed-when-no-cached-key.** The audience check is not optional: a token minted for another service (`aud≠chorus`) is refused even if its signature is valid — that is ADR-042's `aud` "belt over the separate-secret suspenders," now the primary boundary.

### 5. Per-agent credentials, model-resolved registry — **reuses the #3618 identity model**
The registry is **not new** — #3618 already authored it and the classes are **live in `urn:chorus:ontology`**: `chorus:Principal` (an authenticatable actor that `holdsRole` and `ownsCredential`), `chorus:KeyRegistryEntry` (whose own definition reads *"the model-resolved successor to the phase-1 static webId list in auth.rs; #3618 models it, **#3613 makes owl-api resolve from it**"*), plus `chorus:webId`, `chorus:keyId`, `chorus:permittedScope`, `chorus:forPrincipal`. This ADR does **not** introduce a parallel `Agent` class — that would be a competing implementation of `Principal`.

**What is actually missing (verified live 2026-07-14): the instances.** `urn:chorus:domains:security` holds **zero** `Principal` and **zero** `KeyRegistryEntry` today — #3618 deployed the vocabulary and left the registry data unwritten. So #3613's model leg is: (a) author the `Principal` + `KeyRegistryEntry` **instances** for the agent WebIDs (`http://localhost:3000/pods/chorus/_agents/{silas,wren,kade,bridge}/profile/card.ttl#me` — the live path in `auth.rs` / `service-token.ts`), (b) land them through the model-deploy path (mind the known instances-don't-deploy gap), (c) Wren's seam resolves the allow-set from `KeyRegistryEntry` instead of the hardcoded `chorus_agent_webids()`. Each agent gets its **own** CSS credential, **not settable from its own shell env**; adding/revoking an agent becomes a model edit (`chorus:agentStatus`-style flip → regen → drops from the allow-set within one token TTL).

**Reconciliation — decided with Wren, 2026-07-14 (don't overload `keyId`):** Under Solid-OIDC there is **no per-principal key material in the registry at all** — the verify key comes from JWKS by the token header's `kid`, and the issuer is **deployment config, not per-principal data**. So `keyId` is *not* repurposed: it keeps its HS256 meaning, becomes **optional**, appears only on legacy dual-verify entries, and **dies with HS256 at cutover** (test case 9's deletion asserts it). One field, one meaning, then gone — no overloaded semantics to drift.

**The allow-set is `Principal.webId` alone.** Scope stays a per-request **token claim** validated at the seam (the existing #3573 check), *not* registry data — putting scope in the registry would create a second authz source that drifts against the claim. Registry-side scope *ceilings* (max grantable per principal), if ever wanted, are an authz-layer feature on top, not the seam's membership check. **Consequence: the CSS path needs only the `Principal` instances** (authored + `sh:conforms`-validated, `identity-principals-3613.ttl`); `KeyRegistryEntry` is legacy-HS256-only and retires with HS256. The seam resolves membership with `SELECT ?webid WHERE { GRAPH <urn:chorus:domains:security> { ?p a chorus:Principal ; chorus:webId ?webid } }`, fail-closed on absent (parity with `auth.rs:97`).

**Honest scope of a CSS-token write (decided with Wren, 2026-07-14):** a CSS `client_credentials` token carries **no scope claim**, so it proves **membership-gated *entity* writes** — a POST that lands in a class's model-declared `instancesGraph` via chorus-model, authorized by allow-set membership. It does **not** pass the scoped `/batch` path (`x-target-graph`), which requires a scope claim the CSS token doesn't have — that path stays legacy-scoped-token territory during migration. Mapping a CSS token to a `/batch` scope is a **named future concern**, not this card's: case-1's proof is a real governed entity write, which is the stronger claim anyway.

### 6. Credential storage convention (where per-agent CSS creds live)
Follows the **#2734 single-home pattern** (Wren's ask, so her 403 isolation tests read the path rather than guess it): each agent's CSS client credential lives in a **per-agent home under `~/.chorus`** — `~/.chorus/identity/<agent>/cred.json` (mode 0600, owner-only) — **never in the repo, never as a value in a launchd plist** (plists carry a *reference*/path, not the secret; same boundary that broke TCC when secrets churned the cdhash). The value is the CSS `client_id`+`client_secret` for that agent's WebID; the model (`Principal` / any future `credentialRef`) names only the path, never the value (write_scrubber boundary).

**"Not settable from the agent's own shell env" (the AC), concretely:** the credential is provisioned out-of-band into `~/.chorus/identity/<agent>/` by an admin step (`seed-css.sh`), readable by that agent's process but **not** something the agent mints for itself or exports as a forgeable string — an agent cannot promote itself by writing another agent's cred file (owner-only perms) or by setting an env var the door trusts (the door trusts the CSS-signed token, not the env).

**Scoped for cross-agent isolation:** each agent's credential authenticates **only** its own WebID at CSS, so a token minted with wren's credential carries `webid=wren`, and the seam's authz refuses it acting as silas. This is exactly what Wren's 403 tests assert (`wren's token can't act as silas`) — the isolation is a property of *distinct per-agent credentials*, not a check the seam adds. One credential, one WebID, one identity.

**Identity topology (decided 2026-07-14, SA + PM): one pod per agent under Jeff's CSS account.** `/pods/silas/`, `/pods/wren/`, `/pods/kade/`, `/pods/bridge/`, `/pods/chorus-sdk/` — each a distinct pod → distinct WebID + own storage. All under Jeff's account so **Jeff stays root authority**: provisioning is an admin act (agents can't self-mint), and the shape is symmetric with the named futures — a second human is their own pod, a second Chorus instance is its agents' pods (meshable). Account-per-agent was rejected: its 5-accounts / 5-recovery-paths weight buys isolation beyond distinct-WebID + distinct-CC that we don't need.

**The WebID's source of truth is CSS — the model records it, never invents it (Wren's line).** Grounding on 2026-07-14 found that the `/pods/chorus/_agents/<agent>/profile/card.ttl#me` strings in `auth.rs`, `service-token.ts`, and the draft `Principal` instances were **aspirational** — no CSS resource backed them; jeff's account controlled exactly one WebID. CSS-created pods yield `/pods/<name>/profile/card#me` (no `_agents/`, no `.ttl`). So when the pods are created, the **real** CSS-assigned WebIDs replace the aspirational strings in all three places (model `Principal.webId`, `auth.rs`, the live runner's `expected-webid`). The model chases reality; reality is never bent to the model. Wren's runner already guards this: it asserts `minted-webid == the agent's own WebID` and fails loudly otherwise, so a regression names itself instead of passing the 403 test vacuously.

### 7. Hardening path (named, not yet required)
CSS supports **DPoP** (proof-of-possession — the token bound to a holder key, so a stolen bearer token is useless). The spike used plain bearer. DPoP binding is the hardening follow-on; this ADR designs for it (the verify seam can require the `cnf` claim) without blocking the bearer-first cut.

### 8. Migration posture — dual-verify side-by-side, not a cutover
The seam verifies **HS256 and ES256 concurrently** through the rollout, selected behind the registry — an incoming token is tried against the CSS JWKS (ES256) *and* the legacy shared-secret (HS256); either valid identity is accepted. This is ADR-042 §60's "token-always clients, mixed state invisible to operations" pattern applied to the *upgrade*: the ~19 shared-secret writers do not all break the instant ES256 lands. **HS256 retires per-writer as #3611 migrates each to a CSS credential**, and the legacy verify path is deleted only when the last writer has moved (a tracked, asserted count — not a guess). No flag-day. **No interim weakening** (Wren's flag): until ES256 is proven, every write still has a working HS256 path; any owl-api deploy before the seam leg lands keeps riding HS256 unchanged.

### 9. Sign-in method for humans — passkeys, orthogonal to verification
For the human browser grant, the sign-in method at CSS is **passkeys / WebAuthn** (Mark's proposal, Jeff+Mark session 2026-07-14): no shared password, phishing-resistant, device-bound. Architecturally this is **orthogonal to the owl-api seam** — *how* a human authenticates to CSS (passkey, password, whatever CSS supports) changes nothing downstream, because the seam only ever sees and verifies the resulting ES256/WebID token. That separation is the point: the sign-in method can evolve (passkeys today, something else later) without touching the verification boundary. Passkeys are a **CSS-side configuration**, recorded here so the human-login card set (Wren, filing today) and the ADR agree on the method.

## Consequences

- **Closes ADR-042 §7's open-source gate.** The HS256 "anyone who can verify can mint" weak point is retired — CSS holds the only minting key.
- **Closes the gathering gap.** `SOLID-AUTHENTICATION.md`'s unsigned-decode ("local trust") becomes real public-key verification, shared with the agent path.
- **One identity plane for services and humans.** The two-plane split (HS256 + Solid-OIDC) collapses to one issuer, one verify seam, one WebID primitive.
- **Accountability becomes structural.** The audit trail is cryptographically true (the WHO). Jeff stops being the integrity check; the system is.
- **Identity before scope.** This ADR delivers unforgeable **WHO**. ADR-048 scope enforcement (#3612) answers **WHAT-per-who** and is only meaningful once this lands — enforcing "only silas writes X" against a forgeable identity is theater. #3613 is the gating build; #3612's annotation + validation work overlaps and switches to live enforcement the moment this is in.
- **CSS becomes load-bearing for writes.** The owl-api door now depends on CSS being reachable and the JWKS fetchable. Ops consequence: CSS gets a health check on the write-critical path, and owl-api caches the JWKS (with a bounded refresh) so a CSS blip doesn't fail every write. This is a new availability edge — name it in the threat model (#2444) and the monitor set.
- **Migration cost is real.** ~19 shared-secret writers move to per-agent CSS credentials (#3611 UNTANGLE) — the bulk of the labor. The door change is small; the writer migration is the grind.
- **Bearer-first is a known, time-boxed weakness.** Plain bearer tokens are stealable in transit; loopback-only + short TTL bound it now, DPoP (§7) closes it. Not pretended away — sequenced.

## Provenance

Spike #3643 proved the full chain live against the locked store: headless CSS `client_credentials` token → JWKS public-key verify → WebID → gated real write; forged token refused by signature check; the store itself confirmed only the authorized write landed. Reproducible: `platform/scripts/spike-3643-solid-oidc-showme.mjs` (on main). Decision to adopt taken with Jeff + Mark, 2026-07-14.
