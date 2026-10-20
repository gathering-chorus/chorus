#!/usr/bin/env bats
# @test-type: integration — the real werk verbs, chained, on temp git repos; no live daemon, no network
# @domain: cicd
#
# #3325 — the verb pipeline's story, end to end, in one readable suite.
#
# What Jeff sees: each werk verb has its own tests, but nothing proved the
# HAND-OFFS between them, and the seams were where the failures lived (a
# finalize with no merge, a conflict deadlock, a stale base). This suite runs
# the real binaries one after another, the way a card lands:
#
#   pull → (the role codes) → commit → push → merge → accept
#
# and reads what each one left behind: the board, origin's main, the werk, and
# the spine. It is hermetic: the board, the spine, gh and werk-deploy are stubs
# in platform/tests/fixtures/verb-story; git is real.
#
# Each scenario reads Given / When / Then.

setup_file() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export ROOT BIN="$BATS_FILE_TMPDIR/target/debug"
  for v in pull commit push merge accept; do
    CARGO_TARGET_DIR="$BATS_FILE_TMPDIR/target" cargo build -q \
      --manifest-path "$ROOT/platform/services/werk-$v/Cargo.toml" 2>/dev/null \
      || { echo "werk-$v did not build" >&3; return 1; }
  done
}

# Given: an origin whose main holds f.txt, Chorus's home cloned from it, the
# stubs where the verbs look for them, and card $1 on the board as Next.
given_a_card() {
  R="$BATS_TEST_TMPDIR/r"; mkdir -p "$R/origin"
  git -C "$R/origin" init -q -b main
  printf 'base\n' > "$R/origin/f.txt"
  git -C "$R/origin" add f.txt
  git -C "$R/origin" commit -q -m init
  git -C "$R/origin" config receive.denyCurrentBranch ignore
  git clone -q "$R/origin" "$R/home"
  mkdir -p "$R/home/platform/scripts" "$R/board" "$R/gh" "$R/werks" "$R/bin"
  # the stubs, named by their full paths so werk-test sees this suite exercise each one
  cp "$ROOT/platform/tests/fixtures/verb-story/cards" "$ROOT/platform/tests/fixtures/verb-story/chorus-log" \
     "$ROOT/platform/tests/fixtures/verb-story/role-state" "$R/home/platform/scripts/"
  cp "$ROOT/platform/tests/fixtures/verb-story/gh" "$ROOT/platform/tests/fixtures/verb-story/werk-deploy" "$R/bin/"
  echo "${2:-Next}" > "$R/board/$1"
  export CHORUS_HOME="$R/home" CHORUS_WERK_BASE="$R/werks" CHORUS_BIN="$R/bin" \
    BOARD="$R/board" SPINE="$R/spine" GH_STATE="$R/gh" GH_LOG="$R/ghlog" ORIGIN="$R/origin" \
    PATH="$R/bin:$PATH"
  unset DAG_RUN_ID CHORUS_TRACE_ID ROLE ACCEPTER DEPLOY_ROLE
  : > "$R/spine"
}

# A peer moves origin's main: $1 file, $2 content.
a_peer_moves_main() {
  printf '%s\n' "$2" > "$R/origin/$1"
  git -C "$R/origin" add "$1"
  git -C "$R/origin" commit -q -m "peer moved main"
}

board_says() { [ "$(cat "$R/board/$1")" = "$2" ] || { echo "board #$1 says $(cat "$R/board/$1"), want $2"; return 1; }; }
main_has() { [ "$(git -C "$R/origin" show "main:$1" 2>/dev/null)" = "$2" ] || { echo "main:$1 is not '$2'"; return 1; }; }

# The spine holds these events in this order (other events may sit between).
spine_in_order() {
  local file="$1"; shift
  awk -v want="$*" 'BEGIN { n = split(want, w, " "); i = 1 }
       i <= n && $1 == w[i] { i++ }
       END { if (i <= n) { print "spine never reached " w[i]; exit 1 } }' "$file"
}

