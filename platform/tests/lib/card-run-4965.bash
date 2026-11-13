# @domain: tests
# #4965 — the reader the card's AC1/AC3/AC4 checks share. They read the newest
# FINISHED all-scope run started for #4965 and the 2026-11-13 03:00 nightly, both
# from the run folders werk-test writes (one `<ms>\tSUITE|kind|path|owner|verdict|summary`
# line per suite in suites-queue and suites-perf). A run that tested another tree
# or another commit is refused, never read (#4965 AC7: the scope step records
# `root` and `head`).

RUNS_4965="${WERK_TEST_RUNS_4965:-${WERK_TEST_RUNS:-${CHORUS_STATE_DIR:-$HOME/.chorus}/werk-test-runs}}"
NIGHTLY_4965="${NIGHTLY_RUN_4965:-$RUNS_4965/034wlW158xFUngDETJP9Bi}"

# AC1 as narrowed by Jeff 13:01 (Wren 13:07): proof run 034wwPUtirLpzeFRS3SO29 (trace
# kade4965all1794583436, head ac6432f90) is the run AC1 reads. The werk may move past
# its head only by the three code fixes, the three waits put back, and these checks.
PROOF_4965="${PROOF_RUN_4965:-$RUNS_4965/kade4965all1794583436}"
AFTER_PROOF_FILES_4965='platform/tests/lib/quartet-walk.sh
platform/tests/4145-runner-owns-the-run.bats
directing/clearing-v2/tests/headless-connect-4886.test.ts
directing/clearing-v2/tests/photos-4635.test.ts
directing/clearing/tests/jeff-input-delivery.test.ts
platform/tests/lib/card-run-4965.bash
platform/tests/4965-nightly-reds-green.bats
platform/tests/4965-nothing-newly-red.bats
platform/tests/4965-unowned-named.bats'

# the 18 nightly reds the proof run showed green (card AC1)
PROVEN_4965='bats|platform/tests/4416-no-raised-test-timeouts.bats
bats|platform/tests/4773-deploy-posts-manifest.bats
bats|platform/tests/4888-run-finish-wakes.bats
bats|platform/tests/domain-flow-time.bats
bats|platform/tests/shim-resilience.bats
bdd|platform/tests/features/clearing/clearing-access.feature
coverage|platform/api
coverage|platform/pulse
npm|directing/clearing-v2
npm|platform/pulse
security|directing/clearing
security|platform/api
shell|platform/tests/4417-clearing-test-headers.test.sh
shell|platform/tests/4490-v1-freeze.test.sh
shell|platform/tests/4535-shared-state-ratchet.test.sh
shell|platform/tests/4807-inject-unanswered-prod.test.sh
shell|platform/tests/4807-pulse-concurrent-prod.test.sh
shell|platform/tests/verb-binary-drift.test.sh'

# proof_run <werk> <run dir> — the run dir if it tested <werk> at a commit HEAD descends
# from, and every file changed since is on the after-proof list; else exit 1 naming why
proof_run() {
  local werk="$1" d="$2" h f
  [ -s "$d/suites-perf" ] || { echo "no finished run at $d"; return 1; }
  [ "$(cat "$d/root" 2>/dev/null)" = "$werk" ] || { echo "$d tested $(cat "$d/root" 2>/dev/null || echo 'no recorded tree'), not $werk"; return 1; }
  h="$(cat "$d/head" 2>/dev/null)"
  git -C "$werk" merge-base --is-ancestor "$h" HEAD 2>/dev/null || { echo "$d tested ${h:-no recorded head}, not an ancestor of the werk's HEAD"; return 1; }
  for f in $(git -C "$werk" diff --name-only "$h" HEAD); do
    printf '%s\n' "$AFTER_PROOF_FILES_4965" | grep -qxF "$f" || { echo "changed since the proof run, not on the list: $f"; return 1; }
  done
  echo "$d"
}

