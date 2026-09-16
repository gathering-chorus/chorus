#!/usr/bin/env bats
# @test-type: integration — drives platform/scripts/chorus-awake with stub claude, stub tmux, stub ps and a temp session registry; no live tmux, no live claude.
#
# #4184 — Jeff: "a standard script to start each of u that initalizes u and
# makes sure i do the steps". Every outside thing is stubbed and RECORDS its
# calls, so each proof asserts what the script actually sent, not its output.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/chorus-awake"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/sessions" "$T/bin" "$T/roles/kade"
  touch "$T/alive-pids"
  # stub ps: alive iff the pid is listed in alive-pids
  cat > "$T/bin/ps" <<EOS
#!/bin/bash
grep -qx "\$2" "$T/alive-pids"
EOS
  # stub tmux: records every call; has-session answers from a marker file
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
case "\$1" in has-session) [ -f "$T/tmux-session-exists" ]; exit \$? ;; new-session) touch "$T/tmux-session-exists" ;; esac
exit 0
EOS
  # stub claude: records calls; agents --json prints the fixture; send-keys is
  # never executed (tmux is a stub) so registration is simulated by the test
  cat > "$T/bin/claude" <<EOS
#!/bin/bash
echo "claude \$*" >> "$T/claude.log"
if [ "\$1" = "agents" ]; then cat "$T/agents.json" 2>/dev/null || echo '[]'; fi
exit 0
EOS
  chmod +x "$T/bin/"*
  echo '[]' > "$T/agents.json"
  export CLAUDE_BIN="$T/bin/claude" TMUX_BIN="$T/bin/tmux" AWAKE_PS="$T/bin/ps"
  export CHORUS_SESSIONS_DIR="$T/sessions" AWAKE_ROLE_DIR="$T/roles/kade" CHORUS_ROOT="$ROOT"
  export AWAKE_NO_ATTACH=1 AWAKE_WAIT=1
}

reg() { # reg <pid> [pane]
  printf '{"role":"kade","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%s","registered_at":"1"}' "$1" "${1: -1}" "${2:-%0}" > "$T/sessions/kade-$1.json"
  echo "$1" >> "$T/alive-pids"
}

@test "usage: unknown role is exit 2, not a silent start" {
  run "$SCRIPT" bob
  [ "$status" -eq 2 ]
  [ ! -f "$T/tmux.log" ]
}

@test "idempotent: one live tmux session → the proof line, and NOTHING is sent" {
  reg 56344 %0
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"awake: kade  pid 56344"*"pane %0  registered yes  via already awake" ]]
  [ ! -f "$T/tmux.log" ]
  [ ! -f "$T/claude.log" ]
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"pid 56344"* ]]
}

@test "NEGATIVE PROOF — two live sessions for one role → REFUSED, both named, nothing sent" {
  reg 111 %0; reg 222 %1
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED"* ]]
  [[ "$output" == *"pid 111"* ]] && [[ "$output" == *"pid 222"* ]]
  [ ! -f "$T/tmux.log" ]
}

@test "a dead registry entry is not a live session — cold start proceeds" {
  printf '{"role":"kade","pid":999,"tty":"/dev/ttys009","host":"tmux","tmux":"%%3"}' > "$T/sessions/kade-999.json"
  # pid 999 is NOT in alive-pids; simulate the launched claude registering itself
  ( sleep 0.3; reg 777 %5 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  grep -q "tmux new-session -d -s chorus-kade" "$T/tmux.log"
  grep -q "send-keys -t chorus-kade" "$T/tmux.log"
  grep -q -- "$T/bin/claude -c" "$T/tmux.log"
  [[ "$output" == *"pid 777"*"pane %5  registered yes  via claude -c"* ]]
}

@test "NEGATIVE PROOF — a detached background conversation is ATTACHED, never restarted with -c" {
  cat > "$T/agents.json" <<EOS
[{"pid":"87866","id":"79906dc2","cwd":"$T/roles/kade","kind":"background","startedAt":"$(( $(date +%s) * 1000 - 60000 ))","sessionId":"79906dc2-1681","name":"test-verification-workflow","state":"working"}]
EOS
  ( sleep 0.3; reg 88 %2 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  grep -q -- "claude attach 79906dc2" "$T/tmux.log"
  ! grep -q -- "claude -c" "$T/tmux.log"
  [[ "$output" == *"via attach 79906dc2"* ]]
}

@test "registration never appears → exit 1 and the line says registered NO" {
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"registered NO after 1s"* ]]
}

@test "stale background agents are NAMED, and ended only with AWAKE_END_STALE=1" {
  old=$(( $(date +%s) * 1000 - 30 * 3600 * 1000 ))
  cat > "$T/agents.json" <<EOS
[{"id":"aaaa1111","cwd":"$T/roles/kade","kind":"background","startedAt":"$old","sessionId":"aaaa","name":"old-one","state":"blocked"},
 {"id":"bbbb2222","cwd":"$T/roles/kade","kind":"background","startedAt":"$old","sessionId":"bbbb","name":"old-two","state":"blocked"}]
EOS
  ( sleep 0.3; reg 5 %0 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"stale: 2 background agent(s)"*"aaaa1111,bbbb2222"* ]]
  ! grep -q "claude rm" "$T/claude.log"
  rm -f "$T/sessions/kade-5.json"; : > "$T/alive-pids"; rm -f "$T/tmux.log"
  ( sleep 0.3; reg 6 %0 ) &
  export AWAKE_END_STALE=1
  run "$SCRIPT" kade
  unset AWAKE_END_STALE
  [ "$status" -eq 0 ]
  grep -q "claude rm aaaa1111" "$T/claude.log"
  grep -q "claude rm bbbb2222" "$T/claude.log"
}

@test "the picker is never used: no bare 'claude' and no 'claude agents' is ever sent to the pane" {
  ( sleep 0.3; reg 9 %0 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  ! grep -qE "claude'? Enter|claude agents" "$T/tmux.log"
}
