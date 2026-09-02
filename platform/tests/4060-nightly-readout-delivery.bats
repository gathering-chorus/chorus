#!/usr/bin/env bats
# @test-type: unit — signal:api is fixture-data (python http.server in a temp dir stands in for the route; no live :3340)
# 4060-nightly-readout-delivery.bats — "I don't even get a readout on the run."
#
# After a run finishes Jeff gets the readout without asking anyone. The wrapper
# does not compute it: it fetches the text form from chorus-api's readout route
# (one function feeds the nudge, the page and `--readout`) and nudges jeff.
#
# NEGATIVE PROOFS (#3734): when the route cannot answer, the nudge says so and
# carries NO numbers — the wrapper must never substitute a count of its own;
# and `--readout` exits 1 with nothing on stdout rather than inventing one.
#
# Hermetic (#3528): the "api" is python's http.server on an ephemeral port in
# a temp dir; delivery is a stub OPS_NUDGE that records its arguments.

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/nightly-suites.sh"
  [ -f "$SCRIPT" ] || skip "nightly-suites.sh not found"
  W="$BATS_TEST_TMPDIR"
  NUDGES="$W/nudges.txt"; : > "$NUDGES"
  cat > "$W/ops-nudge" <<STUB
#!/usr/bin/env bash
printf 'to=%s|from=%s|content=%s\n' "\$1" "\$3" "\$2" >> "$NUDGES"
STUB
  chmod +x "$W/ops-nudge"
  # the route's text form, at the path the wrapper requests (query string is
  # dropped by http.server's path translation, so the file is 'latest')
  mkdir -p "$W/www/api/chorus/nightly/runs"
  cat > "$W/www/api/chorus/nightly/runs/latest" <<'TXT'
nightly 2026-09-02 03:00:05 took 71 min: 320 suites, 5 red
red by owner: silas 4, kade 1
  silas  platform/api
  kade   tests-domain
since last run (2026-09-01 19:19:47): 1 new red, 3 fixed, 4 still red
http://localhost:3340/nightly?run=2026-09-02T03:00:05
TXT
  cp "$W/www/api/chorus/nightly/runs/latest" "$W/www/api/chorus/nightly/runs/2026-09-02T03:00:05"
  PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')
  # bats waits on any child holding fd 3 — close it, and exec so $SRV is python
  (cd "$W/www" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1 3>&-) &
  SRV=$!; disown "$SRV" 2>/dev/null || true
  for _ in $(seq 1 40); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done
  API="http://127.0.0.1:$PORT"
  # a port nothing listens on — the "api down" world
  DEAD="http://127.0.0.1:$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
}

teardown() { kill "$SRV" 2>/dev/null || true; }

deliver_with() {
  NIGHTLY_API="$1" OPS_NUDGE="$W/ops-nudge" bash -c '
    source "'"$SCRIPT"'" 2>/dev/null || true
    deliver_readout
  ' 2>"$W/stderr.txt"
}

@test "after a run, jeff receives the readout — the route's text, verbatim" {
  deliver_with "$API"
  [ "$(grep -c '^to=jeff|' "$NUDGES")" -eq 1 ] || { cat "$NUDGES"; return 1; }
  grep -q 'from=system' "$NUDGES"
  grep -q 'took 71 min: 320 suites, 5 red' "$NUDGES"
  grep -q 'kade   tests-domain' "$NUDGES"
  grep -q '1 new red, 3 fixed' "$NUDGES"
  grep -q 'nightly?run=2026-09-02T03:00:05' "$NUDGES"
}

@test "NEGATIVE PROOF: api down — jeff is told the readout is unavailable, with NO numbers" {
  deliver_with "$DEAD"
  [ "$(grep -c '^to=jeff|' "$NUDGES")" -eq 1 ] || { cat "$NUDGES"; return 1; }
  grep -q 'could not be built' "$NUDGES"
  # no suite count, no red count, no owner tally may appear
  ! grep -qE '[0-9]+ suites|[0-9]+ red|silas [0-9]' "$NUDGES"
  grep -q 'readout UNAVAILABLE' "$W/stderr.txt"
}

@test "NEGATIVE PROOF: a 404 from the route is unavailable too, not an empty readout" {
  rm "$W/www/api/chorus/nightly/runs/latest"
  deliver_with "$API"
  grep -q 'could not be built' "$NUDGES"
  ! grep -q 'suites' "$NUDGES"
}

@test "--readout prints the same text a role would be nudged with" {
  run env NIGHTLY_API="$API" bash "$SCRIPT" --readout
  [ "$status" -eq 0 ]
  [[ "$output" == *"took 71 min: 320 suites, 5 red"* ]]
  run env NIGHTLY_API="$API" bash "$SCRIPT" --readout 2026-09-02T03:00:05
  [ "$status" -eq 0 ]
  [[ "$output" == *"5 red"* ]]
}

@test "NEGATIVE PROOF: --readout with no api exits 1 and prints no numbers on stdout" {
  run env NIGHTLY_API="$DEAD" bash "$SCRIPT" --readout
  [ "$status" -eq 1 ]
  ! [[ "$output" == *"suites"* ]]
  [[ "$output" == *"no readout"* ]]
}

@test "delivery is wired into --run-all after the record is complete, gated like the team nudge" {
  # a renamed or dropped call must fail here, not silently stop reaching jeff
  awk '/^  --run-all\)/,/^    ;;/' "$SCRIPT" > "$W/runall.txt"
  grep -q 'notify_results "\$out"' "$W/runall.txt"
  grep -q 'NIGHTLY_NO_NUDGE:-0}" = "1" \] || deliver_readout' "$W/runall.txt"
  # order: emit_pipeline_run (the record) precedes deliver_readout (the delivery)
  [ "$(grep -n 'emit_pipeline_run' "$W/runall.txt" | head -1 | cut -d: -f1)" -lt \
    "$(grep -n 'deliver_readout' "$W/runall.txt" | head -1 | cut -d: -f1)" ]
}