@test "story: a card goes from pull to accepted, and each seam leaves its mark on the spine" {
  given_a_card 9001
  # When the role pulls the card
  run "$BIN/werk-pull" 9001 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # Then the card is WIP and its werk sits on its own branch
  board_says 9001 WIP
  [ "$(git -C "$R/werks/kade-9001" rev-parse --abbrev-ref HEAD)" = "kade/9001" ] || return 1

  # When the role codes, then commits and pushes
  printf 'the card\n' > "$R/werks/kade-9001/story.txt"
  run "$BIN/werk-commit" 9001 kade "add the story" --atomic
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  run "$BIN/werk-push" 9001 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # Then origin has the card's branch, and main does not have its work yet
  git -C "$R/origin" rev-parse -q --verify refs/heads/kade/9001 >/dev/null || return 1
  ! git -C "$R/origin" cat-file -e main:story.txt 2>/dev/null || return 1

  # When Jeff's go merges it and accepts it
  ACCEPTER=jeff run "$BIN/werk-merge" 9001 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  ACCEPTER=jeff run "$BIN/werk-accept" 9001 kade
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # Then main has the work, the card is Done, its werk is gone, and the spine tells the story in order
  main_has story.txt "the card"
  board_says 9001 Done
  [ ! -d "$R/werks/kade-9001" ] || { echo "werk still there"; return 1; }
  spine_in_order "$SPINE" card.pulled commit.completed push.completed merge.landed card.accepted card.branch.closed
  grep -q "^pr create" "$GH_LOG" && grep -q "^pr merge" "$GH_LOG" || { echo "gh: $(cat "$GH_LOG")"; return 1; }
}

@test "commit, clean rebase: a peer moved main on another file; the role's commit lands on top of it" {
  given_a_card 9002
  "$BIN/werk-pull" 9002 kade >/dev/null
  printf 'the card\n' > "$R/werks/kade-9002/story.txt"
  # Given main moved after the card was pulled (the stale base)
  a_peer_moves_main other.txt "peer work"
  # When the role commits (the flow mode: commit, then rebase onto current main)
  run "$BIN/werk-commit" 9002 kade "add the story"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # Then the card is one commit ahead of the new main and none behind
  git -C "$R/werks/kade-9002" fetch -q origin main
  [ "$(git -C "$R/werks/kade-9002" rev-list --count origin/main..HEAD)" = 1 ] || return 1
  [ "$(git -C "$R/werks/kade-9002" rev-list --count HEAD..origin/main)" = 0 ] || return 1
  spine_in_order "$SPINE" commit.started commit.completed
}

@test "commit, conflict: held for the role, the role edits the file, --continue lands it, and the merge carries it" {
  given_a_card 9003
  "$BIN/werk-pull" 9003 kade >/dev/null
  printf 'card-version\n' > "$R/werks/kade-9003/f.txt"
  # Given a peer rewrote the same line on main
  a_peer_moves_main f.txt "peer-version"
  # When the role commits (the flow mode rebases, so it meets the peer's line)
  run "$BIN/werk-commit" 9003 kade "edit f"
  # Then the commit holds: markers left in the file, and the role is told the verb's own way on
  [ "$status" -ne 0 ] || { echo "a conflicting commit passed"; return 1; }
  [[ "$output" == *"--continue"* ]] || { echo "$output"; return 1; }
  grep -q '<<<<<<<' "$R/werks/kade-9003/f.txt" || return 1
  # When the role edits the file and continues
  printf 'resolved-version\n' > "$R/werks/kade-9003/f.txt"
  run "$BIN/werk-commit" 9003 kade --continue
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # Then push and merge carry the resolution to main
  "$BIN/werk-push" 9003 kade >/dev/null || return 1
  ACCEPTER=jeff "$BIN/werk-merge" 9003 kade >/dev/null || return 1
  main_has f.txt resolved-version
  grep -q '^commit.conflict .*disposition=held' "$SPINE" || { echo "no held event: $(cat "$SPINE")"; return 1; }
}

@test "the pipeline's commit does not rebase: a stale base waits for the merge, and a conflicting one lands nothing" {
  given_a_card 9009
  "$BIN/werk-pull" 9009 kade >/dev/null
  printf 'card-version\n' > "$R/werks/kade-9009/f.txt"
  a_peer_moves_main f.txt "peer-version"
  # When the pipeline commits (--atomic, what cicd runs) and pushes
  run "$BIN/werk-commit" 9009 kade "edit f" --atomic
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(git -C "$R/werks/kade-9009" rev-list --count HEAD..origin/main)" = 0 ] || return 1
  git -C "$R/werks/kade-9009" fetch -q origin main
  [ "$(git -C "$R/werks/kade-9009" rev-list --count HEAD..origin/main)" = 1 ] || { echo "the atomic commit rebased"; return 1; }
  "$BIN/werk-push" 9009 kade >/dev/null || return 1
  # Then the merge cannot land the card over the peer's line, and main keeps the peer's version
  ACCEPTER=jeff run "$BIN/werk-merge" 9009 kade
  [ "$status" -ne 0 ] || { echo "merge said it landed: $output"; return 1; }
  main_has f.txt peer-version
  ! grep -q '^merge.landed' "$SPINE" || { echo "merge.landed on the spine"; return 1; }
}

