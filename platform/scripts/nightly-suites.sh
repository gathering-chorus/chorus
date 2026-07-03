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

# #3484: pin node 20 for the whole nightly. The launchd plist's PATH has
# /opt/homebrew/bin (node 23, NODE_MODULE_VERSION 131) but NOT nvm, so `npx jest`
# ran under node 23 while better-sqlite3 (the search-engine native) is built for
# node 20 (115) → every FTS/search suite threw an ABI error → a wall of false
# red in the morning nightly. Force the matching node so the run is honest. Same
# fix as werk.yml's test step; resolves the newest installed 20.x.
__N20="$(ls -d "$HOME"/.nvm/versions/node/v20*/bin 2>/dev/null | sort -V | tail -1)"
if [ -n "$__N20" ] && [ -x "$__N20/node" ]; then export PATH="$__N20:$PATH"; fi

# Spine emit (#3484, mirrors the agent-state/#2605 helper). Best-effort — a
# logging failure must never change the run's outcome. CHORUS_LOG_BIN is
# env-overridable so unit tests stub chorus-log without symlinking it.
NIGHTLY_ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"
SCRIPT_DIR_NIGHTLY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHORUS_LOG_BIN="${CHORUS_LOG_BIN:-$(command -v chorus-log || echo "$SCRIPT_DIR_NIGHTLY/chorus-log")}"

spine_emit() {
  local event="$1"; shift
  if [ -x "$CHORUS_LOG_BIN" ]; then
    "$CHORUS_LOG_BIN" "$event" "$NIGHTLY_ROLE" "$@" >/dev/null 2>&1 || true
  fi
}

# #3484 — failure-detail capture. The runner used to keep rc but discard the
# failure OUTPUT, so a red ("compile/run failure rc=N") couldn't explain itself
# and every morning was a fresh re-diagnosis with the evidence gone. We now
# persist the failing suite's output tail here; emit_suite_results surfaces a
# one-line reason from it into the spine event. Env-overridable for tests.
NIGHTLY_FAIL_DIR="${NIGHTLY_FAIL_DIR:-$HOME/.chorus/nightly-failures}"

# Stable per-suite failure-log path (kind+path → one file). Both the writer
# (run_one_attempt) and the reader (emit_suite_results) derive it identically.
_fail_log_path() {
  local kind="$1" path="$2" id
  id=$(printf '%s' "${kind}-${path}" | tr '/ .' '___')
  printf '%s/%s.log' "$NIGHTLY_FAIL_DIR" "$id"
}

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

# #3557 — STACK-GATE. Live-stack suites (real HTTP / deploy / launchctl / health
# probe) can only pass against running services. The nightly runs headless at
# 03:49 where the stack is often down/degraded, so they fail for lack of an
# environment, not for broken code — the "18 env failures" false-red. The gate:
# if a suite NEEDS the stack and the stack is DOWN, report skip (NOT fail). A red
# nightly then means a real regression. "There is no test environment, only
# production" — so probe production once before judging these suites.
_STACK_PROBE=""  # "up" | "down" — probed once per run, cached
_stack_up() {
  if [ -z "$_STACK_PROBE" ]; then
    if curl -fsS -m 4 "http://localhost:3340/api/chorus/context/health" >/dev/null 2>&1 \
       && curl -fsS -m 4 "http://localhost:3030/" >/dev/null 2>&1; then
      _STACK_PROBE="up"
    else
      _STACK_PROBE="down"
    fi
  fi
  [ "$_STACK_PROBE" = "up" ]
}

# Suites that require the live stack (Silas's classification — data/ops owner).
# Deliberately CONSERVATIVE: only suites that are WHOLLY live-stack, so the gate
# can never hide a hermetic regression. The api npm suite (hermetic+integration
# mixed) is intentionally NOT here — it is gated differently: #3559 split it into
# two jest projects, and the npm branch of run_one_attempt sets RUN_INTEGRATION
# per-package (below) so api's HERMETIC project always runs while its INTEGRATION
# project is stack-gated. A whole-suite skip here would wrongly hide the hermetic
# half; the per-project gate is the right granularity for a mixed suite.
_needs_stack() {
  case "$1" in
    */test-api-health.sh|*/test-agent-state.sh)            return 0 ;;
    *deep-health*.bats)                                    return 0 ;;
    */alert-delivery.bats|*/alert-suppress.bats)           return 0 ;;
    *chorus-deploy*.bats|*deploy-daemon*.bats|*deploy-live*.bats|*deploy-verify*.bats|*deploy-running*.bats|*deploy-rollback*.bats) return 0 ;;
    *) return 1 ;;
  esac
}

