#!/usr/bin/env bats
# @test-type: unit — static hermetic guard: parses the committed TTL with arq, no live service.
load test_helper
#
# #3697 — value-stream grammar reconcile. What Jeff sees: asking "what are the value
# streams and their steps?" gets ONE coherent answer. This test pins the reconciled
# state on the SOURCE of truth (the committed TTL), so drift can't creep back:
#   AC1  ValueStreamStep/inStream/stageOrder is the grammar; v1 Vertebra grammar deprecated.
#   AC2  Shaping has exactly one stageOrder; Chorus = 6 steps, orders 1-6, no dup/gap.
#   AC3  Reflecting is inStream chorus at order 6 (not gathering).
#   AC4  (bless is a card fact, not a code test.) FK: every step->stream inStream resolves.
#
# Canonical grammar lives in value-stream-instances.ttl (live in urn:chorus:instances).
# The v1 Vertebra ABox + its 3 drifts live in chorus.ttl and are owl:deprecated here;
# their PHYSICAL removal is #3702 (bundled with the readers), so this test asserts the
# deprecation marker now, not absence.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  VSI="$REPO/designing/data/value-stream-instances.ttl"
  CT="$REPO/roles/silas/ontology/chorus.ttl"
}

# Run a SPARQL query against a TTL file; echoes arq's output.
q() {
  local data="$1" query="$2" qf
  qf="$(mktemp "${BATS_TMPDIR:-/tmp}/q.XXXXXX.rq")"
  printf 'PREFIX c: <https://jeffbridwell.com/chorus#>\nPREFIX owl: <http://www.w3.org/2002/07/owl#>\n%s\n' "$query" > "$qf"
  arq --data="$data" --query="$qf" 2>/dev/null
  rm -f "$qf"
}

@test "grammar files exist" {
  [ -f "$VSI" ]
  [ -f "$CT" ]
}

# ── AC2: Shaping has exactly one stageOrder, and it is 1 ──
@test "AC2 canonical Shaping has exactly one stageOrder" {
  run q "$VSI" 'SELECT (COUNT(?o) AS ?n) WHERE { c:value-stream-step-shaping c:stageOrder ?o }'
  echo "$output" | grep -qE '\|[[:space:]]*1[[:space:]]*\|'
}
@test "AC2 canonical Shaping stageOrder is 1 (not the v1 dup 0/1)" {
  run q "$VSI" 'ASK { c:value-stream-step-shaping c:stageOrder 1 }'
  echo "$output" | grep -qi 'yes'
}

# ── AC2: Chorus stream = exactly 6 steps with 6 distinct orders (no dup, no gap) ──
@test "AC2 Chorus stream contains exactly 6 steps" {
  run q "$VSI" 'SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { ?s c:inStream c:value-stream-chorus }'
  echo "$output" | grep -qE '\|[[:space:]]*6[[:space:]]*\|'
}
@test "AC2 Chorus steps carry 6 distinct stageOrders (1-6, no duplicate)" {
  run q "$VSI" 'SELECT (COUNT(DISTINCT ?o) AS ?n) WHERE { ?s c:inStream c:value-stream-chorus ; c:stageOrder ?o }'
  echo "$output" | grep -qE '\|[[:space:]]*6[[:space:]]*\|'
}
@test "AC2 Chorus orders span 1..6 (min 1, max 6)" {
  run q "$VSI" 'SELECT (MIN(?o) AS ?lo) (MAX(?o) AS ?hi) WHERE { ?s c:inStream c:value-stream-chorus ; c:stageOrder ?o }'
  echo "$output" | grep -qE '\|[[:space:]]*1[[:space:]]*\|[[:space:]]*6[[:space:]]*\|'
}

# ── AC3: Reflecting is the 6th Chorus step, under chorus (not gathering) ──
@test "AC3 Reflecting is inStream chorus at stageOrder 6" {
  run q "$VSI" 'ASK { c:value-stream-step-reflecting c:inStream c:value-stream-chorus ; c:stageOrder 6 }'
  echo "$output" | grep -qi 'yes'
}
@test "AC3 Reflecting is NOT parented to any gathering stream" {
  run q "$VSI" 'ASK { c:value-stream-step-reflecting c:inStream c:gatheringStream }'
  echo "$output" | grep -qi 'no'
}

# ── AC1/FK: every step's inStream resolves to a declared ValueStream (no dangling) ──
@test "AC1 every ValueStreamStep inStream resolves to a declared ValueStream" {
  run q "$VSI" 'SELECT (COUNT(*) AS ?dangling) WHERE { ?s a c:ValueStreamStep ; c:inStream ?vs . FILTER NOT EXISTS { ?vs a c:ValueStream } }'
  echo "$output" | grep -qE '\|[[:space:]]*0[[:space:]]*\|'
}

# ── AC1: the v1 Vertebra grammar is deprecated (removal is #3702) ──
@test "AC1 Vertebra class is owl:deprecated" {
  run q "$CT" 'ASK { c:Vertebra owl:deprecated true }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 old chorusStream individual is owl:deprecated" {
  run q "$CT" 'ASK { c:chorusStream owl:deprecated true }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 old gatheringStream individual is owl:deprecated (drift b superseded)" {
  run q "$CT" 'ASK { c:gatheringStream owl:deprecated true }'
  echo "$output" | grep -qi 'yes'
}
@test "AC1 old lifeStream individual is owl:deprecated" {
  run q "$CT" 'ASK { c:lifeStream owl:deprecated true }'
  echo "$output" | grep -qi 'yes'
}
