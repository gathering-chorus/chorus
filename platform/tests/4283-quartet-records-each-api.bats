#!/usr/bin/env bats
# @test-type: unit — a stub spine writer captures the records; nothing live is touched
# @domain: tests — the product domain this suite guards (#4334)
# #4283 — one result per generated API per owner. NEGATIVE PROOF: a walk with
# one failing class produces exactly one failing record naming it.
# #4579 — a record is a `quartet.result` spine event, never a TestResult row
# (Jeff 10-17: "deprecate logging in the graph").

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  printf '#!/bin/bash\nprintf "%%s\\n" "$*" >> %s/events\n' "$T" > "$T/chorus-log"
  export QUARTET_LOG="$T/chorus-log"
  source "$ROOT/platform/tests/lib/quartet-record.sh"
}

recorded() { grep -c '^quartet.result system ' "$T/events" 2>/dev/null || echo 0; }
where() { grep '^quartet.result system ' "$T/events" 2>/dev/null | grep -c -- " $1" || true; }

@test "#4283 one walk of three classes with one failure records three results, exactly one failing and it names the class" {
  quartet_record api tok kade Nudge PASS "create read update delete, fields survived, no residue"
  quartet_record api tok kade Page  FAIL "create POST wanted 201 got 409"
  quartet_record api tok kade Role  PASS "ok"
  [ "$(recorded)" -eq 3 ]
  [ "$(where 'result=fail')" -eq 1 ]
  grep -q 'owner=kade class=Page result=fail file=platform/tests/4279-api-quartet-prod.bats detail=create POST wanted 201 got 409' "$T/events"
}

@test "#4283 NOT-PERM and UNMEASURED verdicts are recorded as unmeasured, never as pass or fail" {
  quartet_record api tok wren Principal NOT-PERM "deploy-only"
  quartet_record api tok wren LogSource UNMEASURED "no row to point a required edge at"
  [ "$(recorded)" -eq 2 ]
  [ "$(where 'result=unmeasured')" -eq 2 ]
  [ "$(where 'result=pass')" -eq 0 ]
  [ "$(where 'result=fail')" -eq 0 ]
}

@test "#4579 negative proof: a record never posts a TestResult row" {
  # any HTTP call would need curl; a curl on PATH that records itself proves none is made
  mkdir -p "$T/bin"; printf '#!/bin/bash\necho "$*" >> %s/curl\n' "$T" > "$T/bin/curl"; chmod +x "$T/bin/curl"
  PATH="$T/bin:$PATH" quartet_record http://127.0.0.1:9 tok silas Metric PASS ok "https://jeffbridwell.com/chorus#test-x"
  [ ! -e "$T/curl" ]
  [ "$(recorded)" -eq 1 ]
}

@test "#4579 a spine that refuses the record is printed, never fatal" {
  printf '#!/bin/bash\nexit 1\n' > "$T/chorus-log"
  run quartet_record api tok kade Nudge PASS ok
  [ "$status" -eq 0 ]
  [[ "$output" == *"spine refused, not recorded"* ]] || return 1
}

@test "#4283 QUARTET_RECORD=0 records nothing (the seam the werk-only runs use)" {
  QUARTET_RECORD=0 quartet_record api tok kade Nudge PASS ok
  [ "$(recorded)" -eq 0 ]
}
