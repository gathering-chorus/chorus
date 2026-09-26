#!/usr/bin/env bats
# @test-type: unit
# @domain: spine — the product domain this suite guards (#4334)
# #4195 — an athena run is one trace that is only athena. Three proofs on the file and
# the binary, no live stack: (1) the land job mints its own trace by path-resolved
# chorus-log and swallows no emit; (2) prove-trace refuses a run that left fewer
# events than legs (NEGATIVE PROOF); (3) a werk line never counts toward an athena trace.
# Simple commands only: a failing [[ off the last line passes on bash 3.2.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
YML="$ROOT/.github/workflows/athena.yml"
BIN="$ROOT/platform/services/athena-deploy/target/release/athena-deploy"

setup() { T="$(mktemp -d)"; }
teardown() { rm -rf "$T"; }

@test "the land job mints its own trace and records the caller's as parent, never reuses it" {
  land="$(sed -n '/^  land:/,$p' "$YML")"
  printf '%s\n' "$land" | grep -q 'trace="athena-\$(date +%s%N)-\$\$"'
  printf '%s\n' "$land" | grep -q 'ATHENA_PARENT_TRACE=\$parent'
  printf '%s\n' "$land" | grep -q 'parent="\${ATHENA_PARENT_TRACE:-none}"'
}

@test "every emit in the land job goes by path and none is swallowed (NEGATIVE PROOF on the file)" {
  land="$(sed -n '/^  land:/,$p' "$YML")"
  n=$(printf '%s\n' "$land" | grep -c '"\$CHORUS_LOG" athena\.')
  [ "$n" -ge 4 ]
  # the violation: a bare `chorus-log` or an emit ending in `|| true` — zero of each
  bare=$(printf '%s\n' "$land" | grep -cE '^\s+chorus-log ' || true)
  [ "$bare" -eq 0 ]
  # an emit LINE ending in `|| true` (inner `$(... || true)` subshells on the same line are not the emit's exit)
  swallowed=$(printf '%s\n' "$land" | grep -E 'CHORUS_LOG.*\|\| true[[:space:]]*$' | grep -vc '^[[:space:]]*#' || true)
  [ "$swallowed" -eq 0 ]
  printf '%s\n' "$land" | grep -q 'athena-deploy prove-trace "\$CHORUS_TRACE_ID"'
  # a run that cannot emit does not run: the trace step refuses by name when the emitter is missing
  printf '%s\n' "$land" | grep -q '\[ -x "\$log" \] || { echo "::error::athena: no spine emitter at \$log'
}

@test "NEGATIVE PROOF — prove-trace refuses a run that left fewer events than legs, by name" {
  [ -x "$BIN" ]
  printf '{"event":"athena.pipeline.started","trace":"athena-7-7"}\n{"event":"werk.started","trace":"werk-1"}\n' > "$T/spine.log"
  run "$BIN" prove-trace athena-7-7 2 --spine "$T/spine.log"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "1 event(s) on trace athena-7-7 but 2 leg(s) ran"
  run "$BIN" prove-trace athena-7-7 2 --spine "$T/absent.log"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "not readable"
}

@test "a run whose legs all left a record is traceable, and werk lines do not count toward it" {
  printf '{"event":"athena.pipeline.started","trace":"athena-7-7"}\n{"event":"merge.landed","trace":"werk-1"}\n{"event":"athena.pipeline.completed","trace_id":"athena-7-7"}\n' > "$T/spine.log"
  run "$BIN" prove-trace athena-7-7 2 --spine "$T/spine.log"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "2 event(s) on athena-7-7"
}

@test "the trace reader tells werk rows from athena rows: a pipeline pill per row, traces counted per pipeline" {
  # Jeff 2026-09-17 07:27: "if a card involves athena and werk i expect to see both traces via card;
  # visually how do i differentiate werk and athena rows". Shape proof on the served file.
  H="$ROOT/platform/api/public/borg/trace.html"
  grep -q "function pipelineOf(ev, fullTrace)" "$H"
  grep -q "startsWith('athena-')" "$H"
  grep -q 'PIPE_COLOR\[e.pipe\]' "$H"
  grep -q 'byPipe.athena.size' "$H"
  # NEGATIVE PROOF: an event outside the athena vocabulary on a non-athena trace is werk, never 'other'
  n=$(node -e "
    const src=require('fs').readFileSync('$H','utf8');
    const m=src.match(/function pipelineOf[\s\S]*?\n}/)[0];
    const pipelineOf=new Function(m+'; return pipelineOf;')();
    const r=[pipelineOf('merge.landed','1789595964146650000-65927'),pipelineOf('model.seed.posted','1789595964146650000-65927'),pipelineOf('werk.started','athena-1-2')];
    console.log(r.join(','))")
  [ "$n" = "werk,athena,athena" ]
}
