#!/usr/bin/env bash
# authz-scope-reachable.sh (#4058) — is every scope a surface REQUIRES actually
# HELD by some principal?
#
# WHY. On 2026-07-08 the security model started declaring surfaces that require
# `urn:chorus:domains:code`. No principal was ever granted it. Every caller —
# roles, the SDK, the nightly, tests — got 403 out-of-scope on those surfaces for
# eight weeks and nothing noticed, because a surface nobody can reach looks
# exactly like a surface nobody called. It surfaced only when #4015 (2026-08-27)
# made per-case jest results storable and 48 x "Received: 403" appeared at once.
#
# The grant closes today's 403s. THIS closes the class: a requiresScope naming a
# scope no principal holds is a declaration that the surface is dead, and it
# should be loud the day it lands, not two months later.
#
# Emits, for chorus-health to consume:
#   UNREACHABLE_SURFACES=<n>     total surfaces requiring an unheld scope
#   UNREACHABLE_SCOPES=<n>       distinct unheld scopes
#   plus one "  <n> surface(s)  <scope>" row each
# Exit 1 when n > 0, 2 when it could not measure. Never 0 on "couldn't tell".
set -uo pipefail

SPARQL_URL="${AUTHZ_SPARQL_URL:-http://localhost:3030/pods/query}"
PREFIX='PREFIX c: <https://jeffbridwell.com/chorus#>'

# Test seam: both sides can be supplied as plain files, one scope per line for
# HELD, "<scope>" repeated per surface for REQUIRED. A fixture must not need a
# live triplestore to prove this check can go red (#3528 — a test brings its own
# world; #3734 — a gate ships with a fixture where the rule is VIOLATED).
AUTHZ_HELD_FILE="${AUTHZ_HELD_FILE:-}"
AUTHZ_REQUIRED_FILE="${AUTHZ_REQUIRED_FILE:-}"

# Credential comes from the shared FUSEKI_AUTH array (fuseki-auth.sh), never an
# inline -u user:pass — that shape trips the gitleaks curl-auth rule at the
# pre-commit gate, correctly, even when the parts are only variable names.
_q() {
  curl -s --max-time 15 "${FUSEKI_AUTH[@]}" \
    -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=$1" "$SPARQL_URL"
}

if [ -n "$AUTHZ_HELD_FILE" ] && [ -n "$AUTHZ_REQUIRED_FILE" ]; then
  [ -f "$AUTHZ_HELD_FILE" ] && [ -f "$AUTHZ_REQUIRED_FILE" ] || {
    echo "UNREACHABLE_SURFACES=UNMEASURED"; echo "fixture file(s) missing"; exit 2; }
  HELD=$(cat "$AUTHZ_HELD_FILE")
  REQ=$(cat "$AUTHZ_REQUIRED_FILE")
else
  FUSEKI_AUTH=()
  # shellcheck disable=SC1090
  source "$(dirname "${BASH_SOURCE[0]}")/fuseki-auth.sh" 2>/dev/null || true
  HELD_JSON=$(_q "$PREFIX SELECT DISTINCT ?s WHERE { GRAPH ?g { ?p c:hasScope ?s } }")
  REQ_JSON=$(_q "$PREFIX SELECT ?s ?x WHERE { GRAPH ?g { ?x c:requiresScope ?s } }")
  # A store that did not answer is UNMEASURED, never "zero unreachable". The
  # benign default is exactly how the eight weeks happened.
  HELD=$(printf '%s' "$HELD_JSON" | python3 -c '
import json,sys
try: d=json.load(sys.stdin)
except Exception: sys.exit(3)
for b in d["results"]["bindings"]: print(b["s"]["value"])
') || { echo "UNREACHABLE_SURFACES=UNMEASURED"; echo "store did not answer (hasScope)"; exit 2; }
  REQ=$(printf '%s' "$REQ_JSON" | python3 -c '
import json,sys
try: d=json.load(sys.stdin)
except Exception: sys.exit(3)
for b in d["results"]["bindings"]: print(b["s"]["value"])
') || { echo "UNREACHABLE_SURFACES=UNMEASURED"; echo "store did not answer (requiresScope)"; exit 2; }
fi

# Applies to BOTH input paths. No surface declaring any scope is not a healthy
# zero — the model is absent, or the query is wrong, or the fixture is empty.
# This guard was originally on the SPARQL branch only, and its own negative proof
# caught that: through the fixture path an empty model reported a clean bill.
# That is precisely the vacuous pass this whole card exists to make impossible.
if [ -z "$REQ" ]; then
  echo "UNREACHABLE_SURFACES=UNMEASURED"
  echo "no surface declares requiresScope — model absent or query wrong, not a clean bill"
  exit 2
fi

python3 - "$HELD" "$REQ" <<'PY'
import sys, collections
held = {l.strip() for l in sys.argv[1].splitlines() if l.strip()}
req  = collections.Counter(l.strip() for l in sys.argv[2].splitlines() if l.strip())
bad  = {s: n for s, n in req.items() if s not in held}
print(f"UNREACHABLE_SURFACES={sum(bad.values())}")
print(f"UNREACHABLE_SCOPES={len(bad)}")
for s, n in sorted(bad.items(), key=lambda kv: -kv[1]):
    print(f"  {n:>3} surface(s)  {s}")
sys.exit(1 if bad else 0)
PY
