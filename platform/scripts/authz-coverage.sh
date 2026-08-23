#!/usr/bin/env bash
# authz-coverage.sh — does each protected action actually ENFORCE the
# authorization it should? The authZ companion to authn-coverage.sh (#3960).
#
# authN asked a BINARY question: can an UNAUTHENTICATED actor mutate? (one
# probe, no creds → expect 401). authZ is a MATRIX: an actor with a VALID
# identity but the WRONG privilege must still be refused. This tool measures
# axis 1 — SCOPE enforcement:
#
#   present a VALID identity token whose Principal LACKS the surface's
#   required scope → expect 403 out-of-scope.
#     403  COVERED   — the door enforced scope
#     200  OPEN       — authenticated ≠ authorized, but it got through
#     401  MISPROBE   — authn refused first; can't measure authz here
#     000/404 MISPROBE — route wrong / unreachable; never scored covered
#
# Enforcement lives at ONE door (platform/api/src/security-envelope.ts): it
# only fires for surfaces DECLARED as chorus:APISurface with a requiresScope in
# the model. A mutating route with no such declaration is OPEN by construction
# — the envelope never matches it. So coverage has two parts: (a) of the
# declared-secured surfaces, how many actually 403 a under-scoped caller
# (declared ⇒ enforced?), and (b) how many mutating routes are declared at all.
#
# Axis 2 — ROLE LANES (can silas act in wren's lane: deploy/accept/land beyond
# its role?) has no single door and is a SEPARATE phase-2 tool; named here, not
# measured. See card #3977.
#
# Read-only: probes with a benign no-op body; reads only the auth verdict.
#
# Modes:
#   authz-coverage.sh            full run (needs CHORUS_AUTHZ_PROBE_TOKEN)
#   authz-coverage.sh score CODE verdict for one status code (testable)

set -uo pipefail

# --- scoring: the one place a status becomes a verdict ---
# COVERED  only on 403 (scope refusal — the authz door did its job)
# OPEN     on 2xx (an under-scoped caller mutated)
# MISPROBE on 401 (authn refused first — authz not reached) or 000/404
score() {
  case "$1" in
    403)     echo COVERED ;;
    401)     echo MISPROBE ;;   # authn-first: can't observe authz
    000|404) echo MISPROBE ;;   # route wrong / unreachable
    2*)      echo OPEN ;;
    *)       echo OPEN ;;       # any other non-403 refusal isn't a scope refusal
  esac
}

if [ "${1:-}" = "score" ]; then score "${2:-}"; exit 0; fi