# proven_not_pass <run dir> — each of the 18 that is not a pass there; empty = AC1 met
proven_not_pass() {
  local key v
  printf '%s\n' "$PROVEN_4965" | while read -r key; do
    v="$(suites "$1" | awk -F'|' -v k="$key" '$1"|"$2==k{print $4}' | sort -u | paste -sd, -)"
    [ "$v" = "pass" ] || echo "$key ${v:-absent}"
  done
}

# suites <run dir> — "kind|path|owner|verdict" per suite, perf rows included
suites() { cat "$1/suites-queue" "$1/suites-perf" 2>/dev/null | cut -f2 | awk -F'|' '$1=="SUITE"{print $2"|"$3"|"$4"|"$5}'; }

# card_run <werk> — the newest finished kade4965all* run of <werk> at its HEAD, or
# exit 1 naming why. Finished = its perf step wrote suites-perf.
card_run() {
  local werk="$1" head d
  head="$(git -C "$werk" rev-parse HEAD 2>/dev/null)" || { echo "no git HEAD at $werk"; return 1; }
  for d in $(ls -td "$RUNS_4965"/kade4965all* 2>/dev/null); do
    [ -s "$d/suites-perf" ] || continue
    [ "$(cat "$d/root" 2>/dev/null)" = "$werk" ] || continue
    [ "$(cat "$d/head" 2>/dev/null)" = "$head" ] || { echo "newest finished run $d tested $(cat "$d/head" 2>/dev/null || echo 'an unrecorded head'), the werk is at $head"; return 1; }
    echo "$d"; return 0
  done
  echo "no finished all-scope run of $werk under $RUNS_4965"; return 1
}

# the seven after-land checks AC1 leaves out: decisions 6 and 8 (Jeff 07:57), and
# 4022/4279, which write production and so skip on a card's werk (Jeff 10:06)
AFTER_LAND_4965='bats|platform/tests/4022-role-write-scope-live.bats
bats|platform/tests/4279-api-quartet-prod.bats
bats|platform/tests/4915-nightly-zero-red.bats
bats|platform/tests/4889-after-land-depcruise.bats
bats|platform/tests/4889-one-cicd-run.bats
bats|platform/tests/demo-reviews-parallel-live.bats
integration|platform/api'

# reds_still_red <nightly dir> <card run dir> — each suite red in the nightly (fail or
# unmeasured; slow is speed, not breakage) that is not a pass in the card run, except
# the after-land five. Prints "kind|path verdict-in-card-run"; empty = AC1 met.
reds_still_red() {
  local key v
  suites "$1" | awk -F'|' '$4=="fail"||$4=="unmeasured"{print $1"|"$2}' | sort -u | while read -r key; do
    printf '%s\n' "$AFTER_LAND_4965" | grep -qxF "$key" && continue
    v="$(suites "$2" | awk -F'|' -v k="$key" '$1"|"$2==k{print $4}' | sort -u | paste -sd, -)"
    [ "$v" = "pass" ] || echo "$key ${v:-absent}"
  done
}

# newly_red <nightly dir> <card run dir> — suites that passed in the nightly and are
# fail or unmeasured in the card run. Empty = AC3 met.
newly_red() {
  local key v
  suites "$1" | awk -F'|' '$4=="pass"{print $1"|"$2}' | sort -u | while read -r key; do
    v="$(suites "$2" | awk -F'|' -v k="$key" '$1"|"$2==k{print $4}' | sort -u | paste -sd, -)"
    case ",$v," in *,fail,*|*,unmeasured,*) echo "$key $v";; esac
  done
}

# red_unowned <nightly dir> — the suites the nightly reported red AND unowned (AC4's five)
red_unowned() { suites "$1" | awk -F'|' '$3=="unowned" && ($4=="fail"||$4=="unmeasured"){print $1"|"$2}' | sort -u; }

# still_unowned <nightly dir> <card run dir> — of those, the ones whose owner in the
# card run is still unowned or missing. Empty = AC4 met.
still_unowned() {
  local key o
  red_unowned "$1" | while read -r key; do
    o="$(suites "$2" | awk -F'|' -v k="$key" '$1"|"$2==k{print $3}' | sort -u | paste -sd, -)"
    case ",$o," in ,,|*,unowned,*) echo "$key ${o:-absent}";; esac
  done
}
