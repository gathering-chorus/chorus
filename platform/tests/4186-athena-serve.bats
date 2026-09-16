#!/usr/bin/env bats
# @test-type: integration — drives the built athena-serve binary with a stub launchctl and a stub curl; no launchd, no network.
#
# #4186 — the SERVE leg of the athena pipeline. The land used to seed rows
# against an athena-make that launchd called "running" and that answered
# nothing (POST → 0, six lands this week). These proofs pin the one rule:
# nothing downstream runs until the HEALTH endpoint says healthy.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  BIN="${ATHENA_SERVE_BIN:-$ROOT/platform/services/athena-serve/target/release/athena-serve}"
  [ -x "$BIN" ] || skip "athena-serve not built at $BIN"
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/bin"
  # stub launchctl: records the call; fails if a marker says so
  cat > "$T/bin/launchctl" <<EOS
#!/bin/bash
echo "launchctl \$*" >> "$T/launchctl.log"
[ -f "$T/kickstart-fails" ] && { echo "Could not find service" >&2; exit 113; }
exit 0
EOS
  # stub curl: answers from answers.txt, one "CODE|BODY" line per call, last line repeats
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
n=\$(cat "$T/calls" 2>/dev/null || echo 0); n=\$((n+1)); echo \$n > "$T/calls"
line=\$(sed -n "\${n}p" "$T/answers.txt"); [ -n "\$line" ] || line=\$(tail -1 "$T/answers.txt")
code="\${line%%|*}"; body="\${line#*|}"
printf '%s\n%s' "\$body" "\$code"
EOS
  chmod +x "$T/bin/"*
  export LAUNCHCTL_BIN="$T/bin/launchctl" CURL_BIN="$T/bin/curl" ATHENA_SERVE_POLL_MS=50
  unset CHORUS_LOG
}

@test "healthy on the first probe → exit 0, one line naming the service and url" {
  echo '200|{"status":"healthy"}' > "$T/answers.txt"
  run "$BIN" com.chorus.athena-make http://h:1/health
  [ "$status" -eq 0 ]
  [[ "$output" == *"com.chorus.athena-make answers healthy at http://h:1/health"* ]]
  [ ! -f "$T/launchctl.log" ]
}

@test "--kickstart restarts the label first, then waits for health" {
  printf '000|\n000|\n200|{"status":"ok"}\n' > "$T/answers.txt"
  run "$BIN" com.chorus.athena-make http://h:1/health --kickstart --timeout 5
  [ "$status" -eq 0 ]
  grep -q "launchctl kickstart -k gui/$(id -u)/com.chorus.athena-make" "$T/launchctl.log"
  [ "$(cat "$T/calls")" -eq 3 ]
}

@test "NEGATIVE PROOF — launchd 'running' is not served: health never answers → REFUSED, exit 1, last answer named" {
  echo '000|' > "$T/answers.txt"
  run "$BIN" com.chorus.athena-make http://h:1/health --timeout 1
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED"*"http://h:1/health"*"within 1s"*"HTTP 000"* ]]
}

@test "NEGATIVE PROOF — a 200 whose body is not a status is not healthy (the 6.401ms class)" {
  echo '200|{"latency":"6.401ms","note":"ok"}' > "$T/answers.txt"
  run "$BIN" x http://h:1/health --timeout 1
  [ "$status" -eq 1 ]
  [[ "$output" == *"HTTP 200"* ]]
}

@test "NEGATIVE PROOF — a failed kickstart REFUSES before any probe" {
  touch "$T/kickstart-fails"; echo '200|{"status":"healthy"}' > "$T/answers.txt"
  run "$BIN" com.chorus.athena-make http://h:1/health --kickstart
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED"*"kickstart"*"Could not find service"* ]]
  [ ! -f "$T/calls" ]
}

@test "usage: missing url is exit 2" {
  run "$BIN" only-label
  [ "$status" -eq 2 ]
}
