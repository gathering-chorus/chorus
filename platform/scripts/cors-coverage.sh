#!/usr/bin/env bash
# @test-type: fitness:security
# cors-coverage.sh (#2436) — the third axis of the coverage probes.
#
#   authn-coverage.sh   can an UNVERIFIED actor mutate this surface?
#   authz-coverage.sh   is this surface's authorization declared and enforced?
#   cors-coverage.sh    WHICH BROWSER ORIGINS may reach this surface at all?
#
# The gap this closes: a wildcard sat on /api/athena/* alongside POST/PUT/DELETE
# from April to September. Both existing probes were green the whole time,
# because neither asks the CORS question. Nothing in the repo did — one grep for
# 'Access-Control' across every test directory returned a single comment.
#
# Read-only. Sends preflights (OPTIONS) and reads ONLY the response headers.
# A preflight never reaches a handler, so this cannot mutate anything.
#
# WHAT MAKES A ROW GREEN is two-sided on purpose. "Refuses a stranger" alone is
# satisfiable by a surface that refuses everyone, including our own pages — that
# would score perfectly while breaking every demo. So each route is probed twice
# and must get BOTH answers right.
#
# Modes:
#   cors-coverage.sh              full live run + coverage %
#   cors-coverage.sh score H      verdict for one Allow-Origin header value (testable)
#   cors-coverage.sh probe URL O  print "VERDICT HEADER" for one preflight (testable)

set -uo pipefail

API="${CHORUS_API_URL:-http://localhost:3340}"

# An origin that resolves to THIS machine but is not a hostname we recognise.
# localtest.me is a public DNS name mapping to 127.0.0.1, so the probe reaches a
# real local server while presenting a foreign Origin — the two properties a
# meaningful negative case needs at once.
FOREIGN_ORIGIN="${CORS_FOREIGN_ORIGIN:-http://localhost.localtest.me:9999}"
LOCAL_ORIGIN="${CORS_LOCAL_ORIGIN:-http://localhost:3000}"

# --- scoring: the one place a header value becomes a verdict ---
# Called with what came back for a FOREIGN origin.
#   SECURED   no Access-Control-Allow-Origin at all — the browser will refuse
#   WILDCARD  '*' — every page on the network may read it
#   REFLECTED '<the foreign origin>' — worse than a wildcard: it looks deliberate
#   MISPROBE  the route did not answer; never scored as secured
# $2 is the origin that was ASKED. Without it, a route that always answers a
# fixed `http://localhost:3000` looks identical to one echoing the caller back —
# and they are opposites: the browser compares the value to its own origin, so a
# fixed OTHER origin is a refusal, while an echo is a grant. The first version of
# this scorer had no second argument and reported three correctly-configured
# routes as REFLECTED. Measured against prod 2026-09-06.
score() {
  local hdr="${1:-}" asked="${2:-}"
  case "$hdr" in
    '')       echo SECURED ;;
    '*')      echo WILDCARD ;;
    MISPROBE) echo MISPROBE ;;
    *)
      if [ -n "$asked" ] && [ "$hdr" = "$asked" ]; then echo REFLECTED
      else echo SECURED; fi ;;
  esac
}

# The companion question. Called with what came back for a LOCAL origin.
#   GRANTED  our own pages can still read it
#   DENIED   we locked ourselves out — a green stranger-test hiding a broken UI
score_local() {
  case "${1:-}" in
    '')       echo DENIED ;;
    MISPROBE) echo MISPROBE ;;
    *)        echo GRANTED ;;
  esac
}

# allow_origin_for URL ORIGIN -> the header value, '' if absent, MISPROBE if dead
allow_origin_for() {
  local url="$1" origin="$2" out
  out=$(curl -s -o /dev/null -D - -X OPTIONS \
          -H "Origin: $origin" \
          -H 'Access-Control-Request-Method: POST' \
          --max-time 6 "$url" 2>/dev/null) || { echo MISPROBE; return; }
  [ -z "$out" ] && { echo MISPROBE; return; }
  # NOTE: awk's IGNORECASE is a gawk extension and does NOTHING in the BSD awk
  # macOS ships. The first version of this used it, so the pattern never matched
  # the capitalised header, every route came back with an empty value, and the
  # probe cheerfully scored a live `Allow-Origin: *` as SECURED. It was written
  # to catch exactly that state and could not see it. tolower() is portable.
  printf '%s\n' "$out" \
    | tr -d '\r' \
    | awk 'tolower($0) ~ /^access-control-allow-origin:/ {sub(/^[^:]*: */,""); print; exit}'
}

probe_url() { # URL [ORIGIN] -> "VERDICT HEADER"
  local url="$1" origin="${2:-$FOREIGN_ORIGIN}" hdr
  hdr=$(allow_origin_for "$url" "$origin")
  echo "$(score "$hdr" "$origin") ${hdr:-<none>}"
}

# --- sub-command dispatch (kept so the scoring is unit-testable without a server) ---
if [ "${1:-}" = "score" ];       then score "${2-}" "${3-}"; exit 0; fi
if [ "${1:-}" = "score-local" ]; then score_local "${2-}"; exit 0; fi
if [ "${1:-}" = "probe" ];       then probe_url "${2:?probe needs URL}" "${3:-}"; exit 0; fi

# --- full live run ---
# The routes are the CORS-bearing prefixes actually mounted by chorus-api.
ROUTES="
/api/athena/health
/api/athena/subdomains
/api/chorus/context/health
/api/chorus/open
/api/loom/principles
"

PASS=0; TOTAL=0; FAILED=0
printf '%-32s %-10s %-9s %s\n' ROUTE STRANGER OURS NOTE
printf '%-32s %-10s %-9s %s\n' -------------------------------- ---------- --------- ----

for r in $ROUTES; do
  [ -z "$r" ] && continue
  TOTAL=$((TOTAL+1))
  fh=$(allow_origin_for "$API$r" "$FOREIGN_ORIGIN")
  lh=$(allow_origin_for "$API$r" "$LOCAL_ORIGIN")
  fv=$(score "$fh" "$FOREIGN_ORIGIN")
  lv=$(score_local "$lh")
  note=""
  if [ "$fv" = SECURED ] && [ "$lv" = GRANTED ]; then
    PASS=$((PASS+1))
  else
    FAILED=$((FAILED+1))
    case "$fv" in
      WILDCARD)  note="any page on the network may read this" ;;
      REFLECTED) note="echoes a stranger's origin back" ;;
      MISPROBE)  note="route did not answer — not scored as secured" ;;
    esac
    [ "$lv" = DENIED ] && note="${note:+$note; }our own pages are locked out too"
  fi
  printf '%-32s %-10s %-9s %s\n' "$r" "$fv" "$lv" "$note"
done

echo
echo "cors-coverage: $PASS/$TOTAL routes refuse a stranger AND serve our own pages"

if [ "$FAILED" -gt 0 ]; then
  echo "cors-coverage: FAIL — $FAILED route(s) above"
  exit 1
fi
echo "cors-coverage: PASS"
exit 0
