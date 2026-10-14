#!/usr/bin/env bats
# @test-type: unit
# @domain: spine — the product domain this suite guards (#4334)
# #4195 — an athena run is one trace that is only athena. The trace reader page,
# executed in node against a fixture event list, labels and counts werk and athena rows
# apart. (#4515: the land-job step proofs went with the retired act workflow athena.yml;
# the v2 athena-land's trace is the dagu run, covered by 4494-athena-land.bats.)

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
FIX="$ROOT/platform/tests/fixtures/4336"

setup() {
  T="$(mktemp -d "$BATS_TEST_TMPDIR/t.XXXXXX")"
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
