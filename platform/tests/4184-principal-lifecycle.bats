#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-awake binary (Rust, #4184) with stub claude, stub tmux, stub ps and a temp session registry; no live tmux, no live claude.
# @domain: roles — the product domain this suite guards (#4334)
#
# #4184 — Jeff: "a standard script to start each of u that initalizes u and
# makes sure i do the steps". Every outside thing is stubbed and RECORDS its
# calls, so each proof asserts what the script actually sent, not its output.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  # #4409 — the shared fixture world (lib/login-harness.bash); only what this
  # suite needs differently is set below.
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  # one role, one window: has-session answers from a marker; registration is
  # simulated by the cases themselves, so send-keys registers nothing
  cat > "$T/bin/tmux" <<EOS
#!/bin/bash
echo "tmux \$*" >> "$T/tmux.log"
case "\$1" in has-session) [ -f "$T/tmux-session-exists" ]; exit \$? ;; new-session) touch "$T/tmux-session-exists" ;; esac
exit 0
EOS
  chmod +x "$T/bin/tmux"
  export AWAKE_ROLE_DIR="$T/roles/kade" AWAKE_WAIT=1
  # #4295: no live service probes from a unit world; no background retry left running
  export AWAKE_SERVICES=none AWAKE_NO_RETRY=1
  echo '[]' > "$T/agents.json"
  # #4215 — the mute check reads the spine. Point it at a fixture that says kade
  # answered a moment ago, so "already awake" means the same thing on every box.
  printf '{"role":"kade","event":"reply.published","timestamp":"%s"}\n' "$(date '+%Y-%m-%dT%H:%M:%S')" > "$T/spine-read.log"
}

reg() { # reg <pid> [pane]
  printf '{"role":"kade","pid":%s,"tty":"/dev/ttys00%s","host":"tmux","tmux":"%s","registered_at":"1"}' "$1" "${1: -1}" "${2:-%0}" > "$T/sessions/kade-$1.json"
  echo "$1" >> "$T/alive-pids"
}

@test "usage: unknown role is exit 2, not a silent start" {
  # #4368: unknown = no Principal row, not "not in a list of three"
  run "$SCRIPT" bob
  [ "$status" -eq 2 ]
  [ ! -f "$T/tmux.log" ]
  printf '%s' "$output" | grep -qF "no Principal row named bob"
}

@test "idempotent: one live tmux session → the proof line, and NOTHING is sent" {
  reg 56344 %0
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  # #4295 — the line names the login, never "registered yes"; nothing is LAUNCHED
  # (the tmux bar is set, so tmux is called, but no keys are sent)
  printf '%s' "$output" | grep -qF "awake: kade  pid 56344  tty /dev/ttys004  pane %0  logged in  via already awake"
  test -z "$(grep -F send-keys "$T/tmux.log" 2>/dev/null || true)"
  [ ! -f "$T/claude.log" ]
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"pid 56344"* ]] || return 1
}

@test "NEGATIVE PROOF — two live sessions for one role → REFUSED, both named, nothing sent" {
  reg 111 %0; reg 222 %1
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED"* ]] || return 1
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
  printf '%s' "$output" | grep -qF "pane %5  logged in  via claude -c"
}

# #4337 changed HOW the detached conversation is picked up (attach ran it inside the
# daemon's warm spare, which carried another role's env on 09-26); the promise this
# test guards — the same conversation, never a fresh -c — is unchanged.
@test "NEGATIVE PROOF — a detached background conversation is RESUMED in the pane, never restarted with -c" {
  cat > "$T/agents.json" <<EOS
[{"pid":"87866","id":"79906dc2","cwd":"$T/roles/kade","kind":"background","startedAt":"$(( $(date +%s) * 1000 - 60000 ))","sessionId":"79906dc2-1681","name":"test-verification-workflow","state":"working"}]
EOS
  touch "$T/projects/79906dc2-1681.jsonl"
  ( sleep 0.3; reg 88 %2 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  grep -q -- "claude --resume 79906dc2" "$T/tmux.log"
  grep -q -- "stop 79906dc2" "$T/claude.log"
  ! grep -q -- "claude attach" "$T/tmux.log" || return 1
  ! grep -q -- "claude -c" "$T/tmux.log" || return 1
  printf '%s' "$output" | grep -qF "resumed 79906dc2 in its own pane"
}

@test "registration never appears → exit 1 and the line says registered NO" {
  run "$SCRIPT" kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"registered NO after 1s"* ]] || return 1
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
  [[ "$output" == *"stale: 2 background agent(s)"*"aaaa1111,bbbb2222"* ]] || return 1
  ! grep -q "claude rm" "$T/claude.log" || return 1
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
  ! grep -qE "claude'? Enter|claude agents" "$T/tmux.log" || return 1
}

