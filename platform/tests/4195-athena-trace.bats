#!/usr/bin/env bats
# @test-type: unit
# @domain: spine — the product domain this suite guards (#4334)
# #4195 — an athena run is one trace that is only athena. Proofs that RUN the code,
# no live stack: (1) the land job's own steps, pulled out of athena.yml and executed
# with stub emitters (#4336 — these were greps of the workflow text): the trace step
# mints its own trace and records the caller's as parent; every emitting step fails
# when its emit fails (none swallowed) and reaches the emitter by path; (2) prove-trace
# refuses a run that left fewer events than legs (NEGATIVE PROOF); (3) a werk line never
# counts toward an athena trace; (4) the trace reader page, executed in node against a
# fixture event list, labels and counts werk and athena rows apart.
# Simple commands only: a failing [[ off the last line passes on bash 3.2.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
YML="$ROOT/.github/workflows/athena.yml"
BIN="$ROOT/platform/services/athena-deploy/target/release/athena-deploy"
FIX="$ROOT/platform/tests/fixtures/4336"

setup() {
  T="$(mktemp -d "$BATS_TEST_TMPDIR/t.XXXXXX")"
  mkdir -p "$T/bin" "$T/home/platform/scripts" "$T/h/.chorus/bin"
  : > "$T/gh.env"; : > "$T/gh.out"; : > "$T/spine.log"
  # the spine emitter the land job resolves BY PATH under CHORUS_HOME. It records
  # every call as a spine line on the trace it was handed, and exits STUB_LOG_RC.
  printf '%s\n' '#!/bin/bash' \
    'printf "{\"event\":\"%s\",\"role\":\"%s\",\"trace\":\"%s\",\"args\":\"%s\"}\n" "$1" "$2" "$CHORUS_TRACE_ID" "${*:3}" >> "$CHORUS_SPINE"' \
    'exit "${STUB_LOG_RC:-0}"' > "$T/home/platform/scripts/chorus-log"
  : > "$T/home/platform/scripts/fuseki-auth.sh"
  printf '#!/bin/bash\necho tok\n' > "$T/home/platform/scripts/chorus-identity-token"
  # athena-deploy: `scope` answers from STUB_SCOPE; every other verb is the real binary
  printf '%s\n' '#!/bin/bash' \
    'if [ "$1" = scope ]; then printf "%b" "${STUB_SCOPE:-}"; exit 0; fi' \
    'if [ "$#" -eq 0 ]; then exit 0; fi' \
    "exec \"$BIN\" \"\$@\"" > "$T/bin/athena-deploy"
  printf '#!/bin/bash\nexit 0\n' > "$T/bin/athena-serve"
  printf '#!/bin/bash\nexit 0\n' > "$T/h/.chorus/bin/athena-model"
  chmod +x "$T/home/platform/scripts/chorus-log" "$T/home/platform/scripts/chorus-identity-token" \
    "$T/bin/athena-deploy" "$T/bin/athena-serve" "$T/h/.chorus/bin/athena-model"
}

# run_step <step> [expr=value ...] — extract the land job's step from athena.yml and
# run it the way a runner does (bash -e), with the env earlier steps wrote to
# GITHUB_ENV. PATH has no chorus-log: an emit that is not by path cannot land.
# Extra env for the step rides in STEP_ENV (an array of KEY=value).
run_step() {
  local name="$1"; shift
  ruby "$FIX/gha-step.rb" "$YML" land "$name" "$@" > "$T/step.sh" || return 1
  local envs=()
  while IFS= read -r l; do [ -n "$l" ] && envs+=("$l"); done < "$T/gh.env"
  run env -i PATH="$T/bin:/usr/bin:/bin" HOME="$T/h" CHORUS_HOME="$T/home" \
    GITHUB_ENV="$T/gh.env" GITHUB_OUTPUT="$T/gh.out" CHORUS_SPINE="$T/spine.log" \
    CARD_ID=4195 ROLE=kade TARGET=canonical LANDED=abc123 \
    "${envs[@]}" ${STEP_ENV[@]+"${STEP_ENV[@]}"} bash -e "$T/step.sh"
}

RESOLVE=()
resolve_args() {
  RESOLVE=("steps.resolve.outputs.root=$T/home" "steps.resolve.outputs.store=http://127.0.0.1:9/pods" \
           "steps.resolve.outputs.api=http://127.0.0.1:9" "steps.resolve.outputs.label=com.test.none")
}

@test "the land job mints its own trace and records the caller's as parent, never reuses it" {
  run_step trace inputs.parent_trace=werk-parent-1
  [ "$status" -eq 0 ]
  grep -qE '^CHORUS_TRACE_ID=athena-[0-9N]+-[0-9]+$' "$T/gh.env"
  grep -qx 'ATHENA_PARENT_TRACE=werk-parent-1' "$T/gh.env"
  grep -qx "CHORUS_LOG=$T/home/platform/scripts/chorus-log" "$T/gh.env"
  run grep -qx 'CHORUS_TRACE_ID=werk-parent-1' "$T/gh.env"
  [ "$status" -ne 0 ]
  # the parent rides athena.pipeline.started, on the NEW trace
  resolve_args
  STEP_ENV=("STUB_SCOPE=model|m.ttl\n")
  run_step scope "${RESOLVE[@]}"
  [ "$status" -eq 0 ]
  trace=$(sed -n 's/^CHORUS_TRACE_ID=//p' "$T/gh.env")
  grep -q "\"event\":\"athena.pipeline.started\",\"role\":\"kade\",\"trace\":\"$trace\".*parent=werk-parent-1" "$T/spine.log"
}

