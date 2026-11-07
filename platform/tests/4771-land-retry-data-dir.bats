#!/usr/bin/env bats
# @test-type: unit
# @domain: tests
# @card: #4771
# @owner: kade
# #4771 AC3 (was #4864) — werk-land-retry finds dagu's runs from any account.
# On 11-01 a role ran it after Jeff's go and got "no land run to retry": it
# looked under the caller's HOME (/Users/chorus-<role>/.chorus/dagu/data), while
# dagu's data_dir lives where dagu.yaml says. What Jeff sees: a role retries a
# stopped land without borrowing his account.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  VERB="$ROOT/platform/scripts/werk-land-retry"
  unset WLR_DATA WLR_LOG_DIR WLR_DAGU_CONFIG CHORUS_HOME
  export HOME="$BATS_TEST_TMPDIR/role-home"   # a role account with no dagu data
  mkdir -p "$HOME"
  DATA="$BATS_TEST_TMPDIR/shared/data" LOGS="$BATS_TEST_TMPDIR/shared/logs"
  CONF="$BATS_TEST_TMPDIR/dagu.yaml"
  printf 'data_dir: %s\nlog_dir: %s\n' "$DATA" "$LOGS" > "$CONF"
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

# run_file <dag> <id> <params> <status> <nodes> [go json] [enqueued land id]
run_file() {
  python3 - "$DATA" "$BATS_TEST_TMPDIR" "$@" <<'PY'
import json, os, sys
data, tmp, dag, rid, params, status, nodes = sys.argv[1:8]
go = json.loads(sys.argv[8]) if len(sys.argv) > 8 and sys.argv[8] else None
enq = sys.argv[9] if len(sys.argv) > 9 else None
d = f"{data}/dag-runs/{dag}/dag-runs/2026/11/01/dag-run_20261102_015323Z_{rid}/a_20261102_015323Z_a1"
os.makedirs(d, exist_ok=True)
ns = []
for pair in nodes.split(","):
    name, st = pair.split("=")
    n = {"step": {"name": name}, "status": int(st)}
    if name == "skill-werk-go" and go is not None:
        n["humanTaskInput"] = go
    if name == "cicd-land" and enq:
        out = f"{tmp}/{rid}-cicd-land.out"
        open(out, "w").write(json.dumps({"msg": "Enqueued dag-run", "dag": "cicd-land", "run-id": enq}).replace(" ", "") + "\n")
        n["stdout"] = out
    ns.append(n)
open(f"{d}/status.jsonl", "w").write(json.dumps({"name": dag, "dagRunId": rid, "params": params,
    "status": int(status), "queuedAt": "2026-11-02T01:53:23Z", "nodes": ns}) + "\n")
PY
}

land_stopped_after_go() {  # the #4642 shape: Jeff's go enqueued L1, which stopped at werk-merge
  run_file cicd-go go-C1 "CARD=4642 ROLE=wren CICD_RUN=C1" 4 "skill-werk-go=4,cicd-land=4" '{"accepter":"jeff","decision":"go"}' L1
  run_file cicd-land L1 "CARD=4642 ROLE=wren ACCEPTER=jeff" 2 "skill-werk-merge=2,skill-werk-sync=3,skill-werk-accept=3"
}

@test "a role account with no dagu data finds the land through dagu.yaml's data_dir" {
  land_stopped_after_go
  WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642 --dry-run
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" != *"no land run"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"go-C1"* ]] || { echo "$output"; return 1; }
}

@test "the retry log goes to dagu.yaml's log_dir, not under the caller's HOME" {
  land_stopped_after_go
  WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -f "$LOGS/werk-land-retry-4642.log" ] || { ls -R "$BATS_TEST_TMPDIR"; return 1; }
  [ -z "$(find "$HOME" -type f)" ] || { find "$HOME" -type f; return 1; }
}

@test "with no CHORUS_HOME, the dagu.yaml is the repo's own, never one under HOME" {
  run "$VERB" 999999 --dry-run
  [[ "$output" == *"/.chorus/dagu/data"* || "$output" == *"data_dir"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"$HOME"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a dagu.yaml whose data_dir does not exist fails loud, naming the path" {
  printf 'data_dir: %s/nowhere\nlog_dir: %s\n' "$BATS_TEST_TMPDIR" "$LOGS" > "$CONF"
  WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642 --dry-run
  [ "$status" -ne 0 ]
  [[ "$output" == *"$BATS_TEST_TMPDIR/nowhere"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"no land run"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a dagu.yaml with no data_dir key fails loud, naming the config" {
  printf 'log_dir: %s\n' "$LOGS" > "$CONF"
  WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642 --dry-run
  [ "$status" -ne 0 ]
  [[ "$output" == *"$CONF"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"no land run"* ]] || { echo "$output"; return 1; }
}

# #4771 — the land runs as dagu's account: the retry is asked of dagu's server,
# never run by a `dagu` in the caller's process (11-01: wren's retry ran under
# /Users/chorus-wren and found no chorus-runner).
@test "the retry is a POST to dagu's server, and no dagu runs in the caller's process" {
  land_stopped_after_go
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  printf '#!/bin/sh\necho "$@" > %s/local-dagu\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/bin/dagu"
  chmod +x "$BATS_TEST_TMPDIR/bin/dagu"
  PATH="$BATS_TEST_TMPDIR/bin:$PATH" WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  called=$(cat "$BATS_TEST_TMPDIR/dagu-called")
  [[ "$called" == 'POST /api/v1/dag-runs/cicd-land/L1/retry {"dagRunId": "L1", "stepName": "skill-werk-merge", "includeDownstream": true}' ]] || { echo "$called"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/local-dagu" ] || { cat "$BATS_TEST_TMPDIR/local-dagu"; return 1; }
}

@test "negative proof: dagu's server refusing the retry fails loud, never reports retrying" {
  land_stopped_after_go
  rm -f "$BATS_TEST_TMPDIR/port"
  fake_dagu 500
  WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"refused the retry"*"500"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"retrying from"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: dagu's server unreachable fails loud, naming the URL" {
  land_stopped_after_go
  WLR_DAGU_URL="http://127.0.0.1:9" WLR_DAGU_CONFIG="$CONF" run "$VERB" 4642
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"http://127.0.0.1:9/api/v1/dag-runs/cicd-land/L1/retry"*"unreachable"* ]] || { echo "$output"; return 1; }
}