@test "#4215 the background-session list cannot be read → the role STARTS on the last conversation, loudly" {
  # This test asserted the opposite until 2026-09-19: an unreadable list refused,
  # "never guess -c". The guess costs a duplicate pane Jeff can close; the
  # refusal costs him the role, which is the failure this card exists to end.
  touch "$T/agents-fail"
  ( sleep 0.3; reg 91 %0 ) &
  run "$SCRIPT" kade
  printf '%s' "$output" | grep -q "could not list kade's background sessions"
  printf '%s' "$output" | grep -q "unknown option --cwd"
  printf '%s' "$output" | grep -q "continuing with the last conversation"
  grep -q "claude -c" "$T/tmux.log"
  # and it never pretends the list was read
  test -z "$(printf '%s' "$output" | grep -F "already awake" || true)"
}

@test "NEGATIVE PROOF — an OLDER background helper is never attached when a newer conversation exists (first live run defect)" {
  cat > "$T/agents.json" <<EOS
[{"pid":"1","id":"8faa3fa4","cwd":"$T/roles/kade","kind":"background","startedAt":"$(( $(date +%s) * 1000 - 3600000 ))","sessionId":"8faa3fa4-80ae","name":"old-helper","state":"working"}]
EOS
  touch "$T/projects/8faa3fa4-80ae.jsonl"; sleep 1; touch "$T/projects/38b6cebe-6041.jsonl"
  ( sleep 0.3; reg 44 %0 ) &
  run "$SCRIPT" kade
  [ "$status" -eq 0 ]
  ! grep -q -- "claude attach" "$T/tmux.log" || return 1
  grep -q -- "claude -c" "$T/tmux.log"
  [[ "$output" == *"via claude -c"* ]] || return 1
}

# ---- #4219 — turn the key, the car starts ----
# Jeff, 2026-09-19: `chorus-awake kade` came up on a conversation the API had
# refused 14 times, so every line he typed came back an error until he cleared
# it by hand. "i just want it to work like turn a key to start the car."

_poison() { printf '{"content":"API Error: safeguards flagged this message"}\n' >> "$T/projects/$1.jsonl"; }

@test "#4219 a conversation ending in API refusals is NOT resumed — a fresh one starts" {
  touch "$T/projects/dead-conv.jsonl"; _poison dead-conv
  ( sleep 0.3; reg 71 %0 ) &
  run "$SCRIPT" kade
  printf '%s' "$output" | grep -q "ends in API refusals"
  printf '%s' "$output" | grep -q "fresh conversation"
  # the pane gets a plain claude, not -c
  test -z "$(grep -F -- "claude -c" "$T/tmux.log" || true)"
}

@test "#4219 a healthy conversation is still resumed with -c" {
  printf '{"type":"assistant","text":"fine"}\n' > "$T/projects/live-conv.jsonl"
  ( sleep 0.3; reg 72 %0 ) &
  run "$SCRIPT" kade
  grep -q -- "claude -c" "$T/tmux.log"
  test -z "$(printf '%s' "$output" | grep -F "ends in API refusals" || true)"
}

@test "#4219 NEGATIVE PROOF: with the check off, the SAME poisoned conversation is resumed" {
  touch "$T/projects/dead-conv.jsonl"; _poison dead-conv
  ( sleep 0.3; reg 73 %0 ) &
  run env AWAKE_TRANSCRIPT_CHECK=0 "$SCRIPT" kade
  grep -q -- "claude -c" "$T/tmux.log"
  test -z "$(printf '%s' "$output" | grep -F "fresh conversation" || true)"
}
