# Brief: Login loop bug — Loki analysis

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Card:** #667
**Priority:** P1 — Jeff hit this today, it's a hassle

## Symptom

Jeff authenticates through Pivot successfully, lands on `/`, then navigates to `/harvest-manifests` and gets redirected to login. Loop.

## Loki Evidence (18:00:09–18:00:52 UTC today)

### Timeline

1. **18:00:09** — `POST /login` — OIDC discovery **times out** after 3500ms:
   ```
   RPError: outgoing request timed out after 3500ms
   ```
   First attempt fails silently (user sees login page again).

2. **18:00:22** — `POST /login` — Second attempt. Redirects to Pivot successfully.

3. **18:00:48.679** — `GET /callback` — Pivot returns auth code. App logs:
   ```
   "Successfully authenticated user with Pivot"
   webId: https://jeffbridwell.solidcommunity.net/profile/card#me
   userRole: admin
   ```

4. **18:00:48.687** — `GET /` — Redirect after callback. **Authenticated** (webId present).

5. **18:00:48.790–.939** — Static assets (CSS, JS, images, JSON). **All authenticated** (webId present on every request).

6. **18:00:52.855** — `GET /harvest-manifests` — **"Unauthenticated user attempting to access route"**. Redirected to `/login`.

7. **18:00:52.861** — `GET /login` — Shows webId as **authenticated** (public route middleware sees the session). Login page rendered anyway.

### The Bug

At 18:00:52, **two middlewares disagree on auth state in the same second**:
- Public route middleware: sees webId, logs "Authenticated user accessing public route" for `/login`
- Admin middleware on `/harvest-manifests`: sees NO auth, logs "Unauthenticated user"

This is NOT session loss from a deploy — the session exists (static assets at 18:00:52.897 still show webId). The `adminMiddleware` is checking something different from the public route middleware, or the session data is partially written.

## Where to look

1. **`adminMiddleware` vs public route auth check** — are they reading the same session property? If adminMiddleware checks `req.session.webId` but the Pivot callback sets `req.session.user.webId` (or vice versa), public routes pass but admin routes fail.

2. **Session save race** — the Pivot callback at 18:00:48 sets session data and redirects. If `req.session.save()` hasn't flushed to SQLite before the redirect completes and the browser fires `/harvest-manifests`, the admin check reads stale session state.

3. **OIDC timeout** — the first login attempt timed out at 3500ms. Check if the OIDC client timeout is too aggressive. solidcommunity.net may be slow. Consider raising to 10s.

## Suggested Fix Priority

1. **Session save race** — most likely cause. Ensure `req.session.save(callback)` completes before sending the redirect response in the callback handler.
2. **adminMiddleware property check** — verify it reads the same field the Pivot callback writes.
3. **OIDC timeout** — raise from 3500ms to 10000ms to reduce first-attempt failures.
