"""#4412 — a person's sign-in at the shared door is a Session row, written as them.

The guard is where Jeff actually signs in: the Clearing accepts the guard's
cookie and never runs its own callback, so a row written only there was never
written at all (0 rows for principal-jeff, measured 2026-10-10 17:20). The
door that signs him in writes the row, with his own sign-in token as the
bearer, so the store holds proof that he signed in, not a claim that he did.

Everything here takes its I/O as arguments, so the tests drive it without a
network. A failed write never blocks a sign-in: the row is a record of the
sign-in, and `chorus-principal login` refuses (naming the fix) without it.
"""
import hashlib
import json
import time
import urllib.parse



def role_sessions_query():
    """#4948 — the open role (pane) sessions his sign-in binds: no endedAt and no expiry
    filter. A role's session lasts from login to logout; its expiresAt is written once at
    login and never renewed, so an expiresAt.after filter (#4838) emptied binds an hour
    after each role's login (Silas, pair 4948)."""
    return "endedAt.absent=true&channel=pane&limit=5000"

def iso(secs):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(secs))


def slug(s):
    out, dash = [], False
    for ch in s.lower():
        if ch.isalnum():
            out.append(ch)
            dash = False
        elif not dash:
            out.append("-")
            dash = True
    return "".join(out).strip("-")


def token_id(jwt):
    """The token's jti, else a hash: the row names the token, never holds it."""
    try:
        part = jwt.split(".")[1]
        claims = json.loads(base64url_decode(part))
        if isinstance(claims.get("jti"), str) and claims["jti"]:
            return claims["jti"]
    except Exception:
        pass
    return hashlib.sha256(jwt.encode()).hexdigest()[:32]


def base64url_decode(part):
    import base64
    return base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)).decode()


def open_role_sessions(listing):
    """Names of the role Sessions open now, from a list reply in any envelope."""
    items = listing if isinstance(listing, list) else next(
        (listing.get(k) for k in ("items", "data", "rows") if isinstance(listing, dict) and isinstance(listing.get(k), list)), [])
    return [str(r["name"]) for r in items
            if isinstance(r, dict) and r.get("sessionState") == "open"
            and str(r.get("actsAs", "")).startswith("role-") and r.get("name")]


def session_row(principal, id_token, roles, now, lasts, name, host_account):
    row = {
        "name": name,
        "label": f"{principal} signed in at the shared door {iso(now)}",
        "ownedBy": principal,
        "actsAs": principal,
        "channel": "browser",
        "tokenId": token_id(id_token),
        "issuedAt": iso(now),
        "expiresAt": iso(now + lasts),
        "startedAt": iso(now),
        "lastSeenAt": iso(now),
        "sessionState": "open",
        # SessionShape requires it (minCount 1): the account the session runs
        # under. For a person it is their Principal's hostAccount (#4412: every
        # sign-in row was refused without it, 2026-10-10 23:15).
        "hostAccount": host_account,
    }
    if roles:
        row["binds"] = roles
    return row


def principal_query(webid):
    """The Principal that owns this WebID, by the model; never parsed from it."""
    w = webid.replace("\\", "").replace(">", "")
    return ("PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?p ?h WHERE { GRAPH ?g { "
            f"?p a chorus:Principal ; chorus:webId ?w }} FILTER(STR(?w) = \"{w}\") "
            "OPTIONAL { GRAPH ?g2 { ?p chorus:hostAccount ?h } } } LIMIT 1")


def principal_for(webid, sparql):
    """sparql(query) -> parsed JSON or None. Returns (bare name like 'jeff', hostAccount or None)."""
    body = sparql(principal_query(webid))
    try:
        b = body["results"]["bindings"][0]
        iri = b["p"]["value"]
    except Exception:
        return None
    name = iri.rsplit("#", 1)[-1].rsplit("/", 1)[-1]
    name = name[len("principal-"):] if name.startswith("principal-") else name
    return name, (b.get("h") or {}).get("value")


