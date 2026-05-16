#!/usr/bin/env bats
# #2944 — stale-base silent-deletion guard in git-queue.sh push.
#
# Bug class (three receipts 2026-05-15/16): long-lived werk branch silently
# deletes peer-merged files because git's 3-way merge treats "file present
# in current main, absent in branch history" as a deletion (no conflict).
# This guard catches it at push time before the bad commit lands on main.
#
# Strategy: build self-contained git fixtures (origin/main + a stale werk
# branch) and exercise the guard's pure-bash decision logic in isolation
# from the rest of do_push. The decision logic — what counts as a silent
# deletion — is what we need to pin. The fixtures stand in for the real
# canonical+werk pair without needing a working git-queue.sh + lockfile +
# spine emission infrastructure on the test runner.

setup() {
  TMPROOT=$(mktemp -d -t 2944-stale.XXXXXX)
  ORIGIN="$TMPROOT/origin.git"
  WERK="$TMPROOT/werk"

  # Bare origin with HEAD pointing at main (so clone gets a working tree)
  git init -q --bare --initial-branch=main "$ORIGIN"

  # Seed origin/main with baseline files (a.txt + b.txt)
  local seed="$TMPROOT/seed"
  git init -q --initial-branch=main "$seed"
  git -C "$seed" config user.email "test@chorus.local"
  git -C "$seed" config user.name "test"
  echo "base-a" > "$seed/a.txt"
  echo "base-b" > "$seed/b.txt"
  git -C "$seed" add . && git -C "$seed" commit -q -m "seed"
  git -C "$seed" remote add origin "$ORIGIN"
  git -C "$seed" push -q origin main

  # The werk branch (clone of origin at seed time — this is the "stale base")
  git clone -q -b main "$ORIGIN" "$WERK"
  git -C "$WERK" config user.email "test@chorus.local"
  git -C "$WERK" config user.name "test"
  git -C "$WERK" checkout -q -b kade/2944

  # Meanwhile: a peer card lands on origin/main, adding c.txt
  echo "peer-c" > "$seed/c.txt"
  git -C "$seed" add . && git -C "$seed" commit -q -m "peer: add c.txt"
  git -C "$seed" push -q origin main

  # Werk fetches but does NOT rebase — this is the bug-state we're testing.
  git -C "$WERK" fetch -q origin
}

teardown() {
  rm -rf "$TMPROOT"
}

# Helper — runs the same logic git-queue.sh's stale-base check uses.
# Returns 0 (pass) or 1 (silent-deletions found) and prints the file list.
_check_stale_base() {
  local repo="$1"
  local _merge_base
  _merge_base=$(git -C "$repo" merge-base HEAD origin/main 2>/dev/null)
  [ -z "$_merge_base" ] && return 0
  local _branch_deletions _branch_touched _silent_deletions
  _branch_deletions=$(git -C "$repo" diff -M --diff-filter=D --name-only "origin/main..HEAD" 2>/dev/null | sort -u)
  _branch_touched=$(git -C "$repo" diff -M --name-only "$_merge_base..HEAD" 2>/dev/null | sort -u)
  _silent_deletions=$(comm -23 <(printf '%s\n' "$_branch_deletions") <(printf '%s\n' "$_branch_touched") | grep -v '^$' || true)
  if [ -n "$_silent_deletions" ]; then
    printf '%s\n' "$_silent_deletions"
    return 1
  fi
  return 0
}

@test "stale-base guard: clean branch with no peer merges passes" {
  # Make a commit on the werk that genuinely touches its own file. No peer
  # merges have happened since branch-cut (origin/main wasn't moved).
  # NOTE: setup() DID push a peer commit, so we need a fresh fixture for
  # this test — reset origin to seed state.
  git -C "$WERK" fetch -q origin
  git -C "$WERK" reset --hard origin/main 2>/dev/null
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a"

  run _check_stale_base "$WERK"
  [ "$status" -eq 0 ]
}

@test "stale-base guard: branch that doesn't include peer's c.txt triggers refusal" {
  # The werk branch was cut BEFORE peer added c.txt. The werk makes its own
  # commit touching only a.txt. From origin/main's perspective, pushing this
  # branch would "delete" c.txt — silent stale-base ghost.
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a only"

  run _check_stale_base "$WERK"
  [ "$status" -eq 1 ]
  [[ "$output" == *"c.txt"* ]]
}

@test "stale-base guard: deliberate deletion via git rm in branch commit does NOT trigger (not a false-positive)" {
  # Operator intentionally rm's a.txt as part of their branch's work.
  # The deletion appears in branch_touched (git rm touches the file) so
  # it's NOT a silent stale-base ghost.
  git -C "$WERK" rm -q a.txt
  git -C "$WERK" commit -q -m "kade: delete a.txt deliberately"

  # Refresh origin/main reference (peer's c.txt addition still pending)
  git -C "$WERK" fetch -q origin

  run _check_stale_base "$WERK"
  # c.txt is still a silent deletion (peer-added, branch didn't touch).
  # But a.txt was deliberately rm'd by the branch and should NOT appear.
  [[ "$output" != *"a.txt"* ]]
}

@test "stale-base guard: file rename (a.txt → renamed.txt) does NOT false-positive" {
  # -M flag treats rename as rename, not delete+add. The "deleted" name
  # (a.txt) should not appear as a silent-deletion.
  git -C "$WERK" mv a.txt renamed.txt
  git -C "$WERK" commit -q -m "kade: rename a → renamed"

  git -C "$WERK" fetch -q origin

  run _check_stale_base "$WERK"
  [[ "$output" != *"a.txt"* ]]
}

@test "stale-base guard: branch caught up via rebase passes" {
  # Operator does the right thing — rebases onto current origin/main first.
  # After rebase, merge-base catches up and c.txt is part of branch history.
  git -C "$WERK" fetch -q origin
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a"
  git -C "$WERK" rebase -q origin/main

  run _check_stale_base "$WERK"
  [ "$status" -eq 0 ]
}

@test "stale-base guard: silent-deletion message identifies the exact file" {
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a only"

  run _check_stale_base "$WERK"
  [ "$status" -eq 1 ]
  # c.txt is the only file peer added that branch never touched.
  [ "$(echo "$output" | wc -l | tr -d ' ')" -eq 1 ]
  [[ "$output" == "c.txt" ]]
}
