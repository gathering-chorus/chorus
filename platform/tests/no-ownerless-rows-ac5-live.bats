#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door, read-only (negative proofs run anywhere)
# @domain: domains
# @card: #5002
#
# #5002 AC5 — the quartet test principal holds Append and no Write row. Split from no-ownerless-rows-live.bats so each AC names its own check.
# Skips in a card's werk: it measures what landed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

quartet_append_only() {
  python3 - "$1" <<'PY'
import json, sys
rows = [r for r in json.loads(sys.argv[1]).get("data", []) if (r.get("agent") or "").endswith("principal-quartet")]
w = [r.get("name") for r in rows if (r.get("mode") or "").endswith("Write")]
a = [r for r in rows if (r.get("mode") or "").endswith("Append")]
print(f"quartet: {len(a)} Append, {len(w)} Write" + (": " + ", ".join(w) if w else ""))
sys.exit(1 if w or not a else 0)
PY
}

@test "AC5 the quartet test principal holds Append and no Write row" {
  in_werk && skip "after-land check: a card's werk has not landed"
  run quartet_append_only "$(curl -sf --max-time 20 "$DOOR/v1/security/permissions?limit=1000")"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC5 NEGATIVE PROOF: a quartet Write row is red, no Append is red, Append alone passes" {
  A='{"name":"q-a","agent":"chorus:principal-quartet","mode":"Append"}'
  W='{"name":"quartet-write-values","agent":"chorus:principal-quartet","mode":"Write"}'
  run quartet_append_only "{\"data\":[$A]}"
  [ "$status" -eq 0 ] || return 1
  run quartet_append_only "{\"data\":[$A,$W]}"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"quartet-write-values"* ]] || return 1
  run quartet_append_only '{"data":[]}'
  [ "$status" -eq 1 ] || return 1
}
