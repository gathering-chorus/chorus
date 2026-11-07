#!/usr/bin/env bats
# @test-type: unit — the domain flow's store-write plan against a stub door; no live service
# @domain: time — the platform domain this suite guards (#4885)
# What Jeff sees: before any prod write, a dry run lists the time row's change and the
# domain-flow pipeline's 16 rows, writes nothing, and an unreadable door gives no plan.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4885-rows"
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D"
  cat > "$BATS_TEST_TMPDIR/door.py" <<'PY'
import http.server, os, sys, time
D = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        p = os.path.join(D, self.path.split("?")[0].strip("/").replace("/", "_") + ".json")
        body = open(p).read() if os.path.exists(p) else '{"data": []}'
        self.send_response(200); self.end_headers(); self.wfile.write(body.encode())
    def do_POST(self):
        open(os.path.join(D, "writes"), "a").write(self.path + "\n"); self.send_response(200); self.end_headers()
    do_PATCH = do_POST
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(D, "port"), "w").write(str(s.server_address[1]))
s.timeout, end = 1, time.time() + 20
while time.time() < end:
    s.handle_request()
PY
  python3 "$BATS_TEST_TMPDIR/door.py" "$D" >/dev/null 2>&1 3>&- &
  for _ in $(seq 1 50); do [ -s "$D/port" ] && break; perl -e 'select(undef,undef,undef,0.1)'; done
  export OWL_URL="http://127.0.0.1:$(cat "$D/port")"
  # the live time row as it was before this land: scheduling only, no patterns
  echo '{"data":[{"name":"time","comment":"Scheduling, cron jobs, and time-based triggers for automated team work.","purpose":"Drive time-based work — scheduling, cron, and the triggers that run automated team tasks."}]}' > "$D/domains_domains.json"
}

@test "the dry run plans the time row's change and the pipeline's 16 rows, writing nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 16 add, 1 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *'CHANGE domains/domains/time'*'"pathPattern": ["wall-clock"'* ]] || return 1
  [[ "$output" == *"ADD    pipelines/domain-flow "* ]] || return 1
  [[ "$output" == *"ADD    stepskills/domain-flow-bar-1 "*'"withArguments": "by=APPROVER choices=agreed|rejected|stop"'* ]] || return 1
  [ ! -e "$D/writes" ] || return 1
}

@test "a time row that already carries this land's fields reads SAME" {
  python3 - "$PLAN" "$D/domains_domains.json" <<'PY'
import importlib.util, json, sys
spec = importlib.util.spec_from_loader("plan", loader=None); m = importlib.util.module_from_spec(spec); m.__dict__["__file__"] = sys.argv[1]
exec(open(sys.argv[1]).read().replace('if __name__ == "__main__":', 'if False:'), m.__dict__)
json.dump({"data": [m.TIME]}, open(sys.argv[2], "w"))
PY
  run python3 "$PLAN"
  [[ "$output" == *"SAME   domains/domains/time"* ]] || return 1
  [[ "$output" == *"16 add, 0 change; 1 same"* ]] || return 1
}

@test "negative proof: a time row missing one pattern is a CHANGE, never SAME" {
  echo '{"data":[{"name":"time","pathPattern":["wall-clock"]}]}' > "$D/domains_domains.json"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE domains/domains/time"* ]] || return 1
  [[ "$output" != *"SAME   domains/domains/time"* ]] || return 1
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL=http://127.0.0.1:9 run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"ADD"* ]] || return 1
}
