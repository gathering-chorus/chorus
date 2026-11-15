#!/usr/bin/env bats
# @test-type: unit — fixtures for the verdict; the live case reads prod Prometheus after the land
# @domain: persistence
# @card: #4920 · owner: silas
# Checks designing/schemas/shacl-remainder-4920.json and platform/scripts/land-4920-rows.
#
# #4920 AC2 (Jeff 2026-11-15 14:06): zero repairable violations; what remains is named by class,
# field, constraint and owner. Every census line (athena_shacl_violations_by_field > 0) must be
# in the named remainder; a line that is not is unrepaired damage and fails.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  NAMED="$ROOT/designing/schemas/shacl-remainder-4920.json"
}

# Prometheus /api/v1/query JSON on stdin; prints each unnamed line, exits 1 if any
verdict() {
  python3 -c '
import json, sys
named = {(n["class"], n["path"], n["component"]) for n in json.load(open(sys.argv[1]))["named"]}
bad = 0
for r in json.load(sys.stdin)["data"]["result"]:
    m = r["metric"]; k = (m.get("class",""), m.get("path",""), m.get("component","").replace("ConstraintComponent",""))
    if float(r["value"][1]) > 0 and k not in named:
        print("UNNAMED %s.%s %s %s" % (k[0], k[1] or "-", k[2], r["value"][1])); bad = 1
sys.exit(bad)
' "$NAMED"
}

line() { printf '{"metric":{"class":"%s","path":"%s","component":"%sConstraintComponent"},"value":[0,"%s"]}' "$1" "$2" "$3" "$4"; }

@test "every named line has an owner and a reason" {
  python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))["named"]
assert d, "nothing named"
for n in d:
    assert n.get("owner") and n.get("reason"), n
' "$NAMED"
}

@test "census lines that are all named read clean" {
  run bash -c "$(declare -f verdict line); NAMED='$NAMED'; printf '{\"data\":{\"result\":[%s,%s]}}' \"\$(line Delivery '' SPARQL 267)\" \"\$(line Value expressedBy Class 15)\" | verdict"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: a line this card fixed (or any new damage) is not named, so it fails
@test "an unnamed line fails, naming it; a zero line is ignored" {
  run bash -c "$(declare -f verdict line); NAMED='$NAMED'; printf '{\"data\":{\"result\":[%s]}}' \"\$(line Session '' SPARQL 123)\" | verdict"
  [ "$status" -eq 1 ] && [[ "$output" == *"UNNAMED Session.- SPARQL 123"* ]] || { echo "$output"; return 1; }
  run bash -c "$(declare -f verdict line); NAMED='$NAMED'; printf '{\"data\":{\"result\":[%s]}}' \"\$(line Session '' SPARQL 0)\" | verdict"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "the land runs the census once; a failed census refuses the land; a dry run runs nothing" {
  printf '#!/bin/sh\necho census\nexit "${CENSUS_RC:-0}"\n' > "$BATS_TEST_TMPDIR/census"; chmod +x "$BATS_TEST_TMPDIR/census"
  printf 'athena_shacl_violations_by_field{class="Delivery",path="",component="SPARQLConstraintComponent"} 267\n' > "$BATS_TEST_TMPDIR/athena_shacl.prom"
  TEXTFILE_DIR="$BATS_TEST_TMPDIR" SHACL_CENSUS="$BATS_TEST_TMPDIR/census" run python3 "$ROOT/platform/scripts/land-4920-rows" --apply
  [ "$status" -eq 0 ] && [[ "$output" == *"all is named"* || "$output" == *"every census line is in the named remainder"* ]] || { echo "$output"; return 1; }
  # NEGATIVE PROOF: an unnamed census line after the land refuses it, naming the line
  printf 'athena_shacl_violations_by_field{class="Session",path="",component="SPARQLConstraintComponent"} 123\n' > "$BATS_TEST_TMPDIR/athena_shacl.prom"
  TEXTFILE_DIR="$BATS_TEST_TMPDIR" SHACL_CENSUS="$BATS_TEST_TMPDIR/census" run python3 "$ROOT/platform/scripts/land-4920-rows" --apply
  [ "$status" -eq 1 ] && [[ "$output" == *"Session.- SPARQL 123"* ]] || { echo "$output"; return 1; }
  CENSUS_RC=1 TEXTFILE_DIR="$BATS_TEST_TMPDIR" SHACL_CENSUS="$BATS_TEST_TMPDIR/census" run python3 "$ROOT/platform/scripts/land-4920-rows" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  SHACL_CENSUS=/nonexistent run python3 "$ROOT/platform/scripts/land-4920-rows"
  [ "$status" -eq 0 ] && [[ "$output" == plan:* ]] || { echo "$output"; return 1; }
}
