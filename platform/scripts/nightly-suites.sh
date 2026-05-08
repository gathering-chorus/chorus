#!/bin/bash
# #2142 — Discover + run every test suite the nightly backstop covers.
# No suite is silently skipped: if a directory has a Cargo.toml, a package.json
# with scripts.test, or matches platform/scripts/test-*.sh, it runs overnight.
#
# Usage:
#   nightly-suites.sh --list-npm      # one root dir per line (has package.json + scripts.test)
#   nightly-suites.sh --list-cargo    # one crate dir per line (has Cargo.toml)
#   nightly-suites.sh --list-shell    # one script path per line
#   nightly-suites.sh --list-all      # all three, labeled
#   nightly-suites.sh --run-all       # run every suite; emit per-suite status lines
#
# Owner routing (via stdout tags the caller can parse):
#   npm suite under jeff-bridwell-personal-site/        → kade (quality)
#   npm suite under directing/clearing or cards          → kade
#   npm suite under platform/                           → silas (ops)
#   cargo suite                                         → silas
#   shell suite                                         → silas

set -u

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
APP_ROOT="${APP_ROOT:-/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site}"

# --- Discovery ---

list_npm() {
  # Every package dir that has a real jest setup AND owns at least one
  # *.test.{ts,js}/*.spec.{ts,js} via the nearest-package-json walk.
  #
  # "Real jest setup" = scripts.test in package.json OR a `jest` key in
  # package.json OR a jest.config.{js,ts,cjs,mjs} file. A dir with NEITHER
  # will silently skip its discovered specs (default jest can't transform
  # TS without per-project tsconfig) and report nonsense like "82 skipped /
  # 82 total" — the chorus-root pre-#2801 case, where one orphan test
  # (roles/silas/docs/attention-architecture.test.ts) made the root the
  # nearest-package-json owner, then 260 sub-package suites failed at
  # parse time and only the misleading test-line bubbled up.
  #
  # Pre-#2142 (the original) required scripts.test — that missed
  # platform/api (356 tests, 0 wired). Pre-#2801 dropped the gate
  # entirely — that introduced the no-config-runs-anyway bug. Right
  # test: "owns specs AND has the means to run them."
  has_jest_setup() {
    local d="$1"
    local pj="$d/package.json"
    [ -f "$pj" ] || return 1
    if jq -e '.scripts.test' "$pj" >/dev/null 2>&1; then return 0; fi
    if jq -e '.jest' "$pj" >/dev/null 2>&1; then return 0; fi
    for f in jest.config.js jest.config.ts jest.config.cjs jest.config.mjs; do
      [ -f "$d/$f" ] && return 0
    done
    return 1
  }
  for root in "$CHORUS_ROOT" "$APP_ROOT"; do
    [ -d "$root" ] || continue
    find "$root" \( -name "*.test.ts" -o -name "*.test.js" -o -name "*.spec.ts" -o -name "*.spec.js" \) \
         -not -path "*/node_modules/*" \
         -not -path "*/ghost_content/*" \
         -not -path "*/dist/*" \
         -not -path "*/target/*" 2>/dev/null | while IFS= read -r tf; do
      dir=$(dirname "$tf")
      while [ "$dir" != "/" ]; do
        if [ -f "$dir/package.json" ]; then
          if has_jest_setup "$dir"; then
            echo "$dir"
          fi
          break
        fi
        dir=$(dirname "$dir")
      done
    done | sort -u
  done
}

list_cargo() {
  find "$CHORUS_ROOT/platform/services" -name Cargo.toml -not -path "*/target/*" 2>/dev/null | while read -r ct; do
    dirname "$ct"
  done
}

list_shell() {
  find "$CHORUS_ROOT/platform/scripts" -maxdepth 1 -name "test-*.sh" -type f 2>/dev/null | sort
}