# #3559 — env prefix for an npm package's jest run. platform/api was split into
# hermetic + integration jest projects; the integration project is only
# constructed when RUN_INTEGRATION=true (jest.config.js). Set it solely for
# platform/api and solely when the live stack is up (reusing #3557's _stack_up
# probe), so a stack-down nightly runs api hermetic-only and can never false-red.
# Every other npm package → empty (hermetic-only). Pure + stack-probe-driven, so
# test-nightly-stack-gate.sh can force _STACK_PROBE and assert this hermetically.
_npm_jest_env() {
  case "$1" in
    */platform/api) _stack_up && { echo "RUN_INTEGRATION=true"; return; } ;;
  esac
  echo ""
}

# #3606 — does this package actually test with jest? Discovery (has_jest_setup)
# admits any package with a scripts.test, but a test script like `tsx --test`
# (mcp-server) is node's runner, not jest. Only run `npx jest` where jest is
# genuinely configured: scripts.test mentions jest, a `jest` key exists, or a
# jest.config.* file is present.
_npm_package_uses_jest() {
  local d="$1" pj="$1/package.json"
  [ -f "$pj" ] || return 1
  jq -er '.scripts.test // ""' "$pj" 2>/dev/null | grep -q "jest" && return 0
  jq -e '.jest' "$pj" >/dev/null 2>&1 && return 0
  local f
  for f in jest.config.js jest.config.ts jest.config.cjs jest.config.mjs; do
    [ -f "$d/$f" ] && return 0
  done
  return 1
}

# Run a single suite — ONE attempt, deterministic result.
# #3597: the retry-to-absorb-concurrent-flakes band-aid is GONE. It papered over
# non-determinism (a standalone pass but a parallel-pressure race) instead of
# preventing it; the single-flight lock (--run-all dispatch) prevents the overlap
# at the source, so a retry can now only HIDE a real intermittent failure. One
# attempt: a suite passes or fails, and we believe the result.
run_one() {
  local kind="$1" path="$2" owner="$3"
  # #3557 stack-gate: a live-stack suite with no stack is a SKIP, not a fail.
  if _needs_stack "$path" && ! _stack_up; then
    echo "SUITE|$kind|$path|$owner|skip|skipped — no live stack (#3557)"
    return
  fi
  run_one_attempt "$kind" "$path" "$owner"
}

