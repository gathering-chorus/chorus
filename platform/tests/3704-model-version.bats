#!/usr/bin/env bats
# @test-type: integration — hermetic TTL guards (arq) + live owl-api projection check
# (service-hitting, skip-if-absent per #3528).
load test_helper
#
# #3704 — model-version convention: v1/v2 as DECLARED DATA. What Jeff (or an agent,
# page, retire-guard) sees: ask a class "canonical or superseded?" and get a
# machine-readable answer instead of guessing which grammar is current. RULE:
# chorus:modelVersion absent = v2 (born-v2 default); a superseded class carries
# "v1" + chorus:supersededBy <successor>. owl-api PROJECTS modelVersion on every
# envelope so the tag is visible, not just declared.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CT="$REPO/roles/silas/ontology/chorus.ttl"
  OWL_URL="${OWL_URL:-http://localhost:3360}"
}

aq() {
  local qf; qf="$(mktemp "${BATS_TMPDIR:-/tmp}/mv.XXXXXX.rq")"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\n%s\n' "$1" > "$qf"
  arq --data="$CT" --query="$qf" 2>/dev/null; rm -f "$qf"
}

@test "arq SPARQL engine is present (no false-green from a missing binary)" {
  command -v arq
}

# ── AC1: the convention properties are DEFINED ──
@test "AC1 chorus:modelVersion is declared an owl:AnnotationProperty" {
  run aq 'ASK { c:modelVersion a owl:AnnotationProperty }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 chorus:supersededBy is declared an owl:AnnotationProperty" {
  run aq 'ASK { c:supersededBy a owl:AnnotationProperty }'
  echo "$output" | grep -qi 'yes'
}

# ── AC2: the first v1 class is tagged + points at its successor ──
@test "AC2 Vertebra carries modelVersion v1 and supersededBy ValueStreamStep" {
  run aq 'ASK { c:Vertebra c:modelVersion "v1" ; c:supersededBy c:ValueStreamStep }'
  echo "$output" | grep -qi 'yes'
}
# born-v2 default: a live/canonical class must NOT be tagged v1 (absence = v2).
@test "AC2 a canonical class (ValueStreamStep) is NOT tagged v1" {
  run aq 'ASK { c:ValueStreamStep c:modelVersion "v1" }'
  echo "$output" | grep -qi 'no'
}

# ── AC3: the report query — every v1 class with its successor, machine-readable ──
@test "AC3 v1-classes report query returns Vertebra→ValueStreamStep" {
  run aq 'SELECT ?cl ?successor WHERE { ?cl c:modelVersion "v1" ; c:supersededBy ?successor }'
  echo "$output" | grep -q 'Vertebra'
  echo "$output" | grep -q 'ValueStreamStep'
}

# ── owl-api projection (live): every envelope carries modelVersion (born-v2 default) ──
@test "owl-api projects modelVersion on the generated envelope" {
  run curl -s --max-time 8 "$OWL_URL/domains"
  [ "$status" -eq 0 ] || skip "owl-api not reachable at $OWL_URL"
  echo "$output" | grep -q '"modelVersion"'
}
