#!/bin/bash
# @test-type: integration:ui — needs-stack: asserts NAMED FOLDS of the Jeff-facing
# pages' backing APIs render real values. Skip-if-absent when the stack is down.
#
# #1778 (pair kade+wren, 2026-08-20) — the assertion floor, as data:
#   * every check asserts a NAMED FOLD with real values
#   * a 401 or an empty shell FAILS — status codes never appear as asserts
#   * shape-assert where empty is VALID (board wip total:0 is a correct answer;
#     an idle team must not red the suite), value-assert where it is not.
set -u
API="${UI_SEAMS_API:-http://localhost:3340}"
PASS=0; FAIL=0

# stack gate — absent stack is SKIP (visible), never green-by-default (#3443)
if ! curl -sf --max-time 4 "$API/api/chorus/health" >/dev/null 2>&1; then
  echo "SKIP: chorus-api absent at $API — needs-stack suite not run"
  exit 0
fi

# fold <name> <url> <jq-expr that must be TRUE>
fold() {
  local name="$1" url="$2" expr="$3" body
  body=$(curl -sf --max-time 6 "$url" 2>/dev/null)
  if [ -z "$body" ]; then
    echo "FAIL: $name — $url returned nothing (auth wall or dead seam)"; FAIL=$((FAIL+1)); return
  fi
  if printf '%s' "$body" | jq -e "$expr" >/dev/null 2>&1; then
    echo "PASS: $name"; PASS=$((PASS+1))
  else
    echo "FAIL: $name — fold assert '$expr' false at $url"; FAIL=$((FAIL+1))
  fi
}

# /werk — value asserts: this page is meaningless with an empty board fold
fold "werk: board fold has real totals" \
  "$API/api/loom-metrics" '.board.total | numbers | . >= 0'
fold "werk: weekly_throughput key exists" \
  "$API/api/loom-metrics" 'has("weekly_throughput") or has("throughput")'
fold "werk: activity entries name their event" \
  "$API/api/werk/activity" '.entries | length > 0 and (.[0].event | length > 0)'

# /flow + /chorus — shape asserts (empty is VALID: an idle team is not a defect)
fold "flow/chorus: board wip is shaped (total number, cards array)" \
  "$API/api/chorus/context/board/wip" '(.data.total | type == "number") and (.data.cards | type == "array")'
# ...plus one value assert on a query that must have rows in any live system
fold "flow/chorus: roles fold names all three roles" \
  "$API/api/chorus/context/roles" '[.data.roles[]?.role // .roles[]?.role // empty] | length >= 3'

# NEGATIVE PROOF (#3734): the fold checker itself must fail on an empty shell
# and on an auth-walled body — otherwise every assert above is hollow.
neg_body='{"data":null}'
if printf '%s' "$neg_body" | jq -e '.data.total | type == "number"' >/dev/null 2>&1; then
  echo "FAIL: negative proof — empty shell passed a shape assert"; FAIL=$((FAIL+1))
else
  echo "PASS: negative proof — empty shell fails the shape assert"; PASS=$((PASS+1))
fi
if fold "negative: auth-walled endpoint must FAIL" "$API/api/definitely-not-a-route-1778" 'true' 2>/dev/null | grep -q PASS; then
  echo "FAIL: negative proof — dead endpoint read as pass"; FAIL=$((FAIL+1))
else
  echo "PASS: negative proof — dead endpoint fails loud"; PASS=$((PASS+1))
fi

echo "=== ui-seams: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