# #3979 — classify a NOT-scope-covered route by its OTHER auth mechanism. The
# default is GAP (fail-safe): a route that is neither on Clearing's bridge/session
# auth nor loopback-only is an unprotected mutating route, full stop. A new
# Clearing/pulse route must be added here explicitly — silence never justifies.
justify() { # PATH -> BRIDGE | LOOPBACK | GAP
  case "$1" in
    /api/chat/*|/api/message|/api/room/bind|/api/upload|/api/voice|/set-name|/api/account/password|/api/restart) echo BRIDGE ;;
    /mcp|/nudge|/drain|/api/nudge|/api/board-event|/api/jeff-input) echo LOOPBACK ;;
    *) echo GAP ;;
  esac
}
if [ "${1:-}" = "justify" ]; then justify "${2:-}"; exit 0; fi

# The store's SPARQL query endpoint is the `pods` dataset (NOT `chorus`), and it
# requires auth even to READ. A pure ops probe shouldn't carry store keys — so
# the DURABLE fix is a read-free chorus-api endpoint that projects the secured
# surfaces (follow-on card). Until then, read with the governed creds the other
# ops scripts already use (fuseki-write.env), referencing the file, never values.
FUSEKI="${CHORUS_FUSEKI_QUERY:-http://localhost:3030/pods/query}"
API="${CHORUS_API_BASE:-http://localhost:3340}"
TOKEN="${CHORUS_AUTHZ_PROBE_TOKEN:-}"   # a VALID identity whose Principal lacks scope
CRED_FILE="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"
FU=$(grep -E '^FUSEKI_ADMIN_USER=' "$CRED_FILE" 2>/dev/null | cut -d= -f2- | tr -d "\"' ")
FP=$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$CRED_FILE" 2>/dev/null | cut -d= -f2- | tr -d "\"' ")
[ -z "$FU" ] && FU=admin

_code() { curl -s -o /dev/null -w '%{http_code}' --max-time 6 "$@" 2>/dev/null || echo 000; }
_query() { curl -s -G --max-time 8 -u "${FU}:${FP}" "$FUSEKI" --data-urlencode "query=$1" -H 'Accept: application/json' 2>/dev/null; }

echo "=== authz coverage — under-privileged (valid identity, missing scope) probes ($(TZ=America/New_York date '+%Y-%m-%d %H:%M')) ==="

# --- the declared-secured surface set, from the model (GRAPH urn:chorus:ontology) ---
SURF_QUERY='PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?method ?pathPrefix ?requiresScope WHERE {
  GRAPH <urn:chorus:ontology> { ?s a chorus:APISurface ; chorus:requiresScope ?requiresScope .
             OPTIONAL { ?s chorus:httpMethod ?method } OPTIONAL { ?s chorus:pathPrefix ?pathPrefix } } }'
rows=$(_query "$SURF_QUERY" | python3 -c "import json,sys
try:
    d=json.load(sys.stdin)
    for b in d['results']['bindings']:
        print((b.get('method',{}).get('value','POST')), (b.get('pathPrefix',{}).get('value','')), (b.get('requiresScope',{}).get('value','')))
except Exception: pass" 2>/dev/null)

if [ -z "$rows" ]; then
  echo "DEGRADED: no chorus:APISurface with requiresScope readable (store unreachable or creds absent)."
  echo "AUTHZ_DECLARED_SURFACES=0"
  echo "DECLARED_COVERAGE_PCT=0"
  echo "(DEGRADED means the store read failed — investigate, do NOT read this as zero surfaces.)"
  exit 0
fi

# --- the under-scoped probe identity ---
# A valid identity whose Principal LACKS a given scope is the authz test actor.
# Default: mint the probe role's own token (chorus-identity-token) and read its
# grants; probe ONLY surfaces requiring a scope the role LACKS — a correct door
# 403s those with no mutation risk (refused before the body is processed).
# Surfaces the role HOLDS are SKIPPED (probing them could mutate). Override with
# CHORUS_AUTHZ_PROBE_TOKEN + CHORUS_AUTHZ_PROBE_LACKS="scopeA,scopeB".
PROBE_ROLE="${CHORUS_AUTHZ_PROBE_ROLE:-silas}"
if [ -z "$TOKEN" ]; then
  MINT="$(command -v chorus-identity-token 2>/dev/null)"
  [ -z "$MINT" ] && [ -x "${ROOT_EARLY:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}/chorus-identity-token" ] \
    && MINT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/chorus-identity-token"
  [ -n "$MINT" ] && TOKEN="$("$MINT" "$PROBE_ROLE" 2>/dev/null || echo '')"
fi
GRANTS=$(_query "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?scope WHERE { GRAPH ?g { ?p chorus:hasScope ?scope . FILTER(CONTAINS(STR(?p),\"$PROBE_ROLE\")) } }" \
  | python3 -c "import json,sys
try:
    d=json.load(sys.stdin); print(' '.join(sorted({b['scope']['value'] for b in d['results']['bindings']})))
except Exception: pass" 2>/dev/null)

DECLARED=0; ENFORCED=0; OPEN=0; SKIP=0
printf '%-8s %-30s %-14s %-9s %s\n' METHOD PATH REQ-SCOPE VERDICT CODE
echo "-------------------------------------------------------------------------------------------"
while read -r method path scope; do
  [ -z "$path" ] && continue
  DECLARED=$((DECLARED+1))
  sname="${scope##*[:#]}"
  if [ -z "$TOKEN" ]; then
    printf '%-8s %-30s %-14s %-9s %s\n' "$method" "$path" "$sname" "NO-TOKEN" "-"; continue
  fi
  # only probe if the probe role LACKS this scope (safe: expect 403, no mutation)
  if printf '%s' " $GRANTS " | grep -qF " $scope "; then
    SKIP=$((SKIP+1))
    printf '%-8s %-30s %-14s %-9s %s\n' "$method" "$path" "$sname" "SKIP-held" "-"; continue
  fi
  code=$(_code -X "${method:-POST}" "${API}${path}" -H 'Content-Type: application/json' -H "Authorization: Bearer $TOKEN" -d '{}')
  v=$(score "$code")
  case "$v" in COVERED) ENFORCED=$((ENFORCED+1));; OPEN) OPEN=$((OPEN+1));; esac
  printf '%-8s %-30s %-14s %-9s %s\n' "$method" "$path" "$sname" "$v" "$code"
done <<< "$rows"

echo "-------------------------------------------------------------------------------------------"

# --- the denominator, done RIGHT (#3979) ---
# The envelope matches by METHOD + PATH-PREFIX (security-envelope.ts:56,
# `req.path.startsWith(s.pathPrefix)`) — so one declared surface covers all its
# sub-routes. The naive count (declared surfaces vs total routes) OVER-reports
# the gap by treating every prefix-covered sub-route as undeclared. And two live
# services carry their OWN auth, not the scope envelope: Clearing (bridge-token /
# CSS session, #3966/#3743) and pulse/mcp (loopback-trust, #3967/#3390). Those
# are protected, not gaps — classify them, don't count them as holes.
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
# enumerate every mutating route as "METHOD /path"
ALL_ROUTES=$(for svc in platform/api/src platform/mcp-server/src directing/clearing/src platform/pulse/src; do
  grep -rhoE "app\.(post|put|delete|patch)\((['\"][^'\"]+)" "$ROOT/$svc" --include='*.ts' 2>/dev/null | grep -v test
done | sed -E "s/app\.(post|put|delete|patch)\(['\"]/\1 /" | awk '{print toupper($1), $2}' | sort -u)
# declared surface prefixes as "METHOD /prefix" (from the model rows already read)
DECL_PREFIXES=$(printf '%s\n' "$rows" | awk 'NF>=2{print toupper($1), $2}')

TOTAL_MUT=0; COVERED_PFX=0; BRIDGE=0; LOOPBACK=0; REAL_GAP=0
REAL_GAP_LIST=""
while read -r rm rp; do
  [ -z "$rp" ] && continue
  TOTAL_MUT=$((TOTAL_MUT+1))
  # prefix-covered by a declared scope surface?
  if printf '%s\n' "$DECL_PREFIXES" | awk -v m="$rm" -v p="$rp" '$1==m && index(p,$2)==1{f=1} END{exit !f}'; then
    COVERED_PFX=$((COVERED_PFX+1)); continue
  fi
  # not scope-covered → classify by the route's OTHER auth mechanism (same
  # justify() the negative-proof test exercises; default is GAP, fail-safe)
  case "$(justify "$rp")" in
    BRIDGE)   BRIDGE=$((BRIDGE+1)); continue ;;
    LOOPBACK) LOOPBACK=$((LOOPBACK+1)); continue ;;
  esac
  REAL_GAP=$((REAL_GAP+1)); REAL_GAP_LIST="${REAL_GAP_LIST}\n    ${rm} ${rp}"
done <<< "$ALL_ROUTES"

echo "SURFACE POSTURE (prefix-aware, #3979)"
echo "  mutating routes:                        ${TOTAL_MUT}"
echo "  scope-covered (envelope prefix-match):  ${COVERED_PFX}"
echo "  Clearing bridge-token/session auth:     ${BRIDGE}   (justified — #3966/#3743, not scope)"
echo "  loopback-trust (mcp/pulse):             ${LOOPBACK}   (justified — DEC-093, #3390/#3967)"
echo "  REAL GAP (unprotected mutating route):  ${REAL_GAP}"
[ "$REAL_GAP" -gt 0 ] && printf "    the gap:${REAL_GAP_LIST}\n"
UNDECLARED=$REAL_GAP
DPCT=0; [ "$TOTAL_MUT" -gt 0 ] && DPCT=$(( (COVERED_PFX + BRIDGE + LOOPBACK) * 100 / TOTAL_MUT ))
echo "  PROTECTED_PCT=${DPCT}   (scope + bridge + loopback, over all mutating routes)"
echo ""

if [ -z "$TOKEN" ]; then
  echo "ENFORCEMENT (403 probe): SKIPPED — chorus-identity-token unavailable and no CHORUS_AUTHZ_PROBE_TOKEN."
  echo "AUTHZ_DECLARED_SURFACES=${DECLARED}"
  echo "AUTHZ_ENFORCED=unmeasured"
  echo "(declared ≠ enforced — the token probe is what proves it.)"
  exit 0
fi
PROBED=$(( ENFORCED + OPEN ))
EPCT=0; [ "$PROBED" -gt 0 ] && EPCT=$(( ENFORCED * 100 / PROBED ))
echo "ENFORCEMENT  (probe role: ${PROBE_ROLE}, under-scoped on the tested surfaces)"
echo "  probed (role lacks the scope):  ${PROBED}"
echo "  ENFORCED (403):                 ${ENFORCED}"
echo "  OPEN (got past authz):          ${OPEN}"
echo "  skipped (role holds the scope): ${SKIP}"
echo "  ENFORCEMENT_PCT=${EPCT}   (of the surfaces we could safely test, this fraction actually 403'd)"
echo ""
echo "AUTHZ_DECLARED_SURFACES=${DECLARED}   AUTHZ_ENFORCED=${ENFORCED}   AUTHZ_OPEN=${OPEN}"
echo "(COVERED = an under-scoped valid identity was refused 403. OPEN = it got past authz.)"
echo "(The real gap is the ${UNDECLARED} UNDECLARED routes: no APISurface, so the envelope never checks them.)"
echo "(Axis 2 — role lanes (silas acting in wren's lane) — is a separate phase-2 tool, card #3977.)"
