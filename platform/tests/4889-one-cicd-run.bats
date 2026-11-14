#!/usr/bin/env bats
# @test-type: e2e — after land: reads main's history and dagu's run list, no fixture
# @domain: pipelines
# @card: #4889 · owner: silas
# @needs: nightly — after land: measures the newest landed card; the card's AC says `check: after test`
# AC10: a card lands on one cicd run. The count is dagu's run list for that card, the
# "cicd run N" /cws prints (Kade 19:59).
# #4965 (decision 6, Jeff 07:57): rewritten, not retired. It used to measure the one card
# that landed right after #4889 (#4915, 9 runs), so it could never pass again. It now
# reads the NEWEST card on main, so each land is measured by its own after-land check.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# the newest card that landed on main, or nothing
newest_card() {
  git -C "$ROOT" log origin/main --first-parent --format=%s 2>/dev/null \
    | awk 'match($0, /^#[0-9]+/) { print substr($0, 2, RLENGTH-1); exit }'
}

# the cicd run count on a /cws line ("… · cicd run 3 · …"), or nothing
cicd_runs() { grep -oE 'cicd run [0-9]+' <<<"$1" | head -1 | grep -oE '[0-9]+$'; }

# AC10 holds for a /cws line: it names exactly one cicd run
one_run() { [ "$(cicd_runs "$1")" = 1 ]; }

@test "AC10 (after land): the newest card to land needed one cicd run" {
  n="$(newest_card)"
  [ -n "$n" ] || { echo "no card commit on origin/main"; return 1; }
  line="$(python3 "$ROOT/platform/scripts/chorus-werk-status" "$n" 2>&1 | head -1)"
  runs="$(cicd_runs "$line")"
  [ -n "$runs" ] || { echo "no cicd run count for #$n: $line"; return 1; }
  one_run "$line" || { echo "#$n needed $runs cicd runs: $line"; return 1; }
}

@test "negative proof: a card that needed two cicd runs reads 2, not 1" {
  [ "$(cicd_runs '#4888 silas  landed  (17:51) · cicd run 5 · pair 1 run')" = 5 ]
  [ "$(cicd_runs '#4900 kade  landed  (09:00) · cicd run 2')" = 2 ]
  [ "$(cicd_runs '#4901 kade  landed  (09:00) · cicd run 1')" = 1 ]
  [ -z "$(cicd_runs '#4902 kade  waiting')" ]
  ! one_run '#4900 kade  landed  (09:00) · cicd run 2' || return 1
  ! one_run '#4902 kade  waiting' || return 1
  one_run '#4901 kade  landed  (09:00) · cicd run 1' || return 1   # control
}

@test "the newest card is the last card commit on main, past non-card commits" {
  d="$BATS_TEST_TMPDIR/g"; git init -q "$d"
  for s in "#4888 (silas)" "#4889 (silas)" "#4890 (kade)" "wren: board tidy" "#4891 (wren)" "Merge chore"; do
    git -C "$d" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "$s"
  done
  git -C "$d" update-ref refs/remotes/origin/main HEAD
  [ "$(ROOT="$d" newest_card)" = 4891 ]
}

@test "NEGATIVE PROOF: a main with no card commit reads nothing, never a stale card" {
  d="$BATS_TEST_TMPDIR/g2"; git init -q "$d"
  git -C "$d" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "chore only"
  git -C "$d" update-ref refs/remotes/origin/main HEAD
  [ -z "$(ROOT="$d" newest_card)" ]
}
