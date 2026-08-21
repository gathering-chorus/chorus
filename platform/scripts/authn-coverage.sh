#!/usr/bin/env bash
# authn-coverage.sh — probe every live MUTATING surface (UI/API/CLI/DB) and
# report whether an UNAUTHENTICATED / UNVERIFIED actor can mutate it.
#
# Read-only: benign no-op writes; reads ONLY the auth verdict (status), never
# lands data. Routes are the REAL mutation endpoints (verified live). A 404
# means the probe's route is wrong, so it scores MISPROBE — never COVERED
# (verify the store, not a route that 404s clean — the #3837/#3958 lesson).
#
# Coverage = MUTATION surfaces that REFUSE an unverified actor (401/403) over
# all mutation surfaces probed. Reads are reported separately, unscored.
#
# Modes:
#   authn-coverage.sh            full live run + coverage %
#   authn-coverage.sh score CODE print the verdict for one status code (testable)
#   authn-coverage.sh probe URL  print "VERDICT CODE" for one POST (testable)

set -uo pipefail

# --- scoring: the one place a status code becomes a verdict ---
# COVERED    only on an explicit auth refusal (401/403)
# MISPROBE   on 404 / unreachable — route wrong or service down; NEVER covered
# OPEN       on any 2xx (accepted an unverified mutation) or a non-auth 4xx/5xx
score() {
  case "$1" in
    401|403) echo COVERED ;;
    000|404) echo MISPROBE ;;
    2*)      echo OPEN ;;
    *)       echo OPEN ;;
  esac
}

_code() { curl -s -o /dev/null -w '%{http_code}' --max-time 6 "$@" 2>/dev/null || echo 000; }

# probe URL [curl-args...] -> "VERDICT CODE"
probe_url() {
  local url="$1"; shift
  local code; code=$(_code -X POST "$url" "$@")
  echo "$(score "$code") $code"
}

# --- sub-command dispatch (for tests) ---
if [ "${1:-}" = "score" ]; then score "${2:-}"; exit 0; fi
if [ "${1:-}" = "probe" ]; then probe_url "${2:?probe needs URL}" "${@:3}"; exit 0; fi

# --- full live run ---
MPASS=0; MTOTAL=0
row() { printf '%-24s %-4s %-10s %-5s %s\n' "$1" "$2" "$3" "$4" "$5"; }
mutate() { # name class code note
  MTOTAL=$((MTOTAL+1)); local v; v=$(score "$3")
  [ "$v" = COVERED ] && MPASS=$((MPASS+1))
  row "$1" "$2" "$v" "$3" "$4"
}

echo "=== authn coverage — unauthenticated MUTATION probes ($(TZ=America/New_York date '+%Y-%m-%d %H:%M')) ==="
row SURFACE CLASS VERDICT CODE NOTE
echo "------------------------------------------------------------------------------------"

# DB / STORE
mutate "fuseki-update"       DB  "$(_code -X POST http://localhost:3030/chorus/update -H 'Content-Type: application/sparql-update' --data 'DELETE WHERE { <urn:probe:none> ?p ?o }')" "bare SPARQL update"
mutate "owlapi-batch-write"  DB  "$(_code -X POST http://localhost:3360/batch -H 'Content-Type: application/json' -d '{"ops":[]}')" "anon model batch write"

# API
mutate "chorus-api-card-add"  API "$(_code -X POST http://localhost:3340/api/cards/add -H 'Content-Type: application/json' -d '{}')" "anon card create :3340"
mutate "chorus-api-card-move" API "$(_code -X POST http://localhost:3340/api/cards/move -H 'Content-Type: application/json' -d '{}')" "anon card move :3340"

# MCP (agent action surface — no verified caller identity)
mutate "mcp-tools-call-anon"  API "$(_code -X POST http://localhost:3341/mcp -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"chorus_cards_add","arguments":{}}}')" "anon MCP tool-call, no role"

# UI
mutate "clearing-post"        UI  "$(_code -X POST http://localhost:3470/api/message -H 'Content-Type: application/json' -d '{"text":"probe"}')" "anon post into Clearing"
mutate "pulse-chat-message"   UI  "$(_code -X POST http://localhost:3475/api/chat/probe/message -H 'Content-Type: application/json' -d '{"text":"probe"}')" "anon pulse chat write"

# CLI / identity-assertion (asserted role, no verified token — fail-closed #3687?)
mutate "dal-write-asserted"   CLI "$(_code -X POST http://localhost:3360/batch -H 'Content-Type: application/json' -H 'X-Chorus-Role: silas' -d '{"ops":[{"op":"add","s":"urn:probe:x","p":"urn:probe:p","o":"1"}]}')" "asserted role, unverified token"

echo "------------------------------------------------------------------------------------"
echo "-- reads (informational; unscored — some reads are intentionally public) --"
r() { _code "$@"; }
row "owlapi-principals-read" API "$(score $(r http://localhost:3360/principals))"          "$(r http://localhost:3360/principals)"          "private identity read"
row "chorus-api-context"    API "$(score $(r http://localhost:3340/api/chorus/context/health))" "$(r http://localhost:3340/api/chorus/context/health)" "board/health read"

echo "------------------------------------------------------------------------------------"
PCT=0; [ "$MTOTAL" -gt 0 ] && PCT=$(( MPASS * 100 / MTOTAL ))
echo "MUTATION COVERAGE: ${MPASS}/${MTOTAL} surfaces refuse an unverified actor (${PCT}%)"
echo "COVERAGE_PCT=${PCT}"
echo "(MISPROBE = route 404/unreachable — fix the probe route, never scored as covered)"
