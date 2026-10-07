#!/usr/bin/env bats
# @test-type: integration — a real daemon under a real launchd job of its own
# @domain: services — the product domain this suite guards (#4334)
# #4446 — every chorus service logs its own start, stop and failure.
#
# Jeff, 2026-10-06: "all services must be rigorous about structured logging of
# starts stops and failures". This suite runs THIS tree's chorus-hooks under a
# disposable LaunchAgent (com.chorus.fixture-4446-<n>), never a prod label, with
# its own HOME and its own spine. launchd restarts it the way it restarts prod,
# so "restart → start event" is measured, not assumed.
#
# The guard (AC3) is `restarted_with_start`: after a restart the spine must hold
# a service.started for the new pid. Its negative proof runs the same guard
# against a job that never logs (/bin/sleep) and requires it to go red.

TREE="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
BIN="$TREE/platform/services/chorus-hooks/target/release/chorus-hooks"

setup() {
  [[ -x "$BIN" ]] || skip "release binary not built: $BIN"
  UIDN="$(id -u)"
  LABEL="com.chorus.fixture-4446-$$-$BATS_TEST_NUMBER"
  FIXHOME="$(mktemp -d /tmp/h4446.XXXXXX)"
  SPINE="$FIXHOME/spine.log"
  : > "$SPINE"
  PLIST="$FIXHOME/$LABEL.plist"
}

