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
  # Every package dir that contains a .test.ts/.test.js file somewhere within.
  # Previous version only counted packages with scripts.test in package.json —
  # that silently missed platform/api (356 tests, 0 wired), platform/pulse,
  # and anywhere else tests exist but nobody added the npm script.
  # "Every test" means every test FILE, not every test script.
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
          echo "$dir"
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

run_all() {
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
}

# --- Dispatch ---
case "${1:-}" in
  --list-npm)   list_npm   ;;
  --list-cargo) list_cargo ;;
  --list-shell) list_shell ;;
  --list-all)
    echo "# npm";   list_npm
    echo "# cargo"; list_cargo
    echo "# shell"; list_shell
    ;;
  --run-all)    run_all ;;
  *)
    echo "Usage: $0 {--list-npm|--list-cargo|--list-shell|--list-all|--run-all}" >&2
    exit 2
    ;;
esac
