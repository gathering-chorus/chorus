#!/usr/bin/env bats
# @test-type: unit
# @domain: tests
# @card: #4707
# @owner: silas
# #4707 reopen — platform/scripts/werk-land-retry retries a stopped land on the
# go Jeff already gave. On 10-23 #4715's land stopped at borg-ask while the
# roles door restarted, and the hook (rightly) refused every raw retry. What
# Jeff sees: a land that stopped after his go is finished without a second go,
# and a land nobody gave a go for (the 06:03 #4704 curl) is never retried.
# Fixtures are dagu run files in a scratch data dir; dagu itself is a fake that
# records what it was asked to run.

setup() {
  ROOT="$BATS_TEST_DIRNAME/../.."
  VERB="$ROOT/platform/scripts/werk-land-retry"
  export WLR_DATA="$BATS_TEST_TMPDIR/data" WLR_LOG_DIR="$BATS_TEST_TMPDIR/logs"
  fake_dagu
}

# #4771 — the retry is a POST to dagu's server; a fake server records it.
fake_dagu() {  # fake_dagu [code]
  # answers one POST on 127.0.0.1 (port to $BATS_TEST_TMPDIR/port), records
  # "POST <path> <body>" to dagu-called, answers <code>; exits after one
  # request or 30s, so no test ever needs to stop it
  python3 - "$BATS_TEST_TMPDIR" "${1:-200}" 3>&- <<'PY' &
import http.server, os, socketserver, sys, time

d, code = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 200


class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0)).decode()
        with open(f"{d}/dagu-called", "w") as f:
            f.write(f"POST {self.path} {body}\n")
        self.send_response(code)
        self.end_headers()
        self.wfile.write(b'{"dagRunId":"ok"}' if code == 200 else b'{"message":"refused by fake"}')

    def log_message(self, *a):
        pass


srv = socketserver.TCPServer(("127.0.0.1", 0), H)
srv.timeout = 0.5
with open(f"{d}/port.tmp", "w") as f:
    f.write(str(srv.server_address[1]))
os.rename(f"{d}/port.tmp", f"{d}/port")
end = time.time() + 30
while time.time() < end and not os.path.exists(f"{d}/dagu-called"):
    srv.handle_request()
PY
  for _ in $(seq 1 50); do [ -f "$BATS_TEST_TMPDIR/port" ] && break; sleep 0.1; done
  export WLR_DAGU_URL="http://127.0.0.1:$(cat "$BATS_TEST_TMPDIR/port")"
}

# run <dag> <id> <params> <status> <nodes: name=status,...> [go inputs json] [enqueued land id]
run_file() {
  python3 - "$WLR_DATA" "$BATS_TEST_TMPDIR" "$@" <<'PY'
import json, os, sys
data, tmp, dag, rid, params, status, nodes = sys.argv[1:8]
go = json.loads(sys.argv[8]) if len(sys.argv) > 8 and sys.argv[8] else None
enq = sys.argv[9] if len(sys.argv) > 9 else None
d = f"{data}/dag-runs/{dag}/dag-runs/2026/10/23/dag-run_20261023_221141Z_{rid}/a_20261023_221144Z_a1"
os.makedirs(d, exist_ok=True)
ns = []
for pair in nodes.split(","):
    name, st = pair.split("=")
    n = {"step": {"name": name}, "status": int(st)}
    if name == "skill-werk-go" and go is not None:
        n["humanTaskInput"] = go
    if name == "cicd-land" and enq:
        out = f"{tmp}/{rid}-cicd-land.out"
        line = json.dumps({"msg": "Enqueued dag-run", "dag": "cicd-land", "run-id": enq}).replace(" ", "") + "\n"
        if os.environ.get("ENQ_ON") == "stderr":
            # #4760 — dagu logs the enqueue on stderr; stdout stays empty
            open(out, "w").write("")
            open(out + ".err", "w").write(line)
            n["stderr"] = out + ".err"
        else:
            open(out, "w").write(line)
        n["stdout"] = out
    ns.append(n)
open(f"{d}/status.jsonl", "w").write(json.dumps({"name": dag, "dagRunId": rid, "params": params,
    "status": int(status), "queuedAt": "2026-10-23T22:11:41Z", "nodes": ns}) + "\n")
PY
}

LAND_STOPPED="skill-werk-merge=4,skill-werk-sync=4,skill-werk-deploy-prod=4,skill-borg-ask=2,skill-werk-accept=3"
CICD_DONE="skill-werk-demo=4,skill-werk-go=4,cicd-land=4"