teardown() {
  local pid; pid="$(job_pid)"
  launchctl bootout "gui/$UIDN/$LABEL" 2>/dev/null || true
  # bootout returns before the daemon finishes its shutdown writes
  for _ in $(seq 1 50); do [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  if [[ "$FIXHOME" == /tmp/h4446.* ]]; then rm -rf "$FIXHOME"; fi
}

# write_job <program> — a KeepAlive job in its own world, loaded
write_job() {
  write_plist "$1"
  launchctl bootstrap "gui/$UIDN" "$PLIST"
}

# write_plist <program> — the same job's plist, not loaded
write_plist() {
  cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key><array>$1</array>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>1</integer>
  <key>EnvironmentVariables</key><dict>
    <key>HOME</key><string>$FIXHOME</string>
    <key>CHORUS_LOG_FILE</key><string>$SPINE</string>
    <key>CHORUS_CONTEXT</key><string>test</string>
    <key>CHORUS_ROOT</key><string>$TREE</string>
    <key>CHORUS_HOME</key><string>$TREE</string>
  </dict>
  <key>StandardErrorPath</key><string>$FIXHOME/err.log</string>
</dict></plist>
PLIST
}

job_pid() { launchctl print "gui/$UIDN/$LABEL" 2>/dev/null | awk -F' = ' '/^\tpid = /{print $2}'; }

# wait_pid_change <old> — echo the new pid once launchd has restarted the job
wait_pid_change() {
  local p
  for _ in $(seq 1 100); do
    p="$(job_pid)"
    [[ -n "$p" && "$p" != "$1" ]] && { echo "$p"; return 0; }
    sleep 0.1
  done
  return 1
}

# the guard: a service.started for this label and this pid reached the spine
restarted_with_start() {
  local pid="$1"
  for _ in $(seq 1 50); do
    grep '"event":"service.started"' "$SPINE" | grep "\"service\":\"$LABEL\"" | grep -q "\"pid\":\"$pid\"" && return 0
    sleep 0.1
  done
  echo "no service.started for $LABEL pid $pid in:"; cat "$SPINE"
  return 1
}

@test "AC1: a daemon started by launchd logs service.started with its label, pid and version" {
  write_job "<string>$BIN</string>"
  pid="$(wait_pid_change "")"
  restarted_with_start "$pid"
  line="$(grep '"event":"service.started"' "$SPINE" | tail -1)"
  [[ "$line" =~ \"version\":\"[0-9a-f]{12}\" ]] || return 1
}

@test "AC3: a restart produces a start event for the new pid" {
  write_job "<string>$BIN</string>"
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  # the old pid said it stopped, and why
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"'
}

@test "AC1: a kill the daemon could not log is reported by its next start" {
  write_job "<string>$BIN</string>"
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kill -KILL "$first"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  line="$(grep '"event":"service.failed"' "$SPINE" | tail -1)"
  echo "line=$line"
  [[ "$line" == *'"signal":"Killed: 9"'* ]] || return 1
  [[ "$line" == *"\"service\":\"$LABEL\""* ]] || return 1
  [[ "$line" == *'"level":"error"'* ]] || return 1
}

@test "NEGATIVE PROOF: the guard goes red for a service that restarts without logging" {
  write_job "<string>/bin/sleep</string><string>600</string>"
  first="$(wait_pid_change "")"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  run restarted_with_start "$second"
  [[ "$status" -ne 0 ]] || return 1
}

# --- athena-make serve: a std daemon, no async runtime (shared/service_stop.rs) ---
AM="$TREE/platform/services/athena-make/target/release/athena-make"
am_job() {
  [[ -x "$AM" ]] || skip "release binary not built: $AM"
  # a high port of its own; it reads the vocabulary from the live store (read only)
  write_job "<string>$AM</string><string>serve</string><string>--port</string><string>$((43000 + $$ % 1000))</string>"
}

@test "athena-make: start, then a kickstart logs service.stopped SIGTERM and a new start" {
  am_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"'
  # the clean stop exits 0, so the new start reports no failure
  ! grep '"event":"service.failed"' "$SPINE" || return 1
}

@test "athena-make: a kill -9 is reported as service.failed by the next start" {
  am_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kill -KILL "$first"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.failed"' "$SPINE" | grep -q '"signal":"Killed: 9"'
}

# --- node services: the same helper chorus-api calls (chorus-sdk/lifecycle) ---
# A ten-line daemon wired exactly as server.ts wires it: started() once up,
# stopped() in its SIGTERM handler. chorus-api itself is proven on the card's
# demo env, which runs it under its own launchd label.
node_job() {
  NODE="$(command -v node)"
  [[ -x "$NODE" ]] || skip "node not on PATH"
  cat > "$FIXHOME/daemon.js" <<JS
const { serviceLifecycle } = require('$TREE/platform/chorus-sdk/lifecycle/service-lifecycle');
const lifecycle = serviceLifecycle('com.chorus.fixture-node');
process.on('SIGTERM', () => { lifecycle.stopped('SIGTERM'); process.exit(0); });
lifecycle.started();
setInterval(() => {}, 1000);
JS
  write_job "<string>$NODE</string><string>$FIXHOME/daemon.js</string>"
}

@test "node: start, kickstart → service.stopped SIGTERM, then a new start" {
  node_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"'
}

@test "node: a kill -9 is reported as service.failed by the next start" {
  node_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kill -KILL "$first"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.failed"' "$SPINE" | grep '"level":"error"' | grep -q '"signal":"Killed: 9"'
}

# --- bash services: platform/scripts/lib/service-lifecycle.sh ---
LIB="$TREE/platform/scripts/lib/service-lifecycle.sh"

bash_daemon_job() {
  cat > "$FIXHOME/daemon.sh" <<SH
#!/bin/bash
. "$LIB"
service_lifecycle_daemon com.chorus.fixture-bash
# 'sleep & wait' so SIGTERM runs the trap at once, not after the sleep
while :; do sleep 1 & wait \$!; done
SH
  write_job "<string>/bin/bash</string><string>$FIXHOME/daemon.sh</string>"
}

@test "bash daemon: start, kickstart → service.stopped SIGTERM, then a new start" {
  bash_daemon_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"'
  ! grep '"event":"service.failed"' "$SPINE" || return 1
}

@test "bash daemon: a kill -9 is reported as service.failed by the next start" {
  bash_daemon_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kill -KILL "$first"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.failed"' "$SPINE" | grep -q '"signal":"Killed: 9"'
}

# Scheduled jobs run under a real launchd job too (RunAtLoad, no KeepAlive):
# a process cannot fake XPC_SERVICE_NAME — libxpc aborts it at start (rc 134),
# and launchd is the only thing that makes a run "the service".
write_once() {  # write_once <script>
  write_once_args "<string>/bin/bash</string><string>$1</string>"
}

write_once_args() {  # write_once_args <ProgramArguments strings> — a RunAtLoad job
  # Loaded once. Loading the KeepAlive job first, booting it out and loading
  # again raced: bootout returns before launchd drops the label, and the second
  # bootstrap failed with "Bootstrap failed: 5" (runs 9 and 15 of #4446).
  write_plist "$1"
  /usr/bin/sed -i '' 's#<key>KeepAlive</key><true/>#<key>RunAtLoad</key><true/>#' "$PLIST"
  launchctl bootstrap "gui/$UIDN" "$PLIST"
}

# wait_exit — until launchd records the run's exit
wait_exit() {
  for _ in $(seq 1 100); do
    launchctl print "gui/$UIDN/$LABEL" 2>/dev/null | grep -q $'^\tlast exit code = [0-9-]' && return 0
    sleep 0.1
  done
  return 1
}

job_script() {  # job_script <exit code> — a job that ends with its own trap + exec
  # set -euo pipefail like most real jobs: bash 3.2 calls an empty array unbound
  cat > "$FIXHOME/job.sh" <<SH
#!/bin/bash
set -euo pipefail
. "$LIB"
service_lifecycle_job com.chorus.fixture-job "\$@"
trap 'echo cleanup > "$FIXHOME/trap-ran"' EXIT
exec /bin/sh -c 'exit $1'
SH
}

@test "bash job: a run that exits non-zero is service.failed, even past its own trap and exec" {
  job_script 4
  write_once "$FIXHOME/job.sh"
  wait_exit
  sleep 0.5
  line="$(grep '"event":"service.failed"' "$SPINE" | tail -1)"
  echo "line=$line"
  [[ "$line" == *"\"service\":\"$LABEL\""* ]] || return 1
  [[ "$line" =~ \"exit_code\":\"?4\"?[,}] ]] || return 1
  [[ "$line" == *'"level":"error"'* ]] || return 1
  # the run began with a start, and a failed run is not also a clean stop
  grep '"event":"service.started"' "$SPINE" | grep -q "\"service\":\"$LABEL\"" || return 1
  ! grep -q '"event":"service.stopped"' "$SPINE" || return 1
}

@test "bash job: a run that succeeds is service.started then service.stopped, never failed" {
  job_script 0
  write_once "$FIXHOME/job.sh"
  wait_exit
  sleep 0.5
  cat "$SPINE"
  grep '"event":"service.started"' "$SPINE" | grep "\"service\":\"$LABEL\"" | grep -q '"version":"[0-9a-f]\{12\}"' || return 1
  grep '"event":"service.stopped"' "$SPINE" | grep "\"service\":\"$LABEL\"" | grep -q '"reason":"exit 0"' || return 1
  ! grep -q '"event":"service.failed"' "$SPINE" || return 1
}

@test "NEGATIVE PROOF: the same job run by hand (not by launchd) writes nothing when it fails" {
  job_script 4
  HOME="$FIXHOME" CHORUS_LOG_FILE="$SPINE" CHORUS_CONTEXT=test CHORUS_ROOT="$TREE" CHORUS_HOME="$TREE" bash "$FIXHOME/job.sh" || true
  [[ ! -s "$SPINE" ]] || { cat "$SPINE"; return 1; }
}

# --- Rust scheduled jobs: service_lifecycle::run_as_job() (the shim here) ---
SHIM="$TREE/platform/services/chorus-hooks/target/release/chorus-hook-shim"
shim_once() {  # shim_once <verb>
  [[ -x "$SHIM" ]] || skip "release binary not built: $SHIM"
  write_job "<string>$SHIM</string><string>$1</string>"
  /usr/bin/sed -i '' 's#<key>KeepAlive</key><true/>#<key>RunAtLoad</key><true/>#' "$PLIST"
  launchctl bootout "gui/$UIDN/$LABEL" 2>/dev/null || true
  launchctl bootstrap "gui/$UIDN" "$PLIST"
}

@test "Rust job: a run that exits non-zero is service.failed with its exit code" {
  shim_once no-such-verb-4446
  wait_exit
  sleep 0.5
  line="$(grep '"event":"service.failed"' "$SPINE" | tail -1)"
  echo "line=$line"
  [[ "$line" == *"\"service\":\"$LABEL\""* ]] || return 1
  [[ "$line" =~ \"exit_code\":\"?[1-9][0-9]*\"?[,}] ]] || return 1
}

@test "Rust job: a run that succeeds is service.started then service.stopped, never failed" {
  shim_once wall-clock
  wait_exit
  sleep 0.5
  cat "$SPINE"
  grep '"event":"service.started"' "$SPINE" | grep -q "\"service\":\"$LABEL\"" || return 1
  grep '"event":"service.stopped"' "$SPINE" | grep "\"service\":\"$LABEL\"" | grep -q '"reason":"exit 0"' || return 1
  ! grep -q '"event":"service.failed"' "$SPINE" || return 1
}

# --- python services: platform/scripts/lib/service_lifecycle.py ---
py_job() {
  cat > "$FIXHOME/daemon.py" <<PY
import sys, time
sys.path.insert(0, "$TREE/platform/scripts/lib")
from service_lifecycle import service_lifecycle
service_lifecycle("com.chorus.fixture-py").started()
while True:
    time.sleep(1)
PY
  write_job "<string>/usr/bin/python3</string><string>$FIXHOME/daemon.py</string>"
}

@test "python: start, kickstart → service.stopped SIGTERM, then a new start" {
  py_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"'
}

@test "python: a kill -9 is reported as service.failed by the next start" {
  py_job
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kill -KILL "$first"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.failed"' "$SPINE" | grep -q '"signal":"Killed: 9"'
}

# --- the wiring guard: every installed com.chorus.* service calls a helper ---
GUARD="$TREE/platform/tests/4446-every-service-logs-its-lifecycle.test.sh"

@test "NEGATIVE PROOF: the wiring guard is red for a new service nobody wired" {
  mkdir -p "$FIXHOME/agents"; touch "$FIXHOME/agents/com.chorus.brand-new.plist"
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" CHORUS_ROOT="$TREE" bash "$GUARD"
  echo "$output"
  [[ "$status" -ne 0 ]] || return 1
  [[ "$output" == *"com.chorus.brand-new — not in the wiring table"* ]] || return 1
}

@test "NEGATIVE PROOF: the wiring guard is red when a wired source stops calling the helper" {
  mkdir -p "$FIXHOME/agents" "$FIXHOME/root/src"; touch "$FIXHOME/agents/com.chorus.x.plist"
  echo 'fn main() {}' > "$FIXHOME/root/src/main.rs"
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" CHORUS_ROOT="$FIXHOME/root" \
    SERVICE_WIRING_TABLE=$'com.chorus.x\tsrc/main.rs' bash "$GUARD"
  echo "$output"
  [[ "$status" -ne 0 ]] || return 1
  [[ "$output" == *"src/main.rs calls no lifecycle helper"* ]] || return 1
}

@test "NEGATIVE PROOF: a node service that imports the helper but never calls started() is red" {
  mkdir -p "$FIXHOME/agents" "$FIXHOME/root/src"; touch "$FIXHOME/agents/com.chorus.x.plist"
  printf '%s\n' "const lifecycle = serviceLifecycle('com.chorus.x');" > "$FIXHOME/root/src/service.ts"
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" CHORUS_ROOT="$FIXHOME/root" \
    SERVICE_WIRING_TABLE=$'com.chorus.x\tsrc/service.ts' bash "$GUARD"
  echo "$output"
  [[ "$status" -ne 0 && "$output" == *"never calls started()"* ]] || return 1
  echo "lifecycle.started();" >> "$FIXHOME/root/src/service.ts"
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" CHORUS_ROOT="$FIXHOME/root" \
    SERVICE_WIRING_TABLE=$'com.chorus.x\tsrc/service.ts' bash "$GUARD"
  [[ "$status" -eq 0 && "$output" == *"1 wired"* ]] || { echo "$output"; return 1; }
}

plist_args() {  # plist_args <file> <program...> — a minimal plist with these ProgramArguments
  local f="$1"; shift
  { echo '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>ProgramArguments</key><array>'
    for a in "$@"; do echo "<string>$a</string>"; done
    echo '</array></dict></plist>'; } > "$f"
}

@test "NEGATIVE PROOF: a WRAP service whose plist skips service-run is red once service-run is installed" {
  mkdir -p "$FIXHOME/agents"; touch "$FIXHOME/service-run"; chmod +x "$FIXHOME/service-run"
  plist_args "$FIXHOME/agents/com.chorus.y.plist" /bin/bash -c 'echo hi'
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" SERVICE_RUN="$FIXHOME/service-run" \
    SERVICE_WIRING_TABLE=$'com.chorus.y\tWRAP: inline bash -c in the plist' bash "$GUARD"
  echo "$output"
  [[ "$status" -ne 0 ]] || return 1
  [[ "$output" == *"com.chorus.y — inline bash -c in the plist: its plist does not run through service-run"* ]] || return 1
  # wrapped: wired
  plist_args "$FIXHOME/agents/com.chorus.y.plist" "$FIXHOME/service-run" com.chorus.y job /bin/bash -c 'echo hi'
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" SERVICE_RUN="$FIXHOME/service-run" \
    SERVICE_WIRING_TABLE=$'com.chorus.y\tWRAP: inline bash -c in the plist' bash "$GUARD"
  [[ "$status" -eq 0 && "$output" == *"1 wired, 0 pending"* ]] || { echo "$output"; return 1; }
}

@test "a WRAP service before service-run is installed is PENDING: not wired, not red" {
  mkdir -p "$FIXHOME/agents"
  plist_args "$FIXHOME/agents/com.chorus.y.plist" /bin/bash -c 'echo hi'
  run env SERVICE_WIRING_AGENTS="$FIXHOME/agents" SERVICE_RUN="$FIXHOME/no-such-service-run" \
    SERVICE_WIRING_TABLE=$'com.chorus.y\tWRAP: inline bash -c in the plist' bash "$GUARD"
  echo "$output"
  [[ "$status" -eq 0 && "$output" == *"0 wired, 1 pending wrap"* ]] || return 1
}

# --- service-run: the wrapper for plists whose program we do not edit ---
RUN="$TREE/platform/scripts/service-run"

@test "service-run daemon: start, kickstart → service.stopped SIGTERM (child gone), then a new start" {
  write_job "<string>$RUN</string><string>com.chorus.fixture-wrap</string><string>daemon</string><string>/bin/sleep</string><string>600</string>"
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  kid="$(pgrep -P "$first" sleep)"
  [[ -n "$kid" ]] || return 1
  launchctl kickstart -k "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.stopped"' "$SPINE" | grep "\"pid\":\"$first\"" | grep -q '"reason":"SIGTERM"' || return 1
  ! kill -0 "$kid" 2>/dev/null || { echo "child $kid outlived its wrapper"; return 1; }
}

@test "service-run daemon: a kill -9 is reported as service.failed by the next start" {
  write_job "<string>$RUN</string><string>com.chorus.fixture-wrap</string><string>daemon</string><string>/bin/sleep</string><string>600</string>"
  first="$(wait_pid_change "")"
  restarted_with_start "$first"
  launchctl kill SIGKILL "gui/$UIDN/$LABEL"
  second="$(wait_pid_change "$first")"
  restarted_with_start "$second"
  grep '"event":"service.failed"' "$SPINE" | grep -q '"signal":"Killed: 9"'
}

@test "service-run job: a run that exits non-zero is service.started then service.failed with its code" {
  write_once_args "<string>$RUN</string><string>x</string><string>job</string><string>/bin/sh</string><string>-c</string><string>exit 5</string>"
  wait_exit
  sleep 0.5
  cat "$SPINE"
  grep '"event":"service.started"' "$SPINE" | grep -q "\"service\":\"$LABEL\"" || return 1
  line="$(grep '"event":"service.failed"' "$SPINE" | tail -1)"
  [[ "$line" == *"\"service\":\"$LABEL\""* ]] || return 1
  [[ "$line" =~ \"exit_code\":\"?5\"?[,}] ]] || return 1
}

@test "service-run job: a run that succeeds is service.started then service.stopped, never failed" {
  write_once_args "<string>$RUN</string><string>x</string><string>job</string><string>/usr/bin/true</string>"
  wait_exit
  sleep 0.5
  grep '"event":"service.stopped"' "$SPINE" | grep "\"service\":\"$LABEL\"" | grep -q '"reason":"exit 0"' || return 1
  ! grep -q '"event":"service.failed"' "$SPINE" || return 1
}

# --- error handling: a service that refuses to start says so in the log ---
@test "python: share-guard refusing a non-loopback bind exits 2 and logs service.failed with the reason" {
  run env HOME="$FIXHOME" CHORUS_LOG_FILE="$SPINE" CHORUS_CONTEXT=test CHORUS_HOME="$TREE" \
    SHARE_BIND=0.0.0.0 /usr/bin/python3 "$TREE/platform/scripts/chorus-share-guard.py"
  [ "$status" -eq 2 ] || { echo "status=$status $output"; return 1; }
  line="$(grep '"event":"service.failed"' "$SPINE" | tail -1)"
  echo "line=$line"
  [[ "$line" == *'"service":"com.chorus.share-guard"'* ]] || return 1
  [[ "$line" == *'refusing'* ]] || return 1
  [[ "$line" =~ \"exit_code\":\"?2\"?[,}] ]] || return 1
}
