# ADR-052 Test Spec — owl-api CSS/JWKS verify seam (#3613)

Named cases for the owl-api write-seam identity verification. Wren implements the seam against these; each is an automated suite unit (not a manual script — that gap is what #3643 landing exposed). Behavior is stated as Given / When / Then against the **live** CSS issuer + locked store, mirroring the #3643 show-me.

**Fixtures**
- `TOKEN_VALID` — ES256/WebID token minted from CSS via `client_credentials`, `aud=chorus`, unexpired, known `kid`.
- `TOKEN_FORGED` — `TOKEN_VALID` with the signature byte-tampered.
- `TOKEN_EXPIRED` — validly signed, `exp` in the past.
- `TOKEN_WRONG_ISS` — validly signed by a non-CSS issuer.
- `TOKEN_WRONG_AUD` — CSS-signed, valid signature, `aud=some-other-service`.
- `TOKEN_HS256` — legacy shared-secret JWT (the current scheme), valid.
- Assertion surface: HTTP status, whether the triple landed in the store (authed ASK), and the emitted spine event.

---

### 1. `valid-allows`
- **Given** `TOKEN_VALID` for `webid=…/agents/silas/…#me`
- **When** POST a write to the owl-api seam
- **Then** `2xx`; the triple **lands** in the store; a spine event attributes the write to **the WebID** (not `DEPLOY_ROLE`).

### 2. `forged-401`
- **Given** `TOKEN_FORGED`
- **When** POST a write
- **Then** `401`; **no** write lands; a `security.*` spine event records the refusal with reason `signature-invalid`. (The #3643 negative control, promoted to a unit.)

### 3. `expired-401`
- **Given** `TOKEN_EXPIRED`
- **When** POST a write
- **Then** `401`; no write; reason `expired`.

### 4. `wrong-issuer-401`
- **Given** `TOKEN_WRONG_ISS`
- **When** POST a write
- **Then** `401`; no write; reason `issuer-mismatch`. (Signature may be valid *for its own issuer* — rejected because the issuer isn't CSS.)

### 5. `wrong-audience-401`
- **Given** `TOKEN_WRONG_AUD` (valid CSS signature, wrong `aud`)
- **When** POST a write
- **Then** `401`; no write; reason `audience-mismatch`. **A valid signature is not sufficient** — a token minted for another service must not write to chorus. This is the case a naive "verify signature, extract webid" seam gets wrong.

### 6. `no-token-401`
- **Given** no `Authorization` header
- **When** POST a write
- **Then** `401`; no write; reason `no-token`. No anonymous fallback, no `DEPLOY_ROLE` read.

### 7. `jwks-unreachable-failclosed`
- **Given** CSS `/​.oidc/jwks` unreachable **and** the token's `kid` is **not** in cache
- **When** POST a write with `TOKEN_VALID`
- **Then** `401` (fail-closed); no write; reason `jwks-unreachable`. A verification we cannot perform is a verification that fails — never allow-on-error.

### 8. `jwks-blip-resilient` (the paired positive control)
- **Given** CSS momentarily unreachable **but** the token's `kid` **is** already cached (from a prior verify or the boot warm-fetch)
- **When** POST a write with `TOKEN_VALID`
- **Then** `2xx`; the write **lands**. A brief CSS blip must **not** fail otherwise-valid writes — that is why the posture is kid-keyed cache, not fail-on-any-CSS-error. (Cases 7 and 8 together define the exact fail-closed boundary: fail only when we have *no* usable key, not merely when CSS is briefly down.)

### 9. `hs256-legacy-allows` (migration only)
- **Given** `TOKEN_HS256` (a not-yet-migrated writer)
- **When** POST a write
- **Then** `2xx`; the write lands. Dual-verify accepts the legacy identity **during rollout** (§8). This case is **deleted** when #3611 migrates the last writer and HS256 verify is removed — its deletion is the assertion that the cutover completed.

### 10. `attribution-is-webid`
- **Given** a `valid-allows` write
- **When** inspect the spine event and any `wroteBy`/actor field
- **Then** the actor is the **verified WebID**, and `DEPLOY_ROLE` (if still present) does **not** influence it. Proves §3 pass-through: the seam yielded the WebID; nothing downstream trusts the env stamp.

### 11. `revocation-drill`
- **Given** a valid credential, then that credential **revoked** at CSS
- **When** the agent presents a token minted before revocation, after ≤ one token TTL
- **Then** `401` within one TTL; no write. Proves revocation is real (ADR-052 §5 / #3613 AC), not just registry cosmetics.

---

**Coverage note (the honest one):** cases 1–6, 10 are pure seam units and can run headless in the suite. Cases 7–8 (JWKS-unreachable / blip) need a controllable CSS-reachability fixture — a stub JWKS endpoint the test toggles, not the live CSS (don't flap the real issuer in CI). Case 11 (revocation) is an integration drill, likely a scripted live-CSS test gated separately from the fast unit suite. Name which tier each runs in so none silently degrades to "not actually exercised" — the exact `no-affected-units` green-on-nothing trap #3643 hit.
