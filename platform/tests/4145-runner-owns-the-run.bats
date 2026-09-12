#!/usr/bin/env bats
# @test-type: integration:api — drives the built werk-test binary against a stub runner, stub registry and stub nudge on loopback; no live service, no live log
#
# #4145 — Jeff, 2026-09-11: "delete the nightly script, change the launchd".
# `werk-test --nightly --run-all` is now the whole 03:00 run. These proofs
# drive the binary the way launchd will, with every outside thing stubbed:
# the runner child (a script printing runner lines), the registry (a loopback
# HTTP server counting its reads), the nudge primitive (a script appending to
# a file), the log (a temp file). What the page reads must come out identical
# in shape to what the wrapper wrote for 5 months.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  BIN="${WERK_TEST_BIN:-$ROOT/platform/services/werk-test/target/release/werk-test}"
  [ -x "$BIN" ] || skip "werk-test not built at $BIN"
  T="$BATS_TEST_TMPDIR"
  # #3528 — bring your own process table: a real nightly on the box must not refuse these runs
  printf '#!/bin/bash\necho "  PID  PPID ELAPSED COMMAND"\n' > "$BATS_TEST_TMPDIR/ps-none"; chmod +x "$BATS_TEST_TMPDIR/ps-none"
  export NIGHTLY_PS="$BATS_TEST_TMPDIR/ps-none"
  mkdir -p "$T/root/platform/scripts" "$T/root/platform/tests" "$T/fail"
  touch "$T/root/platform/tests/a.bats"
  cat > "$T/runner.sh" <<'EOS'
#!/bin/bash
echo "nightly-plan|bats|platform/tests/a.bats"
echo "nightly-case|platform/tests/a.bats|one"
echo "nightly-case|platform/tests/a.bats|two"
echo "nightly-unit|bats|platform/tests/a.bats|pass|2 pass, 0 fail"
echo "nightly-unit|cargo|werk-x|pass|5 pass, 0 fail"
echo "nightly-unit|perf|platform/tests/p.sh|fail|0 pass, 1 fail"
echo "nightly-unit|shell|platform/scripts/z.sh|fail|0 pass, 1 fail"
echo "werk-test: fail (exit 1)"
EOS
  chmod +x "$T/runner.sh"
  printf '#!/bin/bash\necho "NUDGE to=$1 msg=$2" >> "%s/nudges.txt"\n' "$T" > "$T/nudge.sh"
  chmod +x "$T/nudge.sh"
  python3 - "$T" <<'EOS' &
import sys, json, http.server, socketserver
T = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(s):
        open(T + "/hits.txt", "a").write(s.path + "\n")
        if s.path.startswith("/tests"):
            body = {"data": [
                {"filePath": "platform/tests/a.bats", "testName": "one", "covers": "logs"},
                {"filePath": "platform/tests/a.bats", "testName": "two", "covers": "logs"},
                {"filePath": "platform/tests/a.bats", "testName": "three", "covers": "logs"}]}
        elif s.path.startswith("/domains"):
            body = {"data": [{"name": "logs", "ownedBy": "role-silas"}]}
        else:
            body = {"data": []}
        b = json.dumps(body).encode()
        s.send_response(200); s.send_header("Content-Length", str(len(b))); s.end_headers(); s.wfile.write(b)
    def do_POST(s):
        s.send_response(201); s.end_headers()
    def log_message(s, *a): pass
srv = socketserver.TCPServer(("127.0.0.1", 0), H)
open(T + "/port", "w").write(str(srv.server_address[1]))
srv.serve_forever()
EOS
  STUB_PID=$!
  for _ in $(seq 1 50); do [ -s "$T/port" ] && break; sleep 0.1; done
  PORT=$(cat "$T/port")
}