# #2806: bats discovery — find every *.bats file under chorus that isn't in
# node_modules / target / dist. Pre-#2806 the runner had no bats tier; ~95
# of 97 bats files sat dormant despite being real test surfaces. The two
# that did run got there because individual test-*.sh scripts happened to
# invoke them; the rest were silently dark. List pattern mirrors list_npm's
# scope-and-exclude shape.
list_bats() {
  find "$CHORUS_ROOT" -name "*.bats" \
       -not -path "*/node_modules/*" \
       -not -path "*/target/*" \
       -not -path "*/dist/*" 2>/dev/null | sort
}

# #2806: cucumber discovery — find package dirs with cucumber-js as the
# test runner. Pre-#2806 the runner walked .test.ts only; cucumber's
# .feature files never matched, so platform/tests (with 23+ feature files
# and scripts.test=cucumber-js) was silently dropped from the nightly.
# Heuristic: any package.json whose scripts.test mentions cucumber-js AND
# has a features/ subdirectory.
list_cucumber() {
  for root in "$CHORUS_ROOT"; do
    [ -d "$root" ] || continue
    find "$root" -name "package.json" \
         -not -path "*/node_modules/*" \
         -not -path "*/dist/*" 2>/dev/null | while IFS= read -r pj; do
      local d
      d=$(dirname "$pj")
      if jq -re '.scripts.test // ""' "$pj" 2>/dev/null | grep -q "cucumber-js"; then
        if [ -d "$d/features" ] || find "$d" -maxdepth 4 -name "*.feature" -not -path "*/node_modules/*" 2>/dev/null | grep -q .; then
          echo "$d"
        fi
      fi
    done | sort -u
  done
}

# --- Execution ---

# Run a single suite with one retry on failure — absorbs concurrent-run flakes
# where a standalone run passes but parallel pressure causes a timeout/race.
# If the suite fails twice, report fail.
run_one() {
  local kind="$1" path="$2" owner="$3"
  local out1 out2 line1 line2
  line1=$(run_one_attempt "$kind" "$path" "$owner")
  case "$line1" in *"|pass|"*) echo "$line1"; return ;; esac
  line2=$(run_one_attempt "$kind" "$path" "$owner")
  # Use the second attempt's result (pass absorbs the flake, fail confirms).
  echo "$line2"
}

# Single attempt — the original run_one body.
run_one_attempt() {
  local kind="$1" path="$2" owner="$3"
  local status="pass" summary=""
  case "$kind" in
    npm)
      # #2806 attempted --runInBand for determinism; reverted because
      # serial mode triggers a hang in platform/api's open-handle tier
      # (some test holds a server/socket and serialization changes
      # close-order such that jest never exits — observed 50+ min hang
      # on a single package run, vs ~30s parallel). Parallel default
      # has a rare flake on server-unit's POST /api/chorus/embed
      # (passes alone). Trade-off: rare flake > deterministic hang.
      # The hang class needs root-cause investigation in platform/api's
      # test setup before --runInBand is safe to enable.
      local out rc
      out=$(cd "$path" && npx jest --passWithNoTests --silent 2>&1); rc=$?
      out=$(echo "$out" | tail -3)
      summary=$(echo "$out" | grep -E "Tests:" | head -1 | tr -d '\n')
      [ "$rc" -ne 0 ] && status="fail"
      ;;
    cargo)
      # --test-threads=1: several chorus-hooks tests share global state
      # (role-state files, hook env vars) and flake under parallel runs.
      # Serial execution matches the nightly's isolation goal; under the
      # budget-set workload it adds a few seconds total.
      local out
      out=$(cd "$path" && cargo test --release -- --test-threads=1 2>&1 || true)
      local passed failed
      passed=$(echo "$out" | grep -cE '^test result: ok\.' || true)
      failed=$(echo "$out" | grep -cE '^test result: FAILED\.' || true)
      summary="suites: $passed ok, $failed failed"
      [ "$failed" -gt 0 ] && status="fail"
      ;;
    shell)
      local out rc
      out=$(bash "$path" 2>&1); rc=$?
      summary=$(echo "$out" | tail -1)
      [ "$rc" -ne 0 ] && status="fail"
      ;;
    bats)
      # bats reports `ok N <desc>` per test, `not ok N <desc>` per fail,
      # and a 1..N plan line. Last line of TAP output is the final test
      # result. Summary extracts pass/fail counts via grep.
      local out rc passed failed
      out=$(bats "$path" 2>&1); rc=$?
      passed=$(echo "$out" | grep -cE '^ok ' || true)
      failed=$(echo "$out" | grep -cE '^not ok ' || true)
      summary="bats: $passed passed, $failed failed"
      [ "$rc" -ne 0 ] && status="fail"
      ;;
    cucumber)
      # cucumber-js's `npm test` exits non-zero on any failed scenario.
      # Summary line is typically `N scenarios (M passed, K failed)` near
      # the end of stdout.
      local out rc
      out=$(cd "$path" && npm test --silent 2>&1); rc=$?
      summary=$(echo "$out" | grep -E "scenarios? \(" | tail -1 | tr -d '\n')
      [ -z "$summary" ] && summary=$(echo "$out" | tail -1)
      [ "$rc" -ne 0 ] && status="fail"
      ;;
  esac
  echo "SUITE|$kind|$path|$owner|$status|$summary"
}

