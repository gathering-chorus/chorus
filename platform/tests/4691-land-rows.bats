#!/usr/bin/env bats
# @test-type: unit — the borg-prove land rows' store-write plan against a stub door; no live service
# @domain: pipelines — scripts/land-4691-rows writes the rows pipelines/cicd-land.yaml is generated from (#4691)
# What Jeff sees: before any prod write, a dry run lists Borg Queen's 3 skills
# and the land StepSkills (ask, her step, check, accept), says CHANGE for today's
# land-5 (record leaves the land for her session), writes nothing, and refuses
# an --apply whose runner would write to another door than the plan read.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4691-rows"
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D"
  cat > "$BATS_TEST_TMPDIR/door.py" <<'PY'
import http.server, os, sys, time
D = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        p = os.path.join(D, self.path.split("?")[0].strip("/").replace("/", "_") + ".json")
        if not os.path.exists(p):
            self.send_response(404); self.end_headers(); self.wfile.write(b'{"data":{}}'); return
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
        self.wfile.write(open(p).read().encode())
    def do_POST(self):
        open(os.path.join(D, "writes"), "a").write(self.path + "\n")
        self.send_response(200); self.end_headers(); self.wfile.write(b"{}")
    do_PATCH = do_POST
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(D, "port"), "w").write(str(s.server_address[1]))
s.timeout, end = 1, time.time() + 20
while time.time() < end:
    s.handle_request()
PY
  python3 "$BATS_TEST_TMPDIR/door.py" "$D" >/dev/null 2>&1 3>&- &
  for _ in $(seq 1 50); do [ -s "$D/port" ] && break; perl -e 'select(undef,undef,undef,0.1)'; done
  export OWL_URL="http://127.0.0.1:$(cat "$D/port")" CHORUS_DOOR="http://localhost:$(cat "$D/port")" CHORUS_RUNNER="$BATS_TEST_TMPDIR/no-runner"
}

land5() {  # land 5 as the door serves it today: borg-record
  printf '{"data":{"label":"cicd land 5: borg-record","skillOrder":"5","withArguments":""},"links":{"callsSkill":"chorus:skill-borg-record","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-wren"}}' > "$D/skills_stepskills_cicd-land-5.json"
}

@test "an empty store plans all 8 rows and writes nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 8 add, 0 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *'ADD    skills/skills/borg-queen-proves '*'"executor": "human"'* ]] || return 1
  [[ "$output" == *'ADD    skills/stepskills/cicd-land-6 '*'"withArguments": "by=borg-queen choices=recorded|stop"'* ]] || return 1
  [[ "$output" == *'ADD    skills/stepskills/cicd-land-7 '*'"callsSkill": "borg-check"'* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "today's land-5 (record) is a CHANGE to borg-ask, and its owner is never in the change" {
  land5
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE skills/stepskills/cicd-land-5 "*"chorus:skill-borg-record' -> 'borg-ask'"* ]] || return 1
  ! printf '%s\n' "$output" | grep '^CHANGE skills/stepskills/cicd-land-5 ' | grep -q ownedBy || return 1
  [[ "$output" == *"7 add, 1 change; 0 same"* ]] || return 1
}

@test "negative proof: --apply refuses when the runner writes to another door than the plan read" {
  CHORUS_DOOR="http://127.0.0.1:3360" run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED: the plan reads"*"the runner writes to http://127.0.0.1:3360"* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL="http://127.0.0.1:9" CHORUS_DOOR="http://127.0.0.1:9" run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"plan: door writes"* ]] || return 1
}

@test "--apply with every write refused says REFUSED and exits 1, never applied" {
  run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED 8 of 8"* ]] || return 1
  [[ "$output" != *"applied"* ]] || return 1
}