def open_session(webid, tok, now, lasts, sparql, http, api, log):
    """Write his open browser Session. http(method, url, bearer, body) -> (status, json).

    Returns the stored record (row name + tokens) for sign-out, or None.
    """
    found = principal_for(webid, sparql)
    if not found:
        log(f"person-session: no Principal owns {webid}; no Session row written")
        return None
    principal, host_account = found
    if not host_account:
        log(f"person-session: principal-{principal} has no hostAccount; no Session row written")
        return None
    id_token = tok.get("id_token", "")
    if not id_token:
        log(f"person-session: {principal} has no verified login; no Session row asked for")
        return None
    # #4517 AC6 — the door writes his Session row from his own verified login (Jeff 11-18
    # 00:17); this asks, naming only the channel. Owner, start and expiry are the door's.
    try:
        status, reply = http("POST", f"{api}/v1/identity/logins", id_token, {"channel": "browser"})
    except Exception as e:
        log(f"person-session: {principal} sign-in row NOT written: {e}")
        return None
    if status not in (200, 201) or not (reply or {}).get("name"):
        log(f"person-session: {principal} sign-in row NOT written: HTTP {status} {json.dumps(reply)[:200]}")
        return None
    stored = reply["name"]
    log(f"person-session: {principal} signed in, session {stored}")
    return {"principal": principal, "row": {"name": stored},
            "refresh_token": tok.get("refresh_token", ""), "id_token": id_token,
            "exp": int(tok.get("expires_in", 0) or 0) + int(now)}


def live_bearer(rec, now, refresh, log, why):
    """His id token, refreshed when it has under a minute left; None when it cannot be."""
    if rec.get("exp", 0) - now >= 60:
        return rec.get("id_token", "")
    fresh = refresh(rec.get("refresh_token", "")) if rec.get("refresh_token") else None
    if not fresh or not fresh.get("id_token"):
        log(f"person-session: {rec.get('principal')} {why} {rec['row']['name']}: token expired, no refresh")
        return None
    rec["id_token"] = fresh["id_token"]
    rec["refresh_token"] = fresh.get("refresh_token") or rec.get("refresh_token", "")
    rec["exp"] = int(fresh.get("expires_in", 0) or 0) + int(now)
    return rec["id_token"]


def touch_session(rec, now, expires_at, http, api, refresh, log):
    """#4500 — he is still here. #4517 AC6 — his (possibly refreshed) login renews his own
    open row at the door: lastSeenAt now, expiresAt the login's own (never later), tokenId the
    new login. `expires_at` is kept for the caller's signature; the door decides the expiry.
    Returns the updated record (tokens may be fresh), or None."""
    bearer = live_bearer(rec, now, refresh, log, "renewal could not touch")
    if not bearer:
        return None
    name = rec["row"]["name"]
    try:
        status, _ = http("POST", f"{api}/v1/identity/logins", bearer, {"channel": "browser", "renews": name})
    except Exception as e:
        log(f"person-session: touch failed: {e}")
        return None
    if status not in (200, 204):
        log(f"person-session: {rec.get('principal')} session {name} NOT renewed (HTTP {status})")
        return None
    return dict(rec)


def close_session(rec, now, http, api, refresh, log):
    """Sign-out: #4517 AC6 — his login asks the door to end his row; the door stamps how."""
    bearer = live_bearer(rec, now, refresh, log, "sign-out could not close")
    if not bearer:
        return False
    name = rec["row"]["name"]
    try:
        status, _ = http("POST", f"{api}/v1/identity/logouts", bearer, {"session": name})
    except Exception as e:
        log(f"person-session: close failed: {e}")
        return False
    ok = status in (200, 204)
    log(f"person-session: {rec.get('principal')} signed out, session {name} {'closed' if ok else f'NOT closed (HTTP {status})'}")
    return ok