teardown() {
  [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null || true
}

run_all() {
  CHORUS_ROOT="$T/root" CHORUS_HOME="$T/root" NIGHTLY_LOG_PATH="$T/nightly.log" NIGHTLY_FAIL_DIR="$T/fail" \
  NIGHTLY_LOCKDIR="$T/lock" OWLAPI="http://127.0.0.1:$PORT" NIGHTLY_API="http://127.0.0.1:$PORT" OPS_NUDGE="$T/nudge.sh" \
  NIGHTLY_RUNNER_CMD="${RUNNER:-$T/runner.sh}" NIGHTLY_LEGS_NOOP=1 NIGHTLY_LOAD_MAX_PER_CORE=99 CHORUS_LOG_BIN=/nonexistent \
  "$BIN" --nightly --run-all "$@"
}

@test "the log gets RUN|start, one SUITE row per unit in the page's shape, and RUN|complete with the count" {
  run run_all
  [ "$status" -eq 0 ]
  grep -qE '^RUN\|start\|[0-9T:-]+\|pid=[0-9]+$' "$T/nightly.log"
  grep -q '^SUITE|bats|platform/tests/a.bats|silas|pass|2 pass, 0 fail$' "$T/nightly.log"
  grep -q '^SUITE|cargo|platform/services/werk-x|silas|pass|5 pass, 0 fail$' "$T/nightly.log"
  grep -q '^SUITE|perf|platform/tests/p.sh|unowned|slow|' "$T/nightly.log"
  grep -q '^SUITE|shell|platform/scripts/z.sh|silas|fail|0 pass, 1 fail$' "$T/nightly.log"
  grep -qE '^RUN\|complete\|[0-9T:-]+\|suites=5$' "$T/nightly.log"
}

@test "the registry is read ONCE per run (the wrapper read it once per row: 385 times on 2026-09-11)" {
  run run_all
  [ "$(grep -c '^/tests' "$T/hits.txt")" -eq 1 ]
  [ "$(grep -c '^/domains' "$T/hits.txt")" -eq 1 ]
}

@test "NEGATIVE PROOF: a registered case the run never posted is never-ran, with its name kept in the log" {
  run run_all
  grep -q '^SUITE|reconcile|tests-domain|kade|fail|0 pass, 1 fail (1 registered test(s) never ran of 3' "$T/nightly.log"
  grep -q '^reconcile-detail|.*platform/tests/a.bats :: three' "$T/nightly.log"
}

@test "control: when every registered case was posted the census cross-foots and writes no detail" {
  cat > "$T/runner2.sh" <<'EOS'
#!/bin/bash
echo "nightly-case|platform/tests/a.bats|one"
echo "nightly-case|platform/tests/a.bats|two"
echo "nightly-case|platform/tests/a.bats|three"
echo "nightly-unit|bats|platform/tests/a.bats|pass|3 pass, 0 fail"
EOS
  chmod +x "$T/runner2.sh"
  RUNNER="$T/runner2.sh" run run_all
  grep -q '^SUITE|reconcile|tests-domain|kade|pass|1 pass, 0 fail (3 registered, every one executed' "$T/nightly.log"
  ! grep -q '^reconcile-detail|' "$T/nightly.log"
}

@test "reds nudge their owner as they land, then one grouped line per owner and one TOTAL; slow is named as speed" {
  run run_all
  grep -q 'NUDGE to=silas msg=nightly RED now: platform/scripts/z.sh' "$T/nudges.txt"
  grep -q 'NUDGE to=silas msg=nightly: 1 suite(s) red — z.sh' "$T/nudges.txt"
  grep -q 'NUDGE to=kade msg=nightly: 1 suite(s) red — tests-domain' "$T/nudges.txt"
  grep -qE 'NUDGE to=kade msg=nightly TOTAL: 2 red across the board \((kade 1, silas 1|silas 1, kade 1)\) — bar is zero — 1 slow \(speed, not breakage: p.sh\)' "$T/nudges.txt"
  grep -q 'NUDGE to=jeff msg=' "$T/nudges.txt"
}

@test "NEGATIVE PROOF: a runner that cannot start is one fail row naming it, never a green run" {
  RUNNER="$T/does-not-exist" run run_all
  grep -q '^SUITE|runner|werk-test-nightly|silas|fail|0 pass, 1 fail (runner could not start' "$T/nightly.log"
  grep -qE '^RUN\|complete\|' "$T/nightly.log"
}

@test "a failing unit gets its own failure file with the lane's lines about it" {
  run run_all
  [ -s "$T/fail/shell-platform_scripts_z_sh.log" ]
  grep -q 'z.sh' "$T/fail/shell-platform_scripts_z_sh.log"
  [ -s "$T/fail/_lane-output.log" ]
}

@test "a werk-rooted run writes its own log and nudges nobody (#3722)" {
  mkdir -p "$T/chorus-werk/kade-9/platform/tests"
  CHORUS_ROOT="$T/chorus-werk/kade-9" CHORUS_HOME="$T/chorus-werk/kade-9" NIGHTLY_LOG_PATH="" NIGHTLY_FAIL_DIR="$T/fail" \
  NIGHTLY_LOCKDIR="$T/lock2" OWLAPI="http://127.0.0.1:$PORT" NIGHTLY_API="http://127.0.0.1:$PORT" OPS_NUDGE="$T/nudge.sh" \
  NIGHTLY_RUNNER_CMD="$T/runner.sh" NIGHTLY_LEGS_NOOP=1 NIGHTLY_LOAD_MAX_PER_CORE=99 CHORUS_LOG_BIN=/nonexistent \
  run "$BIN" --nightly --run-all
  [ "$status" -eq 0 ]
  [ -s /tmp/nightly-kade-9.log ]
  [ ! -f "$T/nudges.txt" ]
  rm -f /tmp/nightly-kade-9.log
}

@test "single flight: a second run while the first holds the lock is refused and says why" {
  mkdir -p "$T/lock"; echo $$ > "$T/lock/pid"
  run run_all
  [ "$status" -eq 0 ]
  [[ "$output$stderr" == *"REFUSED"* ]] || grep -q "REFUSED" <<< "$output"
  [ ! -f "$T/nightly.log" ]
}
