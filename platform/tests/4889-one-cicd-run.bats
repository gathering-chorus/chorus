#!/usr/bin/env bats
# @test-type: e2e — after land: reads main's history and dagu's run list, no fixture
# @domain: pipelines
# @card: #4889 · owner: silas
# @needs: nightly — after land: measures the next card; the card's AC says `check: after test`
# AC10: after #4889 lands, the next card to land needs one cicd run. The count is dagu's
# run list for that card, the "cicd run N" /cws prints (Kade 19:59).
# It skips (unmeasured, never a pass) until both hold: #4889 is on main, and another card
# has landed after it. Then it measures; borg-prove's next-land re-run reads that result.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# the first card that landed on main after #4889, or nothing
next_card() {
  git -C "$ROOT" log origin/main --first-parent --reverse --format=%s 2>/dev/null \
    | awk '/^#4889([^0-9]|$)/ { after=1; next } after && match($0, /^#[0-9]+/) { print substr($0, 2, RLENGTH-1); exit }'
}

# the cicd run count on a /cws line ("… · cicd run 3 · …"), or nothing
cicd_runs() { grep -oE 'cicd run [0-9]+' <<<"$1" | head -1 | grep -oE '[0-9]+$'; }

# AC10 holds for a /cws line: it names exactly one cicd run
one_run() { [ "$(cicd_runs "$1")" = 1 ]; }

@test "AC10 (after land): the next card to land needed one cicd run" {
  n="$(next_card)"
  [ -n "$n" ] || skip "#4889 has not landed, or no card has landed after it yet"
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

@test "the next card is the first one after #4889 on main, never #4889 itself" {
  d="$BATS_TEST_TMPDIR/g"; git init -q "$d"
  for s in "#4888 (silas)" "#4889 (silas)" "#4890 (kade)" "#4891 (wren)"; do
    git -C "$d" -c user.name=t -c user.email=t@t commit -q --allow-empty -m "$s"
  done
  git -C "$d" update-ref refs/remotes/origin/main HEAD
  [ "$(ROOT="$d" next_card)" = 4890 ]
}
