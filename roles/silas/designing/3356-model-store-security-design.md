# #3356 — Model-store security: the auth-model design (AC1, Jeff reviews before build)

**Spike-before-building.** 154-file / 19-writer blast radius; a stray process nearly DROP'd the whole model (2026-06-11). This is the migration strategy, grounded in the 2026-07-24 validated audit — not a rewrite.

## Current state (VERIFIED-LIVE today, not assumed)

| Layer | Today | Evidence |
|---|---|---|
| **owl-api door** | CSS token REQUIRED on writes (ES256/JWKS) | probed: `POST /schema/domain` no-token → **401**, garbage → **401**, GET → 200 |
| **DAL** (chorus-model) | `DEPLOY_ROLE` env → membership ASK (Principal exists?). **Not authentication.** | probed: `DEPLOY_ROLE=noone` → fail-closed refusal; but `DEPLOY_ROLE=wren` from *any* process passes |
| **Fuseki :3030** | shiro single shared admin, `writer=*`, **no per-graph ACL** | probed: anon `/pods/update` → 401, anon query → 200 |

**The gap in one line:** owl-api already verifies a real CSS token; the DAL — the thing that actually writes Fuseki — verifies an env string; Fuseki underneath trusts one shared password. The identity infrastructure exists (#3613 OidcVerifier, #3669 CSS login); the DAL and store don't consume it.

## Target

- DAL verifies a **CSS token** (reusing owl-api's `OidcVerifier` — one verify path, not a second implementation) and stamps `dcterms:creator` from the *verified* WebID. Forging a writer then requires a credential, not `export DEPLOY_ROLE=`.
- **Per-graph ACL:** `urn:chorus:ontology` writable by the DBA path only; `urn:chorus:instances` (+ domain graphs) by the DAL only. A stray `DROP GRAPH` is refused.
- `holdsRole` derivation swaps parser → graph query (ADR-054 move 3) — the DAL/owl-api resolve role from the edge, not the WebID string.

## Migration — NON-BREAKING, dual-path (the crux; this is why it's a design pass)

The 19 writers all pass `DEPLOY_ROLE` today. We cannot flip in one commit. Same shape as the #3669 token-then-flag and the shiro-spike DAL-first plan:

1. **DAL accepts a token as an ALTERNATIVE to `DEPLOY_ROLE`** (additive, zero breakage). `verify_identity` gains a token path: a `CHORUS_IDENTITY_TOKEN` (or Authorization header for the HTTP DAL) verified via `OidcVerifier` → the WebID *is* the identity. Absent token → current `DEPLOY_ROLE` path unchanged. Both live at once. **← the first buildable slice, fully testable, breaks nothing.**
2. **Migrate the 19 writers** to mint + pass a token (each already holds `~/.chorus/identity/<agent>/cred.json` → client-credentials token). One writer at a time, verified.
3. **Flip:** `DEPLOY_ROLE` path refused, token required — the `REQUIRE_DPOP=1`-style finish line. Env-var forgery dies.
4. **Fuseki per-graph ACL** (the #3564 lock leg): shiro/authz so the DAL credential writes only instances graphs, the DBA credential only ontology. Closes the priv-esc loop (any writer rewriting SECURITY_GRAPH / ONTOLOGY_GRAPH).

Steps 1–2 are reversible and non-breaking; step 3 is the one gated flip; step 4 is store-level and independent.

## First slice (what I build after your go)

**Step 1 only, TDD:** `verify_identity` gains the token path (verify via OidcVerifier → WebID → Principal), `DEPLOY_ROLE` untouched as fallback. Red-first: a valid CSS token authenticates without `DEPLOY_ROLE`; a forged/garbage token refuses; `DEPLOY_ROLE` still works (no regression). No writer migration, no flip — just the door learning to accept the real key alongside the old one.

## Open for your steer
- **Token transport to the DAL:** env var (`CHORUS_IDENTITY_TOKEN`, matches the CLI shape) vs the DAL going HTTP-only behind owl-api's already-verified door (collapses two doors into one — arguably cleaner, bigger change). My lean: env-var token for step 1 (smallest slice), converge on the HTTP door later.
- **Dependency #3355** (shared-security exists) — the audit confirms the substrate is live (OidcVerifier + allow-set + CSS), so I read #3355 as satisfied; confirm.
