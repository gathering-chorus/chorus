#!/usr/bin/env bats
# @test-type: integration — live owl-api serve checks (skip only when the service is
# absent, #3528) PLUS hermetic red-proofs that the assertions themselves refuse empty.
load test_helper
#
# #3701 — anti-false-green. What Jeff sees: /valuestream is trustworthy because this
# suite goes RED before the page ships empty. The #3190 rule: a test that passes on
# zero rows IS the bug. Two distinct conditions, two behaviors:
#   service ABSENT (unreachable)      → skip  (env absence, hermetic contract #3528)
#   service UP but graph/serve EMPTY  → FAIL  (never skip, never pass)
# The werk pipeline always has the variant up, so emptiness cannot ride a skip there.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
}

# The row-count and tree-depth extractors the live tests ride. Kept as functions so
# the AC4 red-proofs below exercise the SAME logic against empty payloads.
envelope_count() { # stdin: owl-api envelope JSON → count
  python3 -c "import sys,json; print(json.load(sys.stdin).get('count',0))" 2>/dev/null || echo 0
}
tree_depth() { # stdin: tree JSON → max depth (1 = childless root = empty tree)
  python3 -c "
import sys,json
d=json.load(sys.stdin)
def md(n):
    ch=n.get('children') or []
    return 1+max((md(c) for c in ch), default=0)
print(md(d))
" 2>/dev/null || echo 0
}

# ── AC4 (hermetic): the assertions go RED on emptiness — proven every run ──
@test "AC4 row assertion FAILS on a count=0 envelope (red-on-empty, proven)" {
  local n; n=$(echo '{"count":0,"data":[]}' | envelope_count)
  run test "${n:-0}" -ge 1
  [ "$status" -ne 0 ]
}
@test "AC4 row assertion FAILS on a malformed/blank body (no false-green via parse error)" {
  local n; n=$(echo 'not-json' | envelope_count)
  run test "${n:-0}" -ge 1
  [ "$status" -ne 0 ]
}
@test "AC4 tree assertion FAILS on a childless root (empty tree = red)" {
  local d; d=$(echo '{"name":"value-stream-chorus"}' | tree_depth)
  run test "${d:-0}" -ge 2
  [ "$status" -ne 0 ]
}

# ── AC1 (live): /valuestreams serves >=1 row — empty = RED ──
@test "AC1 GET /valuestreams returns >=1 stream (0 rows on a live service = FAIL)" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$OWL_URL/valuestreams"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$OWL_URL/valuestreams"
  local n; n=$(echo "$output" | envelope_count)
  [ "${n:-0}" -ge 1 ]
}

# ── AC1 (live): the recursive tree is non-empty — childless root = RED ──
@test "AC1 GET /valuestreams/value-stream-chorus/tree returns a non-empty tree" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$OWL_URL/valuestreams/value-stream-chorus/tree"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$OWL_URL/valuestreams/value-stream-chorus/tree"
  local d; d=$(echo "$output" | tree_depth)
  [ "${d:-0}" -ge 2 ]
}