go_given() {  # the 4715 shape: Jeff's go enqueued land L1, which stopped at borg-ask
  local said='{"accepter":"jeff","decision":"go"}'
  [ $# -gt 0 ] && said="$1"
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" "$said" "${2:-L1}"
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 2 "$LAND_STOPPED"
}

@test "a land that stopped after Jeff's go is retried from its stopped step, on that go" {
  go_given
  run "$VERB" 4715
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"the go is cicd run C1 (accepter=jeff, decision=go)"* ]] || { echo "$output"; return 1; }
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -f "$BATS_TEST_TMPDIR/dagu-called" ] && break; sleep 0.2; done
  called=$(cat "$BATS_TEST_TMPDIR/dagu-called")
  [[ "$called" == 'POST /api/v1/dag-runs/cicd-land/L1/retry {"dagRunId": "L1", "stepName": "skill-borg-ask", "includeDownstream": true}' ]] || { echo "$called"; return 1; }
}

# #4758 — scripts/werk-land-retry: the go now waits in the go half, cicd-go
# (run go-<cicd run>); the land it enqueued is retried on that go.
@test "a land enqueued by a cicd-go go is retried on that go" {
  run_file cicd-go go-C2 "CARD=4758 ROLE=silas CICD_RUN=C2" 4 "skill-werk-go=4,cicd-land=4" '{"accepter":"jeff","decision":"go"}' L2
  run_file cicd-land L2 "CARD=4758 ROLE=silas ACCEPTER=jeff" 2 "$LAND_STOPPED"
  run "$VERB" 4758 --dry-run
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"the go is cicd-go run go-C2 (accepter=jeff, decision=go)"* ]] || { echo "$output"; return 1; }
}

# #4760 — scripts/werk-land-retry: dagu writes the enqueue line to stderr (10-27,
# #4760's land); the go is still found there
@test "a land whose go logged its enqueue on stderr is retried on that go" {
  ENQ_ON=stderr run_file cicd-go go-C5 "CARD=4760 ROLE=silas CICD_RUN=C5" 4 "skill-werk-go=4,cicd-land=4" '{"accepter":"jeff","decision":"go"}' L5
  run_file cicd-land L5 "CARD=4760 ROLE=silas ACCEPTER=jeff" 2 "$LAND_STOPPED"
  run "$VERB" 4760 --dry-run
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"the go is cicd-go run go-C5"* ]] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: a cicd-go run whose go was not Jeff's enqueues nothing retryable
@test "negative proof: a cicd-go go answered by a role is refused" {
  run_file cicd-go go-C3 "CARD=4758 ROLE=silas CICD_RUN=C3" 4 "skill-werk-go=4,cicd-land=4" '{"accepter":"silas","decision":"go"}' L3
  run_file cicd-land L3 "CARD=4758 ROLE=silas ACCEPTER=jeff" 2 "$LAND_STOPPED"
  run "$VERB" 4758 --dry-run
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"no recorded go"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a land enqueued by hand (no go step names it) is refused, dagu untouched" {
  go_given '{"accepter":"jeff","decision":"go"}' L0   # the go enqueued a different run
  run "$VERB" 4715
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"no recorded go enqueued #4715's land L1"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "negative proof: a go step answered no, or by anyone but jeff, is not a go" {
  for said in '{"accepter":"jeff","decision":"no"}' '{"accepter":"kade","decision":"go"}' '{}'; do
    rm -rf "$WLR_DATA"
    go_given "$said"
    run "$VERB" 4715
    [ "$status" -eq 1 ] || { echo "$said: $output"; return 1; }
    [[ "$output" == *"no recorded go"* ]] || { echo "$said: $output"; return 1; }
  done
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "negative proof: a land with no ACCEPTER=jeff is refused" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=flow" 2 "$LAND_STOPPED"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"carries no ACCEPTER=jeff"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a land still running, or another land in flight, is not retried" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 1 "skill-werk-merge=4,skill-borg-ask=1"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"is running, not stopped"* ]] || { echo "$output"; return 1; }
  go_given
  run_file cicd-land L9 "CARD=4719 ROLE=abby-normal ACCEPTER=jeff" 1 "skill-werk-merge=1"
  run "$VERB" 4715
  [ "$status" -eq 1 ] && [[ "$output" == *"lands go one at a time"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/dagu-called" ]
}

@test "a land that reads succeeded with steps never run is stopped, not done (#4540)" {
  run_file cicd C1 "CARD=4715 ROLE=kade" 4 "$CICD_DONE" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4715 ROLE=kade ACCEPTER=jeff" 4 "skill-werk-merge=4,skill-borg-ask=4,skill-werk-accept=0"
  run "$VERB" 4715 --dry-run
  [ "$status" -eq 0 ] && [[ "$output" == *"/api/v1/dag-runs/cicd-land/L1/retry from skill-werk-accept, downstream"* ]] || { echo "$output"; return 1; }
}