@test "refused at pull: a card already in WIP is not pulled again, and the refusal is on the spine" {
  given_a_card 9004 WIP
  run "$BIN/werk-pull" 9004 kade
  [ "$status" -ne 0 ] || return 1
  [ ! -d "$R/werks/kade-9004" ] || { echo "a werk was made"; return 1; }
  grep -q '^pull.refused .*reason=wrong-status' "$SPINE" || { echo "spine: $(cat "$SPINE")"; return 1; }
}

@test "refused at push: a werk moved off the card's branch pushes nothing" {
  given_a_card 9005
  "$BIN/werk-pull" 9005 kade >/dev/null
  printf 'x\n' > "$R/werks/kade-9005/story.txt"
  "$BIN/werk-commit" 9005 kade "add" --atomic >/dev/null || return 1
  git -C "$R/werks/kade-9005" checkout -q -b somewhere-else
  run "$BIN/werk-push" 9005 kade
  [ "$status" -ne 0 ] || return 1
  ! git -C "$R/origin" rev-parse -q --verify refs/heads/kade/9005 >/dev/null || { echo "branch reached origin"; return 1; }
  grep -q '^push.refused .*reason=wrong-branch' "$SPINE" || { echo "spine: $(cat "$SPINE")"; return 1; }
}

@test "refused at merge: a werk moved off the card's branch lands nothing on main" {
  given_a_card 9006
  "$BIN/werk-pull" 9006 kade >/dev/null
  printf 'x\n' > "$R/werks/kade-9006/story.txt"
  "$BIN/werk-commit" 9006 kade "add" --atomic >/dev/null || return 1
  "$BIN/werk-push" 9006 kade >/dev/null || return 1
  git -C "$R/werks/kade-9006" checkout -q -b somewhere-else
  ACCEPTER=jeff run "$BIN/werk-merge" 9006 kade
  [ "$status" -ne 0 ] || return 1
  ! git -C "$R/origin" cat-file -e main:story.txt 2>/dev/null || { echo "main got the work"; return 1; }
  ! grep -q "^pr merge" "$GH_LOG" 2>/dev/null || return 1
}

@test "refused at accept: only Jeff or Wren may accept, never the card's own role; nothing closes" {
  given_a_card 9007
  "$BIN/werk-pull" 9007 kade >/dev/null
  ACCEPTER=silas run "$BIN/werk-accept" 9007 kade
  [ "$status" -ne 0 ] || { echo "silas accepted"; return 1; }
  ACCEPTER=kade run "$BIN/werk-accept" 9007 kade
  [ "$status" -ne 0 ] || { echo "kade accepted its own card"; return 1; }
  board_says 9007 WIP
  [ -d "$R/werks/kade-9007" ] || { echo "the werk was torn down"; return 1; }
  ! grep -q '^card.accepted' "$SPINE" || return 1
}

@test "accept twice: a re-run after the land converges, and nothing merges twice" {
  given_a_card 9008
  "$BIN/werk-pull" 9008 kade >/dev/null
  printf 'x\n' > "$R/werks/kade-9008/story.txt"
  "$BIN/werk-commit" 9008 kade "add" --atomic >/dev/null || return 1
  "$BIN/werk-push" 9008 kade >/dev/null || return 1
  ACCEPTER=jeff "$BIN/werk-merge" 9008 kade >/dev/null || return 1
  ACCEPTER=jeff "$BIN/werk-accept" 9008 kade >/dev/null || return 1
  ACCEPTER=jeff run "$BIN/werk-accept" 9008 kade
  [ "$status" -eq 0 ] || { echo "re-run failed: $output"; return 1; }
  board_says 9008 Done
  [ "$(grep -c '^pr merge' "$GH_LOG")" = 1 ] || { echo "gh merges: $(grep -c '^pr merge' "$GH_LOG")"; return 1; }
}

@test "negative proof: the story's spine check goes red when a seam is missing" {
  printf 'card.pulled kade\ncommit.completed kade\npush.completed kade\ncard.accepted kade\n' > "$BATS_TEST_TMPDIR/spine"
  run spine_in_order "$BATS_TEST_TMPDIR/spine" card.pulled commit.completed push.completed merge.landed card.accepted
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"never reached merge.landed"* ]] || return 1
  printf 'card.accepted kade\ncard.pulled kade\n' > "$BATS_TEST_TMPDIR/spine"
  run spine_in_order "$BATS_TEST_TMPDIR/spine" card.pulled card.accepted
  [ "$status" -eq 1 ] || { echo "out-of-order seams passed"; return 1; }
}