@test "with no parent input, the caller's CHORUS_TRACE_ID becomes the parent and a fresh trace is minted" {
  STEP_ENV=(CHORUS_TRACE_ID=werk-from-env-9)
  run_step trace inputs.parent_trace=
  [ "$status" -eq 0 ]
  grep -qx 'ATHENA_PARENT_TRACE=werk-from-env-9' "$T/gh.env"
  grep -qE '^CHORUS_TRACE_ID=athena-' "$T/gh.env"
}

@test "a run that cannot emit does not run: the trace step refuses by name when the emitter is missing" {
  rm "$T/home/platform/scripts/chorus-log"
  run_step trace inputs.parent_trace=
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "no spine emitter at $T/home/platform/scripts/chorus-log"
  run grep -q CHORUS_TRACE_ID "$T/gh.env"
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF — every emitting step fails when its emit fails; none is swallowed, all go by path" {
  run_step trace inputs.parent_trace=
  [ "$status" -eq 0 ]
  resolve_args
  complete=("steps.scope.outputs.nothing=" "steps.prove.outcome=success" "steps.prove.outputs.issues=0")
  # emitter healthy: each step succeeds and leaves its event (by path — PATH has no chorus-log)
  STEP_ENV=("STUB_SCOPE=model|m.ttl\nseed|s.ttl\n")
  run_step scope "${RESOLVE[@]}";  [ "$status" -eq 0 ]
  STEP_ENV=()
  run_step deploy "${RESOLVE[@]}"; [ "$status" -eq 0 ]
  run_step seed "${RESOLVE[@]}";   [ "$status" -eq 0 ]
  run_step complete "${complete[@]}"; [ "$status" -eq 0 ]
  for ev in athena.pipeline.started athena.deploy.started athena.seed.completed athena.pipeline.completed; do
    grep -q "\"event\":\"$ev\"" "$T/spine.log"
  done
  # emitter failing: every one of those steps goes red
  STEP_ENV=("STUB_SCOPE=model|m.ttl\n" STUB_LOG_RC=1)
  run_step scope "${RESOLVE[@]}";  [ "$status" -ne 0 ]
  STEP_ENV=(STUB_LOG_RC=1)
  run_step deploy "${RESOLVE[@]}"; [ "$status" -ne 0 ]
  run_step seed "${RESOLVE[@]}";   [ "$status" -ne 0 ]
  run_step complete "${complete[@]}"; [ "$status" -ne 0 ]
}

@test "the run proves its own trace: started + completed on the minted trace pass traceable, a foreign trace does not count" {
  [ -x "$BIN" ]
  run_step trace inputs.parent_trace=werk-parent-2
  [ "$status" -eq 0 ]
  resolve_args
  STEP_ENV=("STUB_SCOPE=")
  run_step scope "${RESOLVE[@]}"; [ "$status" -eq 0 ]
  grep -qx 'nothing=true' "$T/gh.out"
  STEP_ENV=()
  run_step complete steps.scope.outputs.nothing=true steps.prove.outcome=skipped steps.prove.outputs.issues=
  [ "$status" -eq 0 ]
  printf '{"event":"merge.landed","trace":"werk-parent-2"}\n' >> "$T/spine.log"
  STEP_ENV=(CHORUS_SPINE="$T/spine.log")
  run_step traceable steps.scope.outputs.seed= steps.scope.outputs.model=
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "2 event(s) on athena-"
  # a model leg that left no record: want 4, two events on the trace -> red
  run_step traceable steps.scope.outputs.seed= steps.scope.outputs.model=m.ttl
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "2 event(s) on trace athena-.* but 4 leg(s) ran"
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
  # visually how do i differentiate werk and athena rows". The served page's own script, run in node
  # against a card's events (#4336 — was a grep of the page source for identifiers).
  H="$ROOT/platform/api/public/borg/trace.html"
  printf '%s' '[{"ts":"2026-09-17T11:00:00Z","event":"athena.pipeline.started","role":"kade","trace_id":"athena-1-2"},{"ts":"2026-09-17T11:00:01Z","event":"merge.landed","role":"kade","trace_id":"werk-trace-aaa"},{"ts":"2026-09-17T11:00:02Z","event":"werk.started","role":"kade","trace_id":"athena-1-2"}]' > "$T/events.json"
  run node "$FIX/trace-page-run.js" "$H" "$T/events.json"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '^SUM .* 3 events · 2 traces (1 werk · 1 athena)'
  chips=$(echo "$output" | sed -n 's/^CHIPS //p')
  # one pill per row, oldest first: an athena-trace row is athena even with a werk event name
  [ "$(echo "$chips" | tr ',' '\n' | cut -d@ -f1 | paste -sd, -)" = "athena,werk,athena" ]
  athena_c=$(echo "$chips" | tr ',' '\n' | grep '^athena@' | cut -d@ -f2 | sort -u)
  werk_c=$(echo "$chips" | tr ',' '\n' | grep '^werk@' | cut -d@ -f2 | sort -u)
  [ "$(echo "$athena_c" | wc -l | tr -d ' ')" -eq 1 ]
  [ -n "$werk_c" ]
  [ "$athena_c" != "$werk_c" ]
  # NEGATIVE PROOF: an event outside the athena vocabulary on a non-athena trace is werk, never 'other'
  n=$(node -e "
    const src=require('fs').readFileSync('$H','utf8');
    const m=src.match(/function pipelineOf[\s\S]*?\n}/)[0];
    const pipelineOf=new Function(m+'; return pipelineOf;')();
    const r=[pipelineOf('merge.landed','1789595964146650000-65927'),pipelineOf('model.seed.posted','1789595964146650000-65927'),pipelineOf('werk.started','athena-1-2')];
    console.log(r.join(','))")
  [ "$n" = "werk,athena,athena" ]
}
