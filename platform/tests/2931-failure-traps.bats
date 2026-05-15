#!/usr/bin/env bats
# #2931 AC5 regression — ERR/EXIT failure traps in build-signed.sh and
# deploy-daemon-card.sh must (a) emit a structured failure event AND
# (b) propagate the original non-zero exit code. Silas's gate:arch
# watch-item: "ERR-trap inside set -e can swallow exit codes — earns
# its keep as a regression test."
#
# Strategy: extract the trap+emit primitives into a minimal harness and
# assert behavior directly. Testing the full scripts requires real
# cargo / chorus-deploy, which is out of scope for unit-level regression.

setup() {
  TMPDIR_T=$(mktemp -d -t 2931-traps.XXXXXX)
  EMITTED="$TMPDIR_T/emitted"
  : > "$EMITTED"
}

teardown() {
  rm -rf "$TMPDIR_T"
}

@test "build-signed-style ERR trap: emits event AND propagates exit code" {
  cat > "$TMPDIR_T/script.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
_emit() {
  local exit_code="\$1" line_no="\$2" cmd="\$3"
  echo "build.failed result=fail error=\"line=\$line_no cmd=\$cmd exit_code=\$exit_code\"" >> "$EMITTED"
}
trap '_emit \$? \$LINENO "\$BASH_COMMAND"' ERR
echo "before-fail"
false   # forced failure on a known line
echo "should-not-print"
EOF
  chmod +x "$TMPDIR_T/script.sh"
  run "$TMPDIR_T/script.sh"

  # Half 1: trap emitted a structured failure event.
  [ -s "$EMITTED" ]
  grep -q "build.failed" "$EMITTED"
  grep -q "result=fail" "$EMITTED"
  grep -q "exit_code=1" "$EMITTED"
  grep -q "cmd=false" "$EMITTED"

  # Half 2: original non-zero exit code propagates (set -e does not swallow).
  [ "$status" -ne 0 ]

  # Half 3: script halts at the failing command — line after `false` does NOT execute.
  [[ "$output" == *"before-fail"* ]]
  [[ "$output" != *"should-not-print"* ]]
}

@test "deploy-daemon-style EXIT trap: emits once, only on non-zero, propagates code" {
  cat > "$TMPDIR_T/script.sh" <<EOF
#!/bin/bash
set -u
_current_step="init"
_emit() {
  local exit_code="\$?"
  if [ "\$exit_code" -ne 0 ] && [ -z "\${_emitted:-}" ]; then
    _emitted=1
    echo "deploy.failed result=fail error=\"step=\${_current_step} exit=\$exit_code\"" >> "$EMITTED"
  fi
}
trap _emit EXIT
_current_step="probe"
exit 12
EOF
  chmod +x "$TMPDIR_T/script.sh"
  run "$TMPDIR_T/script.sh"

  [ "$status" -eq 12 ]                       # original exit code preserved
  [ "$(wc -l < "$EMITTED" | tr -d ' ')" = "1" ]   # emitted exactly once
  grep -q "deploy.failed" "$EMITTED"
  grep -q "step=probe" "$EMITTED"
  grep -q "exit=12" "$EMITTED"
}

@test "deploy-daemon-style EXIT trap: stays silent on clean exit (no false positives)" {
  cat > "$TMPDIR_T/script.sh" <<EOF
#!/bin/bash
set -u
_current_step="init"
_emit() {
  local exit_code="\$?"
  if [ "\$exit_code" -ne 0 ] && [ -z "\${_emitted:-}" ]; then
    _emitted=1
    echo "deploy.failed" >> "$EMITTED"
  fi
}
trap _emit EXIT
_current_step="done"
exit 0
EOF
  chmod +x "$TMPDIR_T/script.sh"
  run "$TMPDIR_T/script.sh"

  [ "$status" -eq 0 ]
  [ ! -s "$EMITTED" ]   # zero events — clean exit must not emit failure
}
