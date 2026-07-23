#!/usr/bin/env python3
# chorus-mint-token.py (#3573) — mint a SCOPED HS256 service token for the governed
# Fuseki write door. This is the client-side counterpart of owl-api's verify-only
# seam auth (owl-api/src/auth.rs): a writer mints a short-lived token naming the
# graph(s) it may write, so it can pass the door instead of getting 401.
#
# Token format (MUST match owl-api/src/auth.rs verify_token, origin/main):
#   header : {"alg":"HS256","typ":"JWT"}           (base64url, no pad)
#   claims : {"webId":..,"aud":"chorus","exp":<unix>,"scope":[graphs],"agentId":?}
#   sig    : base64url( HMAC_SHA256( b64url(header)+"."+b64url(claims), SECRET ) )
#   secret : env CHORUS_SERVICE_TOKEN_SECRET   (fail-closed if unset — never mint blind)
#
# The webId MUST be in owl-api's phase-1 chorus-agent registry or the door 403s
# (WebIdNotAllowed). scope MUST name the target graph or handle_write 403s
# (out-of-scope). Keys live in env = Silas ops lane (#3567 decision).
import os, sys, json, time, hmac, hashlib, base64, argparse


def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def main() -> int:
    ap = argparse.ArgumentParser(description="mint a scoped HS256 service token for the governed Fuseki write door")
    ap.add_argument("--web-id", "--webId", dest="web_id", required=True,
                    help="caller webId — must be in owl-api's chorus-agent registry")
    ap.add_argument("--scope", action="append", default=[], metavar="GRAPH",
                    help="target graph this token permits (repeatable); empty = unscoped/legacy")
    ap.add_argument("--ttl", type=int, default=300, help="seconds until exp (default 300)")
    ap.add_argument("--agent-id", "--agentId", dest="agent_id", default="")
    ap.add_argument("--bearer", action="store_true", help='prefix output with "Bearer "')
    a = ap.parse_args()

    secret = os.environ.get("CHORUS_SERVICE_TOKEN_SECRET")
    if not secret:
        # #3592 — realm-env fallback: the same canonical gitignored file the
        # service launch wrappers `set -a` source (#3402). Callers spawned
        # outside a wrapper (werk-test's mint_token) otherwise fail every mint.
        # Still fail-closed if the file is absent or lacks the key.
        realm = os.environ.get("CHORUS_REALM_ENV") or os.path.join(
            os.path.expanduser("~"), ".chorus", "secrets", "chorus-realm.env")
        try:
            with open(realm) as f:
                for line in f:
                    line = line.strip()
                    if line.startswith("CHORUS_SERVICE_TOKEN_SECRET="):
                        secret = line.split("=", 1)[1].strip().strip('"').strip("'")
                        break
        except OSError:
            pass
    if not secret:
        sys.stderr.write("chorus-mint-token: CHORUS_SERVICE_TOKEN_SECRET unset — refusing to mint (fail-closed)\n")
        return 1

    header = b64url(b'{"alg":"HS256","typ":"JWT"}')
    claims = {"webId": a.web_id, "aud": "chorus", "exp": int(time.time()) + a.ttl, "scope": a.scope}
    if a.agent_id:
        claims["agentId"] = a.agent_id
    payload = b64url(json.dumps(claims, separators=(",", ":")).encode())
    sig = b64url(hmac.new(secret.encode(), f"{header}.{payload}".encode(), hashlib.sha256).digest())
    token = f"{header}.{payload}.{sig}"
    print(f"Bearer {token}" if a.bearer else token)
    return 0


if __name__ == "__main__":
    sys.exit(main())