# Extract a parseable pass/fail summary from a shell test script's full stdout.
#
# Downstream consumer (daily-review-quality.sh) requires the summary to match
# `[0-9]+ (pass|ok)` AND `[0-9]+ fail` to count the suite as run. Three forms
# are tried in priority order; the last is a synthesis from rc so a script
# that ran but didn't print a recognizable line is never silently bucketed
# as DID NOT RUN.
#
#   1. canonical:  === Results: N passed, M failed ===     (most test-*.sh)
#   2. fallback:   Passed: N + Failed: M on adjacent lines (bin-install style)
#   3. last-line:  the script's tail -1, IF it already matches the consumer
#                  regex (back-compat for any legacy shape).
#   4. synthesize: 1 ok / 0 fail on rc=0, 0 pass / 1 fail otherwise.
_extract_shell_summary() {
  local out="$1" rc="$2"
  local p f line

  # 1. canonical
  line=$(echo "$out" | grep -oE '=== Results: [0-9]+ passed, [0-9]+ failed ===' | tail -1)
  if [ -n "$line" ]; then
    p=$(echo "$line" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+')
    f=$(echo "$line" | grep -oE '[0-9]+ failed' | grep -oE '[0-9]+')
    echo "$p pass, $f fail"
    return
  fi

  # 2. Passed: N / Failed: M (need both, anywhere in output)
  p=$(echo "$out" | grep -oE '^Passed: [0-9]+' | tail -1 | grep -oE '[0-9]+' || true)
  f=$(echo "$out" | grep -oE '^Failed: [0-9]+' | tail -1 | grep -oE '[0-9]+' || true)
  if [ -n "$p" ] && [ -n "$f" ]; then
    echo "$p pass, $f fail"
    return
  fi

  # 3. last-line, only if it already matches the consumer's expected shape
  line=$(echo "$out" | tail -1)
  if echo "$line" | grep -qE '[0-9]+ (pass|ok)' && echo "$line" | grep -qE '[0-9]+ fail'; then
    echo "$line"
    return
  fi

  # 4. synthesize from rc
  if [ "$rc" -eq 0 ]; then
    echo "1 ok, 0 fail (synthesized, no parseable line)"
  else
    echo "0 pass, 1 fail (synthesized rc=$rc, no parseable line)"
  fi
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
      # #3559 — platform/api jest is split into hermetic + integration projects.
      # _npm_jest_env decides the env prefix: api's integration project is only
      # CONSTRUCTED when RUN_INTEGRATION=true, gated on the live stack. Stack down
      # → never built → api hermetic-only, can't false-red. (See _npm_jest_env.)
      local out rc jest_env
      if _npm_package_uses_jest "$path"; then
        jest_env=$(_npm_jest_env "$path")
        out=$(cd "$path" && env $jest_env npx jest --passWithNoTests --silent 2>&1); rc=$?
        # #3598 — keep the FULL jest output (was `tail -3`, which destroyed every
        # failure detail at the source so the saved fail-log held nothing usable).
        summary=$(echo "$out" | grep -E "Tests:" | head -1 | tr -d '\n')
      else
        # #3606 — a package whose scripts.test is NOT jest (mcp-server: tsx
        # --test, node's runner) runs its OWN runner. Hardcoded `npx jest`
        # here downloaded jest into the shared npx cache at 3am, died on
        # cache corruption (ENOTEMPTY), and reported a blank-summary fail
        # for weeks while the package's real suite passed.
        out=$(cd "$path" && npm test --silent 2>&1); rc=$?
        # node test runner summary: "# pass N" / "# fail N"; fall back to
        # the last non-empty line so the summary is never blank again.
        local np nf
        np=$(echo "$out" | grep -E '^# pass ' | tail -1 | awk '{print $3}')
        nf=$(echo "$out" | grep -E '^# fail ' | tail -1 | awk '{print $3}')
        if [ -n "$np" ] || [ -n "$nf" ]; then
          summary="pass ${np:-0}, fail ${nf:-0}"
        else
          summary=$(echo "$out" | grep -v '^\s*$' | tail -1 | tr -d '\n' | cut -c1-160)
        fi
      fi
      [ "$rc" -ne 0 ] && status="fail"
      ;;
    cargo)
      # --test-threads=1: several chorus-hooks tests share global state
      # (role-state files, hook env vars) and flake under parallel runs.
      # Serial execution matches the nightly's isolation goal; under the
      # budget-set workload it adds a few seconds total.
      #
      # #3484: isolated CARGO_TARGET_DIR. Each crate builds in its own target/
      # (no workspace), so a role/recovery `cargo` touching the SAME crate's
      # target/ while the nightly runs fights over the build lock — cargo
      # returns nonzero, the synthesis below stamps "0 ok, 1 failed", and
      # because the contention hits every crate it paints the whole run red at
      # once (the false MASS-red: 2026-06-20 saw werk-push/owl-api/chorus-model
      # all "red" while each was green on a standalone run). A dedicated target
      # dir gives the nightly its own build lock — it can never contend with a
      # role build, so the only input is the code. Warm after the first night.
      local out rc
      local nt="${NIGHTLY_CARGO_TARGET:-$HOME/.chorus/nightly-cargo-target}"
      out=$(cd "$path" && CARGO_TARGET_DIR="$nt" cargo test --release -- --test-threads=1 2>&1); rc=$?
      local passed failed
      passed=$(echo "$out" | grep -cE '^test result: ok\.' || true)
      failed=$(echo "$out" | grep -cE '^test result: FAILED\.' || true)
      if [ "$passed" -eq 0 ] && [ "$failed" -eq 0 ] && [ "$rc" -ne 0 ]; then
        # cargo did not print test-result lines (compile error, panic, run
        # interrupted). Surface as a real failure rather than letting the
        # downstream parser treat a 0/0 summary as "no parseable output"
        # and silently bucket it as DID NOT RUN.
        summary="suites: 0 ok, 1 failed (compile/run failure rc=$rc)"
        status="fail"
      else
        summary="suites: $passed ok, $failed failed"
        [ "$failed" -gt 0 ] && status="fail"
      fi
      ;;
    shell)
      local out rc
      out=$(bash "$path" 2>&1); rc=$?
      summary=$(_extract_shell_summary "$out" "$rc")
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
  # #3484 — persist the failure output so the red can explain itself; clear it
  # on green so a passing rerun doesn't leave a stale reason. `out` is unset for
  # the lint path (separate fn), so guard on it.
  local _flog; _flog=$(_fail_log_path "$kind" "$path")
  if [ "$status" = "fail" ]; then
    mkdir -p "$NIGHTLY_FAIL_DIR" 2>/dev/null || true
    # #3598 — log ALL errors, not a tail. The old `tail -25` kept only the LAST
    # ~1 failure of a multi-failure suite (cucumber emits 756 lines / 29 failures);
    # you could never diagnose the suite from its saved log, forcing a re-run.
    # Keep the full output so the log IS the source of truth.
    printf '%s\n' "${out:-}" > "$_flog" 2>/dev/null || true
  else
    rm -f "$_flog" 2>/dev/null || true
  fi
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
    # #3484: emit a CONSUMER-PARSEABLE summary. daily-review-quality.sh requires
    # `[0-9]+ (pass|ok)` AND `[0-9]+ fail` to count a suite as RUN (lines 88-90);
    # the raw ratchet tail-line matches neither, so lint:chorus was silently
    # bucketed "DID NOT RUN (no parseable test output)" → a false-red every nightly.
    # The ratchet is binary (clean=rc0 / drifted=rc!=0), so synthesize the count
    # from rc — mirroring the cargo/shell synthesis — and keep the real tail as
    # trailing context.
    local detail; detail=$(echo "$out" | tail -1 | tr -d '\n')
    if [ "$rc" -eq 0 ]; then
      summary="1 pass, 0 fail (lint:ratchet clean — ${detail})"
    else
      status="fail"
      summary="0 pass, 1 fail (lint:ratchet drifted rc=${rc} — ${detail})"
    fi
    echo "SUITE|lint|$path|$owner|$status|$summary"
  fi
}