owner_for_npm() {
  case "$1" in
    $APP_ROOT|$APP_ROOT/*)                              echo "kade" ;;
    $CHORUS_ROOT/directing/*)                           echo "kade" ;;
    $CHORUS_ROOT/platform/*|$CHORUS_ROOT/roles/*)       echo "silas" ;;
    *)                                                  echo "kade" ;;
  esac
}

run_lint_ratchet() {
  # #2465: full-codebase ESLint ratchet. Runs every nightly so drift surfaces
  # even when no role is touching TypeScript. Fails if any rule count climbed
  # above baseline OR a new rule fires not in baseline.
  local path="$CHORUS_ROOT" owner="kade" status="pass" summary="" out rc
  if [ -f "$path/.eslint-baseline.json" ] && [ -f "$path/eslint.config.js" ]; then
    out=$(cd "$path" && npm run lint:ratchet --silent 2>&1); rc=$?
    summary=$(echo "$out" | tail -1 | tr -d '\n')
    [ "$rc" -ne 0 ] && status="fail"
    echo "SUITE|lint|$path|$owner|$status|$summary"
  fi
}

run_all() {
  run_lint_ratchet

  while IFS= read -r d; do
    [ -z "$d" ] && continue
    run_one npm "$d" "$(owner_for_npm "$d")"
  done < <(list_npm)

  while IFS= read -r d; do
    [ -z "$d" ] && continue
    run_one cargo "$d" "silas"
  done < <(list_cargo)

  while IFS= read -r s; do
    [ -z "$s" ] && continue
    run_one shell "$s" "silas"
  done < <(list_shell)

  # #2806: bats + cucumber tiers were silently dormant pre-#2806. ~95 of
  # 97 bats files and all 23 cucumber features sat dark while only
  # whichever bats happened to be invoked from a test-*.sh wrapper got
  # exercised. These two loops light them up.
  while IFS= read -r b; do
    [ -z "$b" ] && continue
    run_one bats "$b" "silas"
  done < <(list_bats)

  while IFS= read -r d; do
    [ -z "$d" ] && continue
    run_one cucumber "$d" "silas"
  done < <(list_cucumber)
}

# --- Dispatch ---
case "${1:-}" in
  --list-npm)      list_npm      ;;
  --list-cargo)    list_cargo    ;;
  --list-shell)    list_shell    ;;
  --list-bats)     list_bats     ;;
  --list-cucumber) list_cucumber ;;
  --list-all)
    echo "# npm";       list_npm
    echo "# cargo";     list_cargo
    echo "# shell";     list_shell
    echo "# bats";      list_bats
    echo "# cucumber";  list_cucumber
    ;;
  --run-all)    run_all ;;
  *)
    echo "Usage: $0 {--list-npm|--list-cargo|--list-shell|--list-bats|--list-cucumber|--list-all|--run-all}" >&2
    exit 2
    ;;
esac
