# Nightly writeback stores ZERO — the nightly principal no longer resolves a role

**From:** Kade, 2026-08-28 03:23 (diagnosed live, evidence fresh)
**Severity:** P1 — the first nightly under the #4015 runner executed 4,243 tests and stored none of them. The page says so honestly (`RESULTS LOST`, stored 0 of 4,229 — the reporting works; the storage authz broke).

## The receipt

Minting as the nightly principal works (644-char token), but the write door refuses it:

```
POST :3360/testresults/batch  (Bearer <nightly token>)
→ HTTP 403 { "error": "authz-role", "message": "writes require a model-resolved role" }
```

## Read

The door resolves the caller's role FROM THE MODEL, and `nightly` resolves to none. Yesterday's model deploys touched the roles/domains area (your #3860 chain + the pulse-domain removal my tagger tripped on), and the holdsRole seam moved out of the security graph into the roles domain (Jeff's cut, 07-24). Most likely the nightly principal lost (or never gained) its role edge in the deployed roles model.

## Ask

- If the roles model is yours to patch: give the nightly principal its model-resolved role (least-privilege, tests-graph writes only, per #3975).
- If the resolution logic is the door's (athena-make): say so and I take it — identity/verify is my side of the seam.

Yesterday's card-scoped writes as `kade` worked (218/218 at 16:38, 2,000 at 22:27), so the door itself is fine — it's specifically the nightly principal's role resolution.

## Context

Tonight was otherwise the best nightly in weeks: 1 red total (Silas's), 20m41s elapsed, and the #4015 runner's loss-alarm did exactly its job — this defect would have been invisible a week ago.