def bare(owner):
    """'principal-jeff' and 'jeff' name the same person: the door serves the first, the row is written with the second."""
    o = str(owner or "")
    return o[len("principal-"):] if o.startswith("principal-") else o


def close_expired_sessions(principal, id_token, now, keep, http, api, log):
    """#4966 — at his sign-in, close his own browser sessions whose expiresAt has passed.

    Measured at the door 2026-11-15: 123 of jeff's browser Session rows were open and expired.
    The guard used to close an old session with that session's own token, long expired, so the
    store refused and the row stayed open. Jeff, 2026-11-15 13:34 "yes": the door uses his NEW
    sign-in, as him; no role writes his rows.

    Closes only rows that are his (ownedBy), browser sessions (never an agent's pane session,
    #4948), open, with an expiresAt already past, and never `keep` (the session this sign-in
    just made). Each ends at its own expiresAt, its true end, not now (Silas). The close is a
    PATCH of the two fields that change, never the row the door served: that row carries
    prefixed links (ownedBy principal-jeff, actsAs role-...), which the door refuses 422 when
    sent back (Silas, measured 2026-11-15 18:36). Never raises; returns how many it closed.
    """
    if not id_token:
        return 0
    # keep: the session this sign-in made, or every session the door still holds live
    kept = {keep} if isinstance(keep, str) else set(keep or ())
    try:
        status, listing = http("GET", f"{api}/v1/identity/sessions?endedAt.absent=true&channel=browser&limit=5000", None, None)
    except Exception as e:
        log(f"person-session: {principal} expired sessions not listed: {e}")
        return 0
    if status != 200:
        log(f"person-session: {principal} expired sessions not listed (HTTP {status})")
        return 0
    items = listing if isinstance(listing, list) else next(
        (listing.get(k) for k in ("items", "data", "rows") if isinstance(listing, dict) and isinstance(listing.get(k), list)), [])
    closed, refused, unjudged = 0, {}, 0
    for r in items:
        if not isinstance(r, dict) or not r.get("name") or r["name"] in kept:
            continue
        if bare(r.get("ownedBy")) != principal:
            continue
        if r.get("channel") != "browser":
            continue
        if r.get("sessionState") != "open":
            continue
        exp = r.get("expiresAt")
        # the store writes expiresAt as iso(); anything else is not a time we can judge
        if not isinstance(exp, str) or len(exp) != 20 or not exp.endswith("Z"):
            unjudged += 1
            continue
        if exp > iso(now):
            continue
        # #4517 AC6 — through the door's logout, as him: the door stamps endedAt (now, no
        # longer the row's own expiresAt: the caller cannot say how a session ended)
        try:
            st, _ = http("POST", f"{api}/v1/identity/logouts", id_token, {"session": r["name"]})
        except Exception:
            st = 0
        if st in (200, 204):
            closed += 1
        else:
            refused[st] = refused.get(st, 0) + 1
    if unjudged:
        # said, never silent: a store that re-serialises its times would otherwise turn this into a no-op
        log(f"person-session: {principal} {unjudged} open browser session(s) left open: expiresAt not in the store's time form")
    if closed or refused:
        # the status says why: 401/403 his token, 422 the body, 0 the door could not be reached
        why = ", ".join(f"{n} NOT closed (HTTP {st})" for st, n in sorted(refused.items()))
        log(f"person-session: {principal} sign-in closed {closed} expired browser session(s)" + (f", {why}" if why else ""))
    return closed


def client_document(client_id, redirect_uris, logout_uris):
    """The Solid Client ID Document: the guard's fixed identity as an OIDC client."""
    return {
        "@context": ["https://www.w3.org/ns/solid/oidc-context.jsonld"],
        "client_id": client_id,
        "client_name": "Chorus sign-in",
        "redirect_uris": sorted(set(redirect_uris)),
        "post_logout_redirect_uris": sorted(set(logout_uris)),
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "scope": "openid webid offline_access",
        "token_endpoint_auth_method": "none",
    }
