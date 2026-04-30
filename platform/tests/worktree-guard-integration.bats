#!/usr/bin/env bats
# worktree-guard-integration.bats — #2626 follow-on
#
# Integration test for the worktree contamination guard hook (#2625) as
# invoked through the actual chorus-hook-shim binary. Complements the
# unit tests in src/hooks/worktree_contamination_guard.rs by exercising
# the full JSON → shim → daemon → response path.
#
# Why this exists: today (2026-04-30) the hook shipped twice with green
# unit tests but broke the team in production. The unit tests covered
# cases the author had thought of; the bug class (separators inside
# quoted bodies) was not in the test set. These integration tests run
# realistic Bash command JSON through the shim and assert the actual
# block/allow behavior at the binary level — the same surface that
# Claude Code's Bash tool hits.
#
# Owner: silas (chorus-hooks).

SHIM="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim"
INIT_DIR="/tmp/claude-session-init"

setup() {
  # Ensure session-init markers exist so the shim's session-init gate
  # doesn't fire and mask the worktree-guard behavior we're testing.
  mkdir -p "$INIT_DIR"
  for role in silas wren kade; do
    touch "$INIT_DIR/$role.done"
  done
}

# --- helpers ---

probe() {
  local cmd="$1"
  local cwd="${2:-/Users/jeffbridwell/CascadeProjects/chorus}"
  local role="${3:-silas}"
  local json
  json=$(python3 -c "
import json, sys
print(json.dumps({
  'tool_name': 'Bash',
  'tool_input': {'command': sys.argv[1]},
  'cwd': sys.argv[2],
  'deploy_role': sys.argv[3]
}))" "$cmd" "$cwd" "$role")
  echo "$json" | "$SHIM" pre-tool-use 2>&1
  return $?
}

# --- pre-flight ---

@test "shim binary exists and is executable" {
  [ -x "$SHIM" ]
}

# --- core invariant: dangerous git on canonical /chorus is blocked ---

@test "BLOCK: bare git checkout on canonical /chorus" {
  run probe "git checkout main" "/Users/jeffbridwell/CascadeProjects/chorus"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

@test "BLOCK: bare git pull on canonical /chorus" {
  run probe "git pull" "/Users/jeffbridwell/CascadeProjects/chorus"
  [ "$status" -eq 2 ]
}

@test "BLOCK: bare git reset --hard on canonical /chorus" {
  run probe "git reset --hard HEAD" "/Users/jeffbridwell/CascadeProjects/chorus"
  [ "$status" -eq 2 ]
}

@test "BLOCK: cd to canonical /chorus then git checkout" {
  run probe "cd /Users/jeffbridwell/CascadeProjects/chorus && git checkout main" "/tmp"
  [ "$status" -eq 2 ]
}

# --- exempt: per-role worktrees ---

@test "ALLOW: git checkout from per-role worktree (input.cwd)" {
  run probe "git checkout main" "/Users/jeffbridwell/CascadeProjects/chorus-silas"
  [ "$status" -eq 0 ]
}

@test "ALLOW: cd /chorus-silas then git checkout (cd-prefix cwd)" {
  run probe "cd /Users/jeffbridwell/CascadeProjects/chorus-silas && git checkout main"
  [ "$status" -eq 0 ]
}

@test "ALLOW: cd /chorus-2526 topic worktree then git pull" {
  run probe "cd /Users/jeffbridwell/CascadeProjects/chorus-2526 && git pull"
  [ "$status" -eq 0 ]
}

# --- exempt: non-dangerous git ---

@test "ALLOW: git status on canonical /chorus" {
  run probe "git status"
  [ "$status" -eq 0 ]
}

@test "ALLOW: git log on canonical /chorus" {
  run probe "git log --oneline -5"
  [ "$status" -eq 0 ]
}

@test "ALLOW: git config on canonical /chorus" {
  run probe "git config user.email"
  [ "$status" -eq 0 ]
}

# --- the bug class: dangerous text inside quoted bodies ---
# These are the cases that broke the team in production today and that
# the prior unit-test set missed. They exercise the quote-aware split.

@test "ALLOW: nudge body with semicolon between dangerous-keyword fragments" {
  run probe 'nudge wren "first; git checkout main; done" --from silas'
  [ "$status" -eq 0 ]
}

@test "ALLOW: nudge body with && between dangerous-keyword fragments" {
  run probe 'nudge wren "build && git pull && deploy" --from silas'
  [ "$status" -eq 0 ]
}

@test "ALLOW: nudge body with pipe between dangerous-keyword fragments" {
  run probe 'nudge wren "see git reset | review later" --from silas'
  [ "$status" -eq 0 ]
}

@test "ALLOW: echo of literal dangerous-git chain (wren reproduction)" {
  run probe 'echo "first; git checkout main; done"'
  [ "$status" -eq 0 ]
}

@test "ALLOW: chat.sh say with dangerous-git chain in body" {
  run probe 'bash platform/scripts/chat.sh say chat-123 silas "first git checkout && then git pull"'
  [ "$status" -eq 0 ]
}

@test "ALLOW: git-queue commit with dangerous-keyword in -m message" {
  run probe 'DEPLOY_ROLE=silas bash platform/scripts/git-queue.sh commit foo.rs -- -m "fix: avoid git checkout && git reset race"'
  [ "$status" -eq 0 ]
}

@test "ALLOW: cards update --desc containing dangerous-git chain" {
  run probe 'cards update 2626 --desc "before: git checkout main; after: git reset hard"'
  [ "$status" -eq 0 ]
}

@test "ALLOW: gh pr merge -m with dangerous-keyword in body" {
  run probe 'gh pr merge 73 --squash --delete-branch -m "merge git checkout fix"'
  [ "$status" -eq 0 ]
}

# --- escapes ---

@test "ALLOW: magic-comment override on canonical /chorus" {
  run probe "git checkout main  # worktree-override"
  [ "$status" -eq 0 ]
}

@test "ALLOW: magic-comment override (no space variant)" {
  run probe "git pull #worktree-override"
  [ "$status" -eq 0 ]
}

# --- realistic combined flows ---

@test "ALLOW: real silas nudge body containing cd /chorus-X && git checkout literal" {
  run probe 'bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge wren "[#2626] worktree-guard RCA shipped — 3/3 probes pass: block bare git checkout from /chorus, allow cd /chorus-silas && git checkout, allow git-queue wrapper" --from silas'
  [ "$status" -eq 0 ]
}

@test "ALLOW: chained safe ops with cd to per-role worktree" {
  run probe "cd /Users/jeffbridwell/CascadeProjects/chorus-silas && git status && git log -1"
  [ "$status" -eq 0 ]
}
