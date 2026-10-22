#!/usr/bin/env bats
# @test-type: unit — the clearing-evaluate land rows' store-write plan against a stub door; no live service
# @domain: pipelines — scripts/land-4682-rows writes the rows pipelines/cicd-land.yaml is generated from (#4682)
# What Jeff sees: before any prod write, a dry run lists the clearing-evaluate skill and
# the cicd-land-7 StepSkill, says CHANGE or ADD, SAME for rows already right, and writes nothing; an unreadable door gives no plan.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4682-rows"
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

@test "an empty store plans clearing-evaluate skill and stepskill and writes nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 2 add, 0 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *'ADD    skills/skills/clearing-evaluate '*'"hasDomain": "cards"'* ]] || return 1
  [[ "$output" == *'ADD    skills/stepskills/cicd-land-7 '*'"callsSkill": "clearing-evaluate"'* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "negative proof: a row already right reads SAME, and one that differs never does" {
  printf '{"data":{"label":"cicd land 7: clearing-evaluate","skillOrder":"7","withArguments":"continue_on=failure enqueue=true"},"links":{"callsSkill":"chorus:skill-clearing-evaluate","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-abby-normal"}}' > "$D/skills_stepskills_cicd-land-7.json"
  run python3 "$PLAN"
  [[ "$output" == *"SAME   skills/stepskills/cicd-land-7"* ]] || return 1
  printf '{"data":{"label":"cicd land 7: clearing-evaluate","skillOrder":"7","withArguments":""},"links":{"callsSkill":"chorus:skill-clearing-evaluate","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-abby-normal"}}' > "$D/skills_stepskills_cicd-land-7.json"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE skills/stepskills/cicd-land-7"* ]] || return 1
  [[ "$output" != *"SAME   skills/stepskills/cicd-land-7"* ]] || return 1
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL="http://127.0.0.1:9" run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"plan: door writes"* ]] || return 1
}

@test "--apply with write refused says REFUSED and exits 1, never applied" {
  run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED 2 of 2"* ]] || return 1
  [[ "$output" != *"applied"* ]] || return 1
}
