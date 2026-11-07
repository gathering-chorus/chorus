#!/usr/bin/env bats
# @test-type: unit — the borg-prove land rows' store-write plan against a stub door; no live service
# @domain: pipelines — scripts/land-4683-rows writes the rows pipelines/cicd-land.yaml is generated from (#4683)
# What Jeff sees: before any prod write, a dry run lists the 2 borg skills and
# the 3 land StepSkills, says CHANGE for today's land-4 (accept moves to 6),
# SAME for rows already right, and writes nothing; an unreadable door gives no plan.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4683-rows"
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
  export OWL_URL="http://127.0.0.1:$(cat "$D/port")" CHORUS_RUNNER="$BATS_TEST_TMPDIR/no-runner"
}

land4() {  # land 4 as the door serves it: fields in data, references in links
  printf '{"data":{"label":"%s","skillOrder":"4","withArguments":"%s"},"links":{"callsSkill":"chorus:skill-%s","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-kade"}}' "$1" "$2" "$3" > "$D/skills_stepskills_cicd-land-4.json"
}

@test "an empty store plans all 5 rows and writes nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 5 add, 0 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *'ADD    skills/skills/borg-record '*'"hasDomain": "cards"'* ]] || return 1
  [[ "$output" == *'ADD    skills/stepskills/cicd-land-6 '*'"callsSkill": "werk-accept"'* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "today's land-4 (accept) is a CHANGE to borg-verify-in-prod" {
  land4 "cicd land 4: werk-accept" "atomic=true" "werk-accept"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE skills/stepskills/cicd-land-4 "*"chorus:skill-werk-accept' -> 'borg-verify-in-prod'"* ]] || return 1
  [[ "$output" == *"4 add, 1 change; 0 same"* ]] || return 1
}

@test "negative proof: a land-4 already right reads SAME, and one that differs never does" {
  land4 "cicd land 4: borg-verify-in-prod" "continue_on=failure" "borg-verify-in-prod"
  run python3 "$PLAN"
  [[ "$output" == *"SAME   skills/stepskills/cicd-land-4"* ]] || return 1
  land4 "cicd land 4: borg-verify-in-prod" "" "borg-verify-in-prod"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE skills/stepskills/cicd-land-4"* ]] || return 1
  [[ "$output" != *"SAME   skills/stepskills/cicd-land-4"* ]] || return 1
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL="http://127.0.0.1:9" run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"plan: door writes"* ]] || return 1
}

@test "--apply with every write refused says REFUSED and exits 1, never applied" {
  run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED 5 of 5"* ]] || return 1
  [[ "$output" != *"applied"* ]] || return 1
}
