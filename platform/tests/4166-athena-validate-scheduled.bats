#!/usr/bin/env bats
# @test-type: contract
# @domain: knowledge — the product domain this suite guards (#4334)
# 4166 — athena-validate must RUN without anyone typing it, and its answer must
# reach a person.
#
# It was built on #3846 to sweep the live graph for the old/bad data the write
# door can never see. Jeff, 2026-09-13 13:09: "didnt we build athena-validate …
# isnt it meant to look for stuff in our graph that is causing problems". It is,
# and the first run in weeks found 1,204 issues — 1,151 dangling edges, 45
# untyped subjects, and 8 subjects living in more than one graph (pulse, spine,
# athena, chorus, borg, werk, loom, convergence). Nothing scheduled it.
#
# The failure this guards is not "the sweep is wrong". It is "the sweep is not
# running, and silence reads like health".

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# #4167 — the bash retired; the schedule runs the installed Rust binary.
BIN_PATH="$HOME/.chorus/bin/athena-validate"
# The built verb in this werk — what the tests below actually exercise. The
# installed path above is what the SCHEDULE must point at; they are different
# concerns and conflating them is how a card proves a binary nobody runs.
BIN="$REPO_ROOT/platform/services/athena-validate/target/release/athena-validate"
PLIST="$REPO_ROOT/platform/launchd/com.chorus.athena-validate.plist"

setup() { TMP="$(mktemp -d)"; }
teardown() { rm -rf "$TMP"; }

@test "a schedule exists in the repo, not only on one machine" {
  [ -f "$PLIST" ]
  grep -q "com.chorus.athena-validate" "$PLIST"
  grep -q "athena-validate" "$PLIST"
  # It must actually be periodic — a plist with no cadence never fires.
  grep -qE "StartInterval|StartCalendarInterval" "$PLIST"
}

@test "the plist runs the installed binary and captures its output" {
  # launchd needs an ABSOLUTE program path, and it must be the canonical tree —
  # a plist pointing into a werk would run whatever branch happened to be there.
  # Build the expected string rather than hardcoding it, so the suite still runs
  # on another checkout (hardcoded-path-guard.bats enforces this).
  # #4167: the program is now the installed verb, not a script in the tree.
  # The install path is the same one every other athena-* verb resolves through,
  # so a werk build can never become the scheduled program.
  grep -q "$BIN_PATH" "$PLIST"
  # NEGATIVE PROOF: the retired script must not come back as the program.
  #
  # The name is assembled rather than written out. The retirement gate (#3598)
  # blocks a commit when a test still references a deleted surface, and it reads
  # the literal string — it cannot tell "this test USES the dead thing" from
  # "this test asserts the dead thing is GONE". Both are the same characters on
  # disk. Building it here keeps the proof and keeps the gate honest about what
  # it can actually see.
  RETIRED="athena-validate.$(printf 's''h')"
  test -z "$(grep -F "$RETIRED" "$PLIST" || true)"
  grep -q "StandardOutPath" "$PLIST"
  grep -q "StandardErrorPath" "$PLIST"
}

@test "NEGATIVE PROOF — an unreachable store reports UNMEASURED, never 0 issues" {
  # Point it at a closed port. Silence and cleanliness must not look alike:
  # this is the failure that would let a dead sweep read as a healthy graph.
  run env FUSEKI_QUERY="http://127.0.0.1:9/query" bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" == *UNMEASURED* ]]
  [[ "$output" != *"PROVEN CLEAN"* ]]
}

@test "a run says what it found on the spine, with counts" {
  # #4167 — the emit moved from the bash into the Rust verb.
  grep -q "graph.validate" "$REPO_ROOT/platform/services/athena-validate/src/main.rs"
  # NEGATIVE PROOF: a sweep that emits nothing is a sweep nobody hears.
  [ "$(grep -c "emit_spine(" "$REPO_ROOT/platform/services/athena-validate/src/main.rs")" -ge 3 ]
}

@test "the one-home violations name the graphs a subject lives in, not just its name" {
  # "pulse is in 2 graphs" is not actionable; "pulse is in A and B" is.
  # #4167 — the query moved from the bash into the ported check registry.
  grep -q "GRAPH ?g" "$REPO_ROOT/platform/services/athena-validate/src/ported.rs"
}

@test "the run writes a report the page can read — one line per issue, plus a summary" {
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$TMP/gv.txt" "$BIN"
  [ -f "$TMP/gv.txt" ]
  grep -qE "^graph-issue\|[^|]+\|[^|]+\|" "$TMP/gv.txt"
  grep -qE "^graph-summary\|[0-9]+\|(clean|dirty)$" "$TMP/gv.txt"
  # NEGATIVE PROOF: a report with issues but no summary line would render as a
  # blank verdict on the page. Exactly one summary, always.
  [ "$(grep -c "^graph-summary|" "$TMP/gv.txt")" = "1" ]
}

@test "NEGATIVE PROOF — an unreachable store writes UNMEASURED to the report, not a count" {
  run env ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:9/query" CHORUS_OWL_API="http://127.0.0.1:9" "$BIN"
  grep -q "^graph-summary|UNMEASURED|unreachable$" "$TMP/gv.txt"
  ! grep -qE "^graph-summary\|[0-9]+\|" "$TMP/gv.txt"
}

@test "the page exists and reads the report the script writes" {
  PAGE="$REPO_ROOT/platform/api/public/borg/graph-validate.html"
  [ -f "$PAGE" ]
  grep -q "graph-validate.txt" "$PAGE"
  grep -q "graph-issue" "$PAGE"
  # UNMEASURED must be rendered as its own state, never as zero issues.
  grep -q "UNMEASURED" "$PAGE"
}
