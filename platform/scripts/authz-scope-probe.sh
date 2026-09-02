#!/usr/bin/env bash
# authz-scope-probe.sh (#4058) — AC2 + AC3 as ONE runnable check against a LIVE api.
#
# AC2: a caller holding urn:chorus:domains:code reaches POST /api/athena/reload (2xx).
# AC3: a caller WITHOUT it is refused 403 — the grant did not widen into open-by-default.
#
# Both legs must hold for exit 0. A 401 on either leg is UNMEASURED (the token was
# not accepted as identity, so scope was never evaluated) and FAILS — a probe that
# cannot tell "no identity" from "no scope" would pass for the wrong reason (#3734).
#
# Usage:
#   AUTHZ_API=http://localhost:3343 \
#   SCOPED_TOKEN="$(chorus-identity-token silas)" \
#   UNSCOPED_TOKEN="$(chorus-identity-token chorus-sdk)" \
#   authz-scope-probe.sh
#
# Route: /api/athena/reload is surface-post-api-athena-reload (requiresScope
# domains:code, security-3619-surfaces-final.ttl). It re-reads the model from the
# store; against a werk variant it mutates nothing durable.
set -uo pipefail

API="${AUTHZ_API:-http://localhost:3343}"
ROUTE="${AUTHZ_ROUTE:-/api/athena/reload}"
[ -n "${SCOPED_TOKEN:-}" ]   || { echo "authz-scope-probe: SCOPED_TOKEN unset" >&2;   exit 2; }
[ -n "${UNSCOPED_TOKEN:-}" ] || { echo "authz-scope-probe: UNSCOPED_TOKEN unset" >&2; exit 2; }

probe() {
  curl -s -o /dev/null -w '%{http_code}' --max-time 20 -X POST \
    -H "Authorization: Bearer $1" -H 'Content-Type: application/json' \
    --data '{}' "$API$ROUTE" 2>/dev/null || echo 000
}

scoped=$(probe "$SCOPED_TOKEN")
unscoped=$(probe "$UNSCOPED_TOKEN")
echo "route=$ROUTE api=$API"
echo "scoped   caller -> $scoped   (AC2 wants 2xx)"
echo "unscoped caller -> $unscoped   (AC3 wants 403)"

rc=0
case "$scoped" in
  2*) ;;
  401) echo "AC2 UNMEASURED: scoped token not accepted as identity (401)" >&2; rc=1 ;;
  *)   echo "AC2 FAIL: scoped caller got $scoped" >&2; rc=1 ;;
esac
case "$unscoped" in
  403) ;;
  401) echo "AC3 UNMEASURED: unscoped token not accepted as identity (401)" >&2; rc=1 ;;
  *)   echo "AC3 FAIL: unscoped caller got $unscoped — grant widened or door open" >&2; rc=1 ;;
esac
[ "$rc" -eq 0 ] && echo "AUTHZ_SCOPE_PROBE=pass" || echo "AUTHZ_SCOPE_PROBE=fail"
exit "$rc"
