# Brief: Auth & Session Bugs — Loki Analysis

**Date**: 2026-02-20
**From**: Silas (Architect)
**To**: Kade (Engineer)
**Priority**: P1 — Jeff is hitting this in production right now
**Trigger**: Jeff reported being forced to re-login when navigating between docs in About

---

## Bug 1: SESSION_SECRET not set (P1 — root cause of session loss)

**Evidence** (Loki):
```
{"level":"warn","message":"Set SESSION_SECRET environment variable for production use"}
```

**Impact**: Without a stable `SESSION_SECRET`, Express generates a random secret on each container start. Every restart invalidates all active sessions. Jeff was authenticated at 19:25:31, lost session by 19:26:05 (34 seconds later).

**Fix**: Add `SESSION_SECRET` to `.env` with a stable random value. Pass it through Docker env in Terraform. This is a one-line fix that prevents session loss on restarts.

```bash
# Generate a secret
openssl rand -hex 32
# Add to .env
SESSION_SECRET=<generated-value>
```

Make sure Terraform/Docker passes it: `environment = ["SESSION_SECRET=${var.session_secret}"]` or via env_file.

---

## Bug 2: SOLID OIDC auth fails, Pivot fallback saves it (P2)

**Evidence** (Loki, happens on EVERY login):
```
19:25:23 "Error handling redirect" error: {}
19:25:23 "Error handling incoming redirect" error: {}
19:25:23 "Session info after callback" hasSessionInfo: false, hasWebId: false
19:25:23 "Detected Pivot authentication parameters in callback"
19:25:23 "Successfully authenticated user with Pivot"
```

**Impact**: The standard SOLID OIDC flow (`handleIncomingRedirect`) fails every time — empty error object, no session info. A "Pivot" fallback mechanism catches the failure and authenticates by extracting the `iss` parameter from the callback URL. This is why Jeff feels like there are "more than 1 flow" — there ARE two flows, and the first one always fails.

**Fix**:
1. First: log the actual error instead of swallowing it (`"error":{}` means the error object is being serialized empty — try `JSON.stringify(error, Object.getOwnPropertyNames(error))`)
2. Then: debug why `handleIncomingRedirect` returns `hasSessionInfo: false`. Likely causes:
   - Session storage mismatch between redirect start and callback
   - CORS or cookie issue with solidcommunity.net
   - The `SESSION_SECRET` issue (Bug 1) may be causing this too — fix Bug 1 first and retest

---

## Bug 3: Chorus activity API polling without auth (P3)

**Evidence** (Loki, every ~60 seconds):
```
"Unauthenticated user attempting to access route" path: "/api/chorus/activity"
```

**Impact**: The Chorus page's JavaScript is fetching `/api/chorus/activity` without passing credentials. It works when you're logged in (session cookie sent automatically), but fails on page load before auth completes, and generates noisy log entries.

**Fix**: Add `credentials: 'same-origin'` to the fetch call in the Chorus page JavaScript, and/or make `/api/chorus/activity` return a 401 cleanly instead of logging "unauthenticated user attempting" on every poll.

---

## Suggested Order

1. **Bug 1 first** (SESSION_SECRET) — may fix Bug 2 as a side effect
2. Retest login flow after Bug 1
3. If Bug 2 persists, add proper error logging and debug
4. Bug 3 whenever convenient

---

— Silas
