#!/usr/bin/env bats
# #2944 / #3026 — stale-base detection in git-queue.sh push.
#
# #2944 ORIGINAL premise: a long-lived werk branch silently deletes peer-merged
# files because git's 3-way merge drops "present in main, absent in branch".
# #3026 CORRECTION: that premise does NOT hold for the actual workflow. Push
# targets the role branch; /acp then squash-merges, applying ONLY the branch's
# own changeset — so a peer-added file absent from a stale branch SURVIVES the
# squash (verified empirically: it's added on main's side, untouched on the
# branch's, so the 3-way keeps it). The "deletions" are 2-way-diff artifacts,
# not real reverts. Hard-blocking on them jammed the whole team (2026-05-21),
# so #3026 downgraded the BLOCK to a non-fatal NOTE: detect + name the stale
# files (so the operator can rebase for clean history), but never refuse.
# These tests pin the note-not-block contract.
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

# Helper — mirrors git-queue.sh's stale-base check AFTER #3026. The detection
# logic is unchanged (it computes which peer-added files are absent from the
# stale branch), but the contract is now NOTE-not-BLOCK: it ALWAYS returns 0
# (the push proceeds) and prints any detected files so callers can assert what
# the note would name. The squash-merge keeps these files (verified), so a
# stale base is no longer a refusal.
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
  fi
  return 0  # #3026 — note, never block
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

@test "stale-base guard (#3026): branch missing peer's c.txt is NOTED but does NOT block" {
  # The werk branch was cut BEFORE peer added c.txt and touches only a.txt.
  # A 2-way diff makes it LOOK like the push deletes c.txt — but the /acp
  # squash-merge applies only the branch's own changeset, so c.txt survives
  # (verified empirically). #3026: detect it for the note, but never refuse.
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a only"

  run _check_stale_base "$WERK"
  [ "$status" -eq 0 ]            # #3026 — no longer a refusal
  [[ "$output" == *"c.txt"* ]]  # still named in the note so the operator can rebase if they want
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

@test "stale-base guard (#3026): the note identifies the exact stale file (without blocking)" {
  echo "kade-change" > "$WERK/a.txt"
  git -C "$WERK" add . && git -C "$WERK" commit -q -m "kade: touch a only"

  run _check_stale_base "$WERK"
  [ "$status" -eq 0 ]  # #3026 — note, not refusal
  # c.txt is the only peer-added file the branch never touched — named in the note.
  [ "$(echo "$output" | wc -l | tr -d ' ')" -eq 1 ]
  [[ "$output" == "c.txt" ]]
}
