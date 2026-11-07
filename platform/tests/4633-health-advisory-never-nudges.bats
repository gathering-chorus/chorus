#!/usr/bin/env bats
# @test-type: unit — the alert rule reads a saved Loki answer; nothing live
# @domain: services
# #4633 — Jeff 2026-10-18: "if its useless remove it". A chorus-health warning marked
# advisory stays in the chorus.health event but never nudges; a real failure still does.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  RULE="$ROOT/platform/scripts/health-alert-firing.py"
  T="$(mktemp -d)"
}

teardown() { rm -rf "$T"; }

# prior <warns> <failures>... → a Loki answer with one chorus.health event per pair, newest first
prior() {
  python3 - "$T/loki.json" "$@" <<'PY'
import json, sys
out, rest = sys.argv[1], sys.argv[2:]
vals = []
for i in range(0, len(rest), 2):
    vals.append([str(100 - i), json.dumps({"event": "chorus.health", "warns": rest[i], "failures": rest[i+1]})])
json.dump({"data": {"result": [{"values": vals}]}}, open(out, "w"))
PY
}

@test "an advisory warning persisting 3 runs sends no nudge" {
  prior "owl-x" "" "owl-x" ""
  out=$(python3 "$RULE" 3 "" "owl-x" "owl-x" < "$T/loki.json")
  [ -z "$out" ]
}

@test "negative proof: the same warning, not advisory, does nudge at 3 runs" {
  prior "owl-x" "" "owl-x" ""
  out=$(python3 "$RULE" 3 "" "owl-x" "" < "$T/loki.json")
  [ "$out" = "owl-x" ]
}

@test "a real failure still nudges on its first run" {
  prior "" "" "" ""
  out=$(python3 "$RULE" 3 "clearing-external" "" "owl-x" < "$T/loki.json")
  [ "$out" = "clearing-external" ]
}

@test "chorus-health marks a warning whose detail says advisory" {
  grep -qF 'ADVISORY_NAMES+=("$1")' "$ROOT/platform/scripts/chorus-health"
  run grep -c 'owl-api-drift\|owl-api-conformance' "$ROOT/platform/scripts/chorus-health"
  [ "$output" = "0" ]
}