# #3527 — coverage tier, FOLDED from nightly-coverage.sh (#2207) so the single runner emits
# suites + lint + COVERAGE in one report + one grouped nudge (retiring the separate 2 AM
# com.chorus.nightly-coverage). Reads coverage-floors.yml (Jeff-authored — the authoritative
# bar, NOT jest.config/tarpaulin which roles can tune), runs jest --coverage / cargo llvm-cov
# per declared project, emits ONE parseable SUITE|coverage line each. Dry-run
# (NIGHTLY_COVERAGE_DRY_RUN=1 + NIGHTLY_COVERAGE_FIXTURES=<dir>) reads pre-baked summary json
# so the fold is unit-testable without real coverage. Unmeasured (no summary) = SKIP — not a
# silent pass, not a false red.
_cov_owner() { case "$1" in directing/*) echo kade ;; platform/*|roles/*) echo silas ;; *) echo kade ;; esac; }
run_coverage() {
  local floors="${NIGHTLY_COVERAGE_FLOORS:-$CHORUS_ROOT/coverage-floors.yml}"
  [ -f "$floors" ] || return 0
  local dry="${NIGHTLY_COVERAGE_DRY_RUN:-}" fix="${NIGHTLY_COVERAGE_FIXTURES:-}"
  local lang rel floor owner dir sj pct rc
  while IFS=' ' read -r lang rel floor; do
    [ -z "$lang" ] && continue
    owner=$(_cov_owner "$rel"); dir="$CHORUS_ROOT/$rel"; pct=""; sj=""; rc=0
    if [ "$lang" = "ts" ]; then
      if [ -n "$dry" ] && [ -n "$fix" ]; then sj="$fix/$rel/coverage/coverage-summary.json"
      elif [ -d "$dir" ]; then
        (cd "$dir" && npx jest --coverage --coverageReporters=json-summary --passWithNoTests --silent >/dev/null 2>&1); rc=$?
        sj="$dir/coverage/coverage-summary.json"
      else rc=127; fi
      [ -f "$sj" ] && pct=$(python3 -c "import json;print(json.load(open('$sj'))['total']['statements']['pct'])" 2>/dev/null || true)
    elif [ "$lang" = "rust" ]; then
      if [ -n "$dry" ] && [ -n "$fix" ]; then sj="$fix/$rel/llvm-cov-summary.json"
      elif [ -d "$dir" ]; then
        (cd "$dir" && cargo llvm-cov --summary-only --json >"$dir/llvm-cov-summary.json" 2>/dev/null); rc=$?
        sj="$dir/llvm-cov-summary.json"
      else rc=127; fi
      [ -f "$sj" ] && pct=$(python3 -c "import json;print(json.load(open('$sj'))['data'][0]['totals']['lines']['percent'])" 2>/dev/null || true)
    else continue; fi
    # #3597 — deterministic grading: exit-code FIRST, artifact second, NEVER a third
    # "unmeasured/skip" state. A declared floor means coverage MUST run; if the run
    # errored (rc≠0) or ran but produced NO summary artifact, that is a FAIL we can
    # SEE — not a silent skip that "folds clean" and hides a broken coverage setup.
    # Reverses #3557's skip-on-unmeasured: skip WAS the "we don't know" third state
    # Jeff's #3597 exists to eliminate ("either pass or fail, either way we know").
    if [ "${rc:-1}" -ne 0 ]; then
      echo "SUITE|coverage|$rel|$owner|fail|0 pass, 1 fail (coverage run errored rc=$rc — floor ${floor}%, no clean measurement)"
    elif [ -z "$pct" ]; then
      echo "SUITE|coverage|$rel|$owner|fail|0 pass, 1 fail (coverage ran but produced NO summary artifact — expected floor ${floor}%, got nothing)"
    elif python3 -c "import sys;sys.exit(0 if float('$pct')>=float('$floor') else 1)" 2>/dev/null; then
      echo "SUITE|coverage|$rel|$owner|pass|1 pass, 0 fail (coverage ${pct}% >= floor ${floor}%)"
    else
      echo "SUITE|coverage|$rel|$owner|fail|0 pass, 1 fail (coverage ${pct}% < floor ${floor}%)"
    fi
  done < <(python3 - "$floors" <<'PYEOF'
import re, sys
text = open(sys.argv[1]).read(); section=None
for line in text.splitlines():
    ms=re.match(r'^(ts|rust):\s*$', line); me=re.match(r'^\s{2}(\S[^:]+):\s+(\d+)', line)
    if ms: section=ms.group(1)
    elif me and section: print(f"{section} {me.group(1).strip()} {me.group(2)}")
PYEOF
)
}

# #3527 — smoke tier, FOLDED from daily-review-quality.sh. Broad app-health (smoke-check.sh
# --all) was orphaned in the 6 AM runner; now one SUITE line. STACK-GATED (#3557): smoke is
# live-health, so a stack-down nightly SKIPS it (never false-reds). Owner kade (app health).
run_smoke() {
  local sc="$CHORUS_ROOT/platform/scripts/smoke-check.sh"
  [ -x "$sc" ] || return 0
  if ! _stack_up; then echo "SUITE|smoke|$sc|kade|skip|skipped — no live stack (#3557)"; return; fi
  local out rc; out=$(bash "$sc" --all 2>&1); rc=$?
  if [ "$rc" -eq 0 ]; then echo "SUITE|smoke|$sc|kade|pass|1 pass, 0 fail (smoke --all clean)"
  else
    mkdir -p "$NIGHTLY_FAIL_DIR" 2>/dev/null || true; printf '%s\n' "$out" > "$(_fail_log_path smoke "$sc")" 2>/dev/null || true  # #3598 — full output, not tail
    echo "SUITE|smoke|$sc|kade|fail|0 pass, 1 fail (smoke --all rc=$rc)"
  fi
}

# #3527 — gathering-app frontend eslint, FOLDED from daily-review-quality.sh. This is the
# gathering frontend's ONLY lint gate (npx eslint src/ on APP_ROOT) — DISTINCT from
# run_lint_ratchet (chorus-root). Was orphaned in daily-review-quality; now one SUITE line.
# Hermetic (no stack). Preserves the existing --max-warnings 999 bar (don't move the gate
# during a consolidation). Owner kade.
run_app_eslint() {
  [ -d "$APP_ROOT/src" ] || return 0
  local out rc detail; out=$(cd "$APP_ROOT" && npx eslint src/ --max-warnings 999 2>&1); rc=$?
  detail=$(echo "$out" | tail -1 | tr -d '\n')
  if [ "$rc" -eq 0 ]; then echo "SUITE|app-eslint|$APP_ROOT|kade|pass|1 pass, 0 fail (app eslint clean — ${detail})"
  else echo "SUITE|app-eslint|$APP_ROOT|kade|fail|0 pass, 1 fail (app eslint rc=$rc — ${detail})"; fi
}

run_all() {
  run_lint_ratchet
  # #3527 — folded tiers (was 3 competing runners): coverage (nightly-coverage #2207),
  # smoke + app-eslint (daily-review-quality). One runner, one report, one nudge.
  run_coverage
  run_smoke
  run_app_eslint

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

# #3254 — close the loop: the instant the nightly finishes, ALERT each owning role of THEIR
# red suites via ops-nudge (the call-to-action; the role then acts per the attention contract,
# no Jeff in the middle). One grouped nudge per owner, not one per suite. All-green → a single
# confirmation to the nightly owner so "green" is also a signal. ops-nudge is the same primitive
# deep-health/alert-runner use (#2804); its path is env-overridable so unit tests stub it.
notify_results() {
  local results="$1"
  local ops_nudge="${OPS_NUDGE:-${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/ops-nudge}"
  [ -x "$ops_nudge" ] || { echo "notify_results: ops-nudge not executable at $ops_nudge — skipping alert" >&2; return 0; }

  local owners skipped skipmsg
  owners=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="fail" {print $4}' | sort -u)
  # #3557 — skipped (no-stack) suites are NOT failures; surface the count so the
  # morning signal is honest ("green + N skipped"), never a hidden gap.
  skipped=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="skip"' | grep -c . | tr -d ' ')
  skipmsg=""
  [ "${skipped:-0}" -gt 0 ] && skipmsg=" — $skipped skipped (no live stack, #3557)"

  if [ -z "$owners" ]; then
    "$ops_nudge" kade "nightly: all hermetic suites green ✅$skipmsg" system >/dev/null 2>&1 || true
    return 0
  fi

  local owner reds n
  while IFS= read -r owner; do
    [ -z "$owner" ] && continue
    reds=$(printf '%s\n' "$results" | awk -F'|' -v o="$owner" '$1=="SUITE" && $5=="fail" && $4==o {k=split($3,a,"/"); print a[k]}' | paste -sd', ' -)
    n=$(printf '%s\n' "$results" | awk -F'|' -v o="$owner" '$1=="SUITE" && $5=="fail" && $4==o' | grep -c .)
    "$ops_nudge" "$owner" "nightly: $n suite(s) red — $reds" system >/dev/null 2>&1 || true
  done <<< "$owners"
}

# #3484 — emit ONE structured `test.suite.result` per suite (green AND red) so
# the daily job's per-set pass/fail is queryable + dashboardable, not just a
# count nudge with the detail lost in stdout. Fed the same SUITE| results
# notify_results gets, so lint/cargo/npm/bats/cucumber all surface uniformly.
# Jeff 2026-06-20: "we need to emit logs to show which test sets pass and fail."
emit_suite_results() {
  local results="$1" line kind path owner status summary suite passed failed
  while IFS= read -r line; do
    case "$line" in SUITE\|*) ;; *) continue ;; esac
    IFS='|' read -r _tag kind path owner status summary <<< "$line"
    suite=$(basename "$path")
    # passed/failed by LABEL, not position — cucumber's "110 scenarios (45 failed,
    # 5 undefined, 60 passed)" breaks first-two-integers. Match "<N> passed|pass|ok"
    # and "<N> failed|fail" so every runner's summary maps to the real counts.
    passed=$(printf '%s' "$summary" | grep -oE '[0-9]+ (passed|pass|ok)' | grep -oE '[0-9]+' | head -1); passed=${passed:-0}
    failed=$(printf '%s' "$summary" | grep -oE '[0-9]+ (failed|fail)'   | grep -oE '[0-9]+' | head -1); failed=${failed:-0}
    # #3484 — for a red, attach a one-line reason from the captured failure log
    # (the most error-ish tail line), sanitized to a single pipe-free field, so
    # the spine event explains the failure instead of just rc.
    local reason="" flog
    if [ "$status" = "fail" ]; then
      flog=$(_fail_log_path "$kind" "$path")
      if [ -s "$flog" ]; then
        reason=$( (grep -iE 'error|panic|fail|assert' "$flog" | tail -1 || true; tail -1 "$flog") \
                  | head -1 | tr '\n|"' '   ' | tr -s ' ' | cut -c1-200 )
      fi
    fi
    if [ -n "$reason" ]; then
      spine_emit test.suite.result \
        "suite=$suite" "kind=$kind" "status=$status" \
        "passed=$passed" "failed=$failed" "owner=$owner" "reason=$reason"
    else
      spine_emit test.suite.result \
        "suite=$suite" "kind=$kind" "status=$status" \
        "passed=$passed" "failed=$failed" "owner=$owner"
    fi
  done <<< "$results"
}

# #3597 — single-flight lock. macOS has no flock, so use mkdir (atomic on every
# POSIX fs). Only one nightly run executes at a time; a second invocation while one
# is in flight exits cleanly (declared), never a silent concurrent race — that race
# is what produced the "standalone passes, parallel pressure fails" flakes the retry
# band-aid used to absorb. A stale lock (holder crashed mid-run) is stolen: if the
# recorded pid is no longer alive, reclaim it so a crash can't wedge the nightly.
NIGHTLY_LOCKDIR="${NIGHTLY_LOCKDIR:-${TMPDIR:-/tmp}/chorus-nightly-suites.lock.d}"
acquire_single_flight_lock() {
  local d="$NIGHTLY_LOCKDIR"
  if mkdir "$d" 2>/dev/null; then echo $$ > "$d/pid"; return 0; fi
  local oldpid; oldpid=$(cat "$d/pid" 2>/dev/null || true)
  if [ -n "$oldpid" ] && kill -0 "$oldpid" 2>/dev/null; then
    return 1   # a live run holds the lock
  fi
  rm -rf "$d" 2>/dev/null   # stale (dead/absent holder) — steal it
  if mkdir "$d" 2>/dev/null; then echo $$ > "$d/pid"; return 0; fi
  return 1
}
release_single_flight_lock() { rm -rf "$NIGHTLY_LOCKDIR" 2>/dev/null || true; }

# --- Dispatch ---
# Below = dispatch-only (CLI entry, exits on unknown arg).
# Above = sourceable (function definitions safe for unit tests to import).
# Guard so `source` from a unit test gets the function definitions only,
# without tripping the unknown-arg `exit 2` branch.
if [ "${BASH_SOURCE[0]}" != "${0}" ]; then
  return 0 2>/dev/null || true
fi

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
  --run-one)
    # #3606 — run a single suite exactly as the nightly would (stack-gate
    # included), emit its SUITE line, exit 0/1 on pass/fail. Gives a red suite
    # a one-command reproduction instead of a full --run-all.
    _kind="${2:-}"; _path="${3:-}"
    if [ -z "$_kind" ] || [ -z "$_path" ]; then
      echo "Usage: $0 --run-one {npm|cargo|shell|bats|cucumber} <path>" >&2; exit 2
    fi
    case "$_kind" in
      npm)   _owner=$(owner_for_npm "$_path") ;;
      cargo) _owner=$(owner_for_cargo "$_path" 2>/dev/null || echo silas) ;;
      *)     _owner="silas" ;;
    esac
    _line=$(run_one "$_kind" "$_path" "$_owner")
    printf '%s\n' "$_line"
    echo "$_line" | grep -q '|fail|' && exit 1 || exit 0
    ;;
  --run-all)
    # #3597 single-flight: refuse to run concurrently with another nightly.
    if ! acquire_single_flight_lock; then
      echo "nightly-suites: another run holds $NIGHTLY_LOCKDIR (pid $(cat "$NIGHTLY_LOCKDIR/pid" 2>/dev/null)) — exiting cleanly (single-flight, #3597)" >&2
      exit 0
    fi
    trap release_single_flight_lock EXIT
    out=$(run_all); printf '%s\n' "$out"; emit_suite_results "$out"; notify_results "$out"
    ;;
  *)
    echo "Usage: $0 {--list-npm|--list-cargo|--list-shell|--list-bats|--list-cucumber|--list-all|--run-all}" >&2
    exit 2
    ;;
esac
