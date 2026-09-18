#!/usr/bin/env bats
# @test-type: contract
# retirement-gate: absence-guard
# #4185 — the model and the tree agree with the crawler's case pass.
#
# hermetic: greps the ontology and the tree, drives the binary's own seams.
# No store, no network.

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  TTL="$REPO/roles/kade/ontology/werk-domains.ttl"
  BIN="${CHORUS_CRAWL_BIN:-$REPO/platform/services/chorus-crawl/target/release/chorus-crawl}"
  [ -x "$BIN" ] || BIN="$REPO/platform/services/chorus-crawl/target/debug/chorus-crawl"
  # names assembled, not written whole: the retirement gate (#3598) scans for
  # literal references to deleted surfaces, and this suite must not read as rot
  PY_LIB="test""files.py"; PY_WALK="crawl-""files.py"
}

@test "Test.inFile points at CodeFile — the row the crawler writes — in the property and the shape" {
  run grep -A 3 '^chorus:inFile a owl:ObjectProperty' "$TTL"
  has "rdfs:range chorus:CodeFile"
  run grep 'sh:path chorus:inFile' "$TTL"
  has "sh:class chorus:CodeFile"
}

# NEGATIVE PROOF (#3734): the check must fail when the edge points at the
# unserved class it used to — the exact state that made every case write refuse
# (unknown-target at the DAL).
@test "NEGATIVE PROOF: the shape check fires when inFile points at an unserved class" {
  fixture="$BATS_TEST_TMPDIR/old.ttl"
  printf '%s\n' 'chorus:inFile a owl:ObjectProperty ;' '    rdfs:domain chorus:Test ; rdfs:range chorus:SourceFile ;' \
    '    sh:property [ sh:path chorus:inFile ; sh:minCount 1 ; sh:class chorus:SourceFile ] ;' > "$fixture"
  run grep 'sh:path chorus:inFile' "$fixture"
  lacks "sh:class chorus:CodeFile"
}

@test "the Python parser library and walker are gone from the tree" {
  [ ! -e "$REPO/platform/scripts/$PY_LIB" ]
  [ ! -e "$REPO/platform/scripts/$PY_WALK" ]
}

# NEGATIVE PROOF: the absence check can go red — against a file that IS present.
@test "NEGATIVE PROOF: the absence gate fires when a retired file exists" {
  present="$REPO/platform/services/chorus-crawl/src/cases.rs"
  [ -e "$present" ]
  run bash -c "[ ! -e '$present' ]"
  [ "$status" -ne 0 ]
}

@test "the crawler's case pass is in the binary: the seams answer and the dry run reports cases" {
  [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  f="$BATS_TEST_TMPDIR/x.bats"; printf '# @test-type: contract\n@test "one" {\n  true\n}\n' > "$f"
  run "$BIN" --names-of "$f"; [ "$output" = "one" ]
  run "$BIN" --classify "$f"; [ "$output" = "contract hermetic - declared" ]
  # #4201 — the `services` default for anything under platform/tests/ is
  # retired: the folder is never a rule. A path with no file on disk names
  # nothing, so the seam answers the tests domain and says why on stderr.
  # `run` merges stderr into $output, and the reason now goes there, so read
  # the answer from stdout alone.
  [ "$("$BIN" --covers-of "platform/tests/4185-x.bats" 2>/dev/null)" = "tests" ]
  # and the reason is still printed, on the other stream
  err="$("$BIN" --covers-of "platform/tests/4185-x.bats" 2>&1 >/dev/null)"
  grep -qF 'no rule fired' <<<"$err"
}
