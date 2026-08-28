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

# #4009 — ONE run_id, minted once, stamped on every event this run emits and
# exported to every child. Today a run's events could not be correlated: the
# only way to ask "what did run X do" was to count log lines between RUN|start
# and RUN|complete and hope nobody else wrote in between. A run that cannot
# name itself cannot be traced, and an untraceable run cannot be trusted.
NIGHTLY_RUN_ID="${NIGHTLY_RUN_ID:-nr-$(date +%s)-$$}"
export NIGHTLY_RUN_ID

spine_emit() {
  local event="$1"; shift
  if [ -x "$CHORUS_LOG_BIN" ]; then
    "$CHORUS_LOG_BIN" "$event" "$NIGHTLY_ROLE" "run_id=$NIGHTLY_RUN_ID" "$@" >/dev/null 2>&1 || true
  fi
}

# #3753 — load gate. A red suite must mean the code failed, never "the machine
# was busy": 2026-08-05 receipts show 3 of 6 reds vanishing on a quiet box and
# deep-health paging "unreachable" for an app answering in 29ms. Above a
# cores-relative load threshold the run defers, then reports UNMEASURABLE — a
# typed state distinct from pass/fail/skip. Thresholds live in config
# (gate-ops ask), env-overridable; NIGHTLY_LOAD_STUB lets tests bring their own
# load (#3528) so the negative proof needs no real load generator.
NIGHTLY_LOAD_CONF="${NIGHTLY_LOAD_CONF:-$SCRIPT_DIR_NIGHTLY/nightly-load.conf}"
[ -f "$NIGHTLY_LOAD_CONF" ] && . "$NIGHTLY_LOAD_CONF"
NIGHTLY_LOAD_MAX_PER_CORE="${NIGHTLY_LOAD_MAX_PER_CORE:-1.5}"
NIGHTLY_LOAD_DEFER_SECS="${NIGHTLY_LOAD_DEFER_SECS:-900}"
NIGHTLY_LOAD_RECHECK_SECS="${NIGHTLY_LOAD_RECHECK_SECS:-60}"

_load_1m() {
  if [ -n "${NIGHTLY_LOAD_STUB:-}" ]; then printf '%s' "$NIGHTLY_LOAD_STUB"; return; fi
  sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}'
}

# #3753 AC2 — a suite that FAILED TO START is not a code failure. Remap the
# spawn/ABI class to the typed `unmeasurable` verdict always; remap the timeout
# class only when the box is loaded RIGHT NOW (a timeout on a quiet box is a
# real wedge and stays fail). Everything else passes through untouched.
_classify_verdict() {
  local verdict="$1" summary="$2"
  [ "$verdict" = "fail" ] || { printf '%s' "$verdict"; return; }
  case "$summary" in
    *"NODE_MODULE_VERSION"*|*"Cannot find module"*|*"command not found"*|*"ERR_DLOPEN_FAILED"*|*"spawn "*ENOENT*|*"DID NOT RUN"*)
      printf 'unmeasurable'; return ;;
    *"SUITE TIMEOUT"*|*"rc=124"*)
      if ! _load_gate >/dev/null; then printf 'unmeasurable'; return; fi ;;
  esac
  printf '%s' "$verdict"
}

# rc 0 = measurable (load under threshold); rc 1 = loaded. Prints "load=X max=Y".
_load_gate() {
  local cores load max
  cores=$(sysctl -n hw.ncpu 2>/dev/null || echo 8)
  load=$(_load_1m); load=${load:-0}
  max=$(awk -v c="$cores" -v f="$NIGHTLY_LOAD_MAX_PER_CORE" 'BEGIN{printf "%.1f", c*f}')
  printf 'load=%s max=%s' "$load" "$max"
  awk -v l="$load" -v m="$max" 'BEGIN{exit (l+0 <= m+0) ? 0 : 1}'
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

# #3722 — DESTRUCTIVE suites the nightly must NEVER run: they mutate live
# LaunchAgents (bootout/bootstrap). test-product-membrane.sh (#3611) stops EVERY
# com.chorus.* agent — INCLUDING com.chorus.nightly-suites, the agent running
# this — to prove gathering serves with chorus down. Run under the nightly it
# kills its own runner mid-loop: the process group dies, EXIT trap never fires,
# every agent it stopped stays down. That IS the ~13-min "untrappable" killer
# that took the nightly out from Jul 22 (membrane landed) to Aug 2 (the #3720
# black box named it). These belong to ops-run/CI-with-full-restore, not the
# in-agent nightly. Belt-and-suspenders: the suites also self-refuse under a
# com.chorus ancestor (#3722), so a stray invocation is safe too.
NIGHTLY_DESTRUCTIVE_SUITES="test-product-membrane.sh"
list_shell() {
  # #4004 — exclude by BASENAME, not by canonical path. The previous grep -vF
  # matched the literal "test-product-membrane.sh" fragment, which held for the
  # canonical copy but the bats/shell discovery also reaches werk trees, where
  # the same destructive suite lives under chorus-werk/<role>-<card>/... and
  # slipped straight past the filter (seen live: a kade-3721 copy ran and was
  # scored). Matching the basename anywhere closes that.
  find "$CHORUS_ROOT/platform/scripts" -maxdepth 1 -name "test-*.sh" -type f 2>/dev/null | sort \
    | while IFS= read -r _f; do
        case " $NIGHTLY_DESTRUCTIVE_SUITES " in
          *" $(basename "$_f") "*) continue ;;
        esac
        printf '%s\n' "$_f"
      done
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
# #3974 — RETIRED. The #3557 filename allowlist could not scale (every new
# live-stack suite needed a hand edit here); hermeticity is DECLARED per test
# in the registry and the runner derives typed skips from it (#3919). This
# stub fails loudly so any resurrected caller is a red, never a silent
# wrong-answer gate (a guard whose target is deleted must fail loudly).
_needs_stack() {
  echo "_needs_stack is RETIRED (#3974): hermeticity is registry-declared; the runner owns skips" >&2
  return 2
}

# #3559 — env prefix for an npm package's jest run. platform/api was split into
# hermetic + integration jest projects; the integration project is only
# constructed when RUN_INTEGRATION=true (jest.config.js). Set it solely for
# platform/api and solely when the live stack is up (reusing #3557's _stack_up
# probe), so a stack-down nightly runs api hermetic-only and can never false-red.
# Every other npm package → empty (hermetic-only). Pure + stack-probe-driven, so
# test-nightly-stack-gate.sh can force _STACK_PROBE and assert this hermetically.
_npm_jest_env() {
  echo "_npm_jest_env is RETIRED (#3974): werk-test --nightly sets RUN_INTEGRATION from the live stack probe" >&2
  return 2
}

# #3606 — does this package actually test with jest? Discovery (has_jest_setup)
# admits any package with a scripts.test, but a test script like `tsx --test`
# (mcp-server) is node's runner, not jest. Only run `npx jest` where jest is
# genuinely configured: scripts.test mentions jest, a `jest` key exists, or a
# jest.config.* file is present.
_npm_package_uses_jest() {
  echo "_npm_package_uses_jest is RETIRED (#3974): the runner picks jest vs the package runner itself" >&2
  return 2
}

# Run a single suite — ONE attempt, deterministic result.
# #3597: the retry-to-absorb-concurrent-flakes band-aid is GONE. It papered over
# non-determinism (a standalone pass but a parallel-pressure race) instead of
# preventing it; the single-flight lock (--run-all dispatch) prevents the overlap
# at the source, so a retry can now only HIDE a real intermittent failure. One
# attempt: a suite passes or fails, and we believe the result.
run_one() {
  local kind="$1" path="$2" owner="$3"
  # #3920/#3974 — NO tier has a walker here any more: every one-suite repro
  # routes through the same runner as the full lane, keyed by the unit the
  # registry knows (cargo = crate name; npm/bats/shell = repo-relative path).
  # The runner carries the typed #3919 stack gate — #3557's filename allowlist
  # is RETIRED (see the retirement guard in test-nightly-via-runner.sh).
  local unit
  case "$kind" in
    cargo) unit="$(basename "$path")" ;;
    *)     unit="${path#"$CHORUS_ROOT"/}" ;;
  esac
  run_cargo_lane "$unit"
}

# #3920 fold — the cargo lane, through the ONE runner. `werk-test --nightly`
# owns selection (every registered crate — the registry, not a glob), execution
# (nextest #3929), the needs-stack typed skips (#3919), and the per-case
# TestResult posts (#3592), identically to the werk gate. This function only
# FOLDS: each `nightly-unit|cargo|<crate>|verdict|summary` line the runner
# prints becomes one SUITE row (same verdict vocabulary, same parser
# downstream). A refusal, a missing binary, or a run that produced no unit
# lines is a LOUD red row — the lane never silently vanishes (#3597: either
# pass or fail, either way we know).
run_cargo_lane() {
  # #3974 — now the WHOLE runner lane: one `werk-test --nightly` invocation
  # covers cargo + npm/node + bats + shell + cucumber (registry selection,
  # typed skips, per-case posts). Each `nightly-unit|<kind>|<unit>|verdict|
  # summary` line folds to one SUITE row; ONE owner rule (owner_for) replaces
  # the three disagreeing maps. Name kept from #3920 for its call sites.
  local only="${1:-}"
  # ~/.chorus/bin is the deploy home (#2734); the LaunchAgent PATH may predate
  # it, so resolve the installed binary explicitly before giving up.
  local wt; wt=$(command -v werk-test || true)
  [ -z "$wt" ] && [ -x "$HOME/.chorus/bin/werk-test" ] && wt="$HOME/.chorus/bin/werk-test"
  if [ -z "$wt" ]; then
    echo "SUITE|runner|werk-test-nightly|silas|fail|0 pass, 1 fail (werk-test not on PATH — runner lanes DID NOT RUN, #3920/#3974)"
    return
  fi
  # #3484 — the nightly's own build lock: never contend with a role build.
  local nt="${NIGHTLY_CARGO_TARGET:-$HOME/.chorus/nightly-cargo-target}"
  local _cap rc; _cap=$(mktemp)
  # #4009 — the lane is the long pole and it used to be SILENT for its whole
  # duration (38 min on 2026-08-25), so "working" and "wedged" looked identical
  # from outside. Announce entry, then stream progress from the capture file
  # while the lane runs: one nightly.suite.observed per unit line as it appears.
  spine_emit nightly.lane.started "lane=runner" "cap=${NIGHTLY_RUNNER_LANE_TIMEOUT:-${NIGHTLY_CARGO_LANE_TIMEOUT:-7200}}"
  ( _seen=0
    while [ ! -s "$_cap" ] || kill -0 $$ 2>/dev/null; do
      sleep 10
      [ -f "$_cap" ] || continue
      _now=$(grep -c '^nightly-unit|' "$_cap" 2>/dev/null); _now=${_now:-0}
      if [ "$_now" -gt "$_seen" ]; then
        printf '%s\n' "$(sed -n "$((_seen+1)),${_now}p" "$_cap" | grep '^nightly-unit|')" | while IFS='|' read -r _ k u v s; do
          [ -n "$u" ] && spine_emit nightly.suite.observed "lane=runner" "kind=$k" "unit=$u" "verdict=$v"
        done
        _seen="$_now"
      fi
      kill -0 $$ 2>/dev/null || break
    done ) &
  _streamer=$!
  NIGHTLY_SUITE_TIMEOUT="${NIGHTLY_RUNNER_LANE_TIMEOUT:-${NIGHTLY_CARGO_LANE_TIMEOUT:-7200}}" _run_capped "$_cap" \
    env CARGO_TARGET_DIR="$nt" CHORUS_ROOT="$CHORUS_ROOT" \
    "$wt" --nightly ${only:+--crate="$only"}; rc=$?
  kill "$_streamer" 2>/dev/null; wait "$_streamer" 2>/dev/null
  spine_emit nightly.lane.completed "lane=runner" "rc=$rc"
  local out; out=$(cat "$_cap"); rm -f "$_cap"
  local units; units=$(printf '%s\n' "$out" | grep '^nightly-unit|' || true)
  if [ -z "$units" ]; then
    # No unit lines at all: refused (registry down), crashed, or timed out.
    local reason; reason=$(printf '%s\n' "$out" | grep -v '^\s*$' | tail -1 | cut -c1-160)
    echo "SUITE|runner|werk-test-nightly|silas|fail|0 pass, 1 fail (runner produced no unit results rc=$rc — ${reason:-no output})"
    return
  fi
  local kind unit verdict summary path
  while IFS='|' read -r _ kind unit verdict summary; do
    [ -z "$unit" ] && continue
    case "$kind" in
      cargo) path="platform/services/$unit" ;;
      *)     path="$unit" ;;
    esac
    verdict=$(_classify_verdict "$verdict" "$summary")
    # #4009 — a suite that produced NO counts was not measured. Reporting it as
    # "0 pass, 0 fail" let two different states (ran-and-passed-nothing vs
    # never-produced-output) share one row. #4013 — extracted to a function so
    # the proof drives the REAL logic instead of a copy that can drift.
    local _r; _r=$(_remap_unmeasured "$verdict" "$summary")
    verdict="${_r%%|*}"; summary="${_r#*|}"
    echo "SUITE|$kind|$path|$(owner_for "$path")|$verdict|$summary"
    # #3484 — persist the failing lane's output so the red explains itself;
    # clear on green so a passing rerun drops the stale reason.
    local _flog; _flog=$(_fail_log_path "$kind" "$path")
    if [ "$verdict" = "fail" ]; then
      mkdir -p "$NIGHTLY_FAIL_DIR" 2>/dev/null || true
      # #4004 — write THIS unit's slice, not the whole lane. `$out` is one
      # werk-test blob covering every unit, so writing it per failing suite
      # produced N byte-identical files (14 × 155245 on 2026-08-25): the logs
      # could not tell you WHICH suite failed, which is the only question they
      # exist to answer. The full blob is kept ONCE, referenced by name.
      local _blob="$NIGHTLY_FAIL_DIR/_lane-output.log"
      printf '%s\n' "$out" > "$_blob" 2>/dev/null || true
      {
        echo "# unit: $unit ($kind) — verdict $verdict"
        echo "# summary: $summary"
        echo "# full lane output: $_blob"
        echo "---"
        # the unit's own lines: its nightly-unit row plus anything naming it
        printf '%s\n' "$out" | grep -F -- "$unit" || echo "(no lines in the lane output named this unit)"
      } > "$_flog" 2>/dev/null || true
    else
      rm -f "$_flog" 2>/dev/null || true
    fi
  done <<EOF
$units
EOF
}

# #3974 — ONE owner-routing rule. The previous state was three disagreeing
# maps (owner_for_npm, _cov_owner, per-tier hardcodes) plus a call to an
# owner_for_cargo that never existed. Path in, owner out, everywhere.
owner_for() {
  case "$1" in
    "$APP_ROOT"|"$APP_ROOT"/*)              echo "kade" ;;
    directing/*|"$CHORUS_ROOT"/directing/*) echo "kade" ;;
    platform/*|roles/*|"$CHORUS_ROOT"/platform/*|"$CHORUS_ROOT"/roles/*) echo "silas" ;;
    *)                                      echo "kade" ;;
  esac
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
  #
  # #4004 — rc=3 is a suite's own SELF-REFUSAL ("I must not run here"), not a
  # failure. test-product-membrane exits 3 when it detects a chorus-agent
  # ancestor, exactly as #3722 designed: refusing is the correct behaviour, and
  # scoring it "0 pass, 1 fail" made a working guard read identically to a
  # broken suite — it sat red in the nightly for weeks while never running.
  # A skip is not a pass either: it reports as its own state, visible, never
  # silently green.
  if [ "$rc" -eq 0 ]; then
    echo "1 ok, 0 fail (synthesized, no parseable line)"
  elif [ "$rc" -eq 3 ]; then
    echo "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)"
  else
    echo "0 pass, 1 fail (synthesized rc=$rc, no parseable line)"
  fi
}

# #4013 — the UNMEASURED remap, as a function so its proof drives the real logic.
#
# #4004 and #4009 landed hours apart on 2026-08-26 and collided here. #4004 makes
# a suite that DECLINES to run say so ("SELF-REFUSED rc=3 — suite declined to run
# here"); #4009 matched "0 pass, 0 fail"* and overwrote it with "UNMEASURED —
# produced no parseable output". Both cards exist to stop one row meaning two
# things, and landing them separately re-merged the two states they each split:
# "working as designed" and "produced nothing readable" have different owners and
# must not share a row.
#
# The rule: a summary that already carries a parenthesised state has named itself
# and is left alone. A BARE "0 pass, 0 fail" has not, and still becomes
# UNMEASURED — the skip must not be a blanket "never rewrite", or it would
# silently undo #4009 while looking like a fix.
#
# Prints "<verdict>|<summary>".
_remap_unmeasured() {
  local verdict="$1" summary="$2"
  case "$summary" in
    "0 pass, 0 fail"|"0 pass, 0 fail "*)
      case "$summary" in
        *"("*")"*) : ;;   # already names its own state — leave it
        *) verdict="unmeasured"
           summary="0 pass, 0 fail (UNMEASURED — suite produced no parseable output)" ;;
      esac ;;
  esac
  printf '%s|%s\n' "$verdict" "$summary"
}

# #3662 — per-suite wedge guard. The Jul 17 03:00 run hung 4 days on one bats
# suite and no nightly ran 7/18–7/21. Two wedge classes, one guard:
#   (a) EOF-wait: `out=$(cmd)` waits for pipe EOF, so a suite that exits but
#       leaves a background child holding stdout wedges the capture forever.
#       Fixed by capturing to a FILE — the wrapper returns when the suite
#       process exits, regardless of who still holds the fd.
#   (b) hung suite: never exits. Fixed by the wall-clock cap — the suite runs
#       in its own process group (perl setpgrp; macOS has no setsid/timeout)
#       and the whole group is killed on expiry, recorded as rc=124.
NIGHTLY_SUITE_TIMEOUT="${NIGHTLY_SUITE_TIMEOUT:-1800}"

# _run_capped <outfile> <cmd...> — run cmd with stdout+stderr → outfile,
# capped at NIGHTLY_SUITE_TIMEOUT seconds. Returns cmd's rc, or 124 on timeout.
_run_capped() {
  local outfile="$1"; shift
  perl -e 'setpgrp(0,0); exec @ARGV or die "exec failed: $!"' "$@" \
    >"$outfile" 2>&1 </dev/null &
  local pid=$! waited=0 tick=5
  NIGHTLY_CHILD_PGID="$pid"   # #4008 — the trap reaps this group if we are killed
  [ "$NIGHTLY_SUITE_TIMEOUT" -lt 30 ] && tick=1
  # #4009 — the cap measured time-since-START, so the runner lane's 7200s meant a
  # wedge could sit two hours looking like work (2026-08-25: 90 min at 13/99).
  # What identifies a wedge is time-since-LAST-OUTPUT. Track it and kill on that,
  # keeping the total cap as the outer bound.
  local quiet_cap="${NIGHTLY_QUIET_CAP:-600}" last_size=-1 quiet=0 now_size
  NIGHTLY_CAPPED_REASON=""
  while kill -0 "$pid" 2>/dev/null; do
    now_size=$(wc -c <"$outfile" 2>/dev/null | tr -d ' ')
    if [ "${now_size:-0}" != "$last_size" ]; then last_size="${now_size:-0}"; quiet=0; else quiet=$((quiet + tick)); fi
    if [ "$quiet_cap" -gt 0 ] && [ "$quiet" -ge "$quiet_cap" ]; then
      kill -TERM -- "-$pid" 2>/dev/null || true
      sleep 2
      kill -KILL -- "-$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      NIGHTLY_CAPPED_REASON="quiet"
      printf '\nSUITE WEDGED: no output for %ss — killed (#4009 quiet-cap; total cap was %ss)\n' \
        "$quiet_cap" "$NIGHTLY_SUITE_TIMEOUT" >>"$outfile"
      return 124
    fi
    if [ "$waited" -ge "$NIGHTLY_SUITE_TIMEOUT" ]; then
      kill -TERM -- "-$pid" 2>/dev/null || true
      sleep 2
      kill -KILL -- "-$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      NIGHTLY_CAPPED_REASON="total"
      printf '\nSUITE TIMEOUT: killed after %ss (#3662 wedge guard)\n' \
        "$NIGHTLY_SUITE_TIMEOUT" >>"$outfile"
      return 124
    fi
    sleep "$tick"; waited=$((waited + tick))
  done
  wait "$pid"
  local _rc=$?
  NIGHTLY_CHILD_PGID=""
  return "$_rc"
}

# Single attempt — the original run_one body.
# #3974 — run_one_attempt RETIRED: every tier executes via `werk-test
# --nightly` (see run_cargo_lane). The per-tier walkers, their verdict
# synthesis, and the #3557 filename stack-gate live in the runner now.

# #3974 — retired map: delegates to the ONE owner rule (owner_for).
owner_for_npm() { owner_for "$1"; }

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
_cov_owner() { owner_for "$1"; }  # #3974 — delegates to the ONE owner rule
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
        # #3734 AC2 — jest writes coverage/coverage-summary.json itself, so there is
        # no redirect to truncate here; snapshot the prior artifact and restore it if
        # the run fails, so a broken package keeps its last good baseline too.
        _cov_prev=""
        if [ -f "$dir/coverage/coverage-summary.json" ]; then
          _cov_prev="$dir/coverage/.coverage-summary.prev.$$"
          cp -p "$dir/coverage/coverage-summary.json" "$_cov_prev" 2>/dev/null || _cov_prev=""
        fi
        # #3606 — --forceExit + a hard per-package timeout. WITHOUT THESE A SINGLE
        # LEAKY SUITE STALLS THE WHOLE NIGHTLY, and a stalled nightly yields NO data
        # at all — strictly worse than red data.
        #
        # Observed 2026-08-04: clearing's coverage step hung 97+ MINUTES; the run
        # never advanced past suite 1 of ~232. Cause: clearing's tests import
        # src/server.ts, which really starts the server (LAN listener, message
        # restore, buzz bridge), leaving open handles. jest waits on them forever
        # unless told otherwise — the same file run by hand with --forceExit
        # finishes in 2.6s, 16/16, printing "Force exiting Jest".
        #
        # Previously MASKED: clearing coverage used to fail fast on the `const m`
        # compile collision, so the hang was unreachable. Fixing that collision
        # correctly exposed the defect underneath.
        #
        # --forceExit unblocks THIS leak; the timeout bounds EVERY future one, so a
        # leaky suite costs its own slot and never the night. Neither fixes the
        # leaked handles in clearing — that is a separate teardown fix, and this
        # comment exists so "unblocked" is never misread as "resolved".
        _cov_timeout="${NIGHTLY_COVERAGE_TIMEOUT_S:-600}"
        (cd "$dir" && _t0=$(date +%s)
         npx --no-install jest --coverage --coverageReporters=json-summary --passWithNoTests --silent --forceExit >/dev/null 2>&1 &
         _jp=$!
         while kill -0 "$_jp" 2>/dev/null; do
           if [ $(( $(date +%s) - _t0 )) -ge "$_cov_timeout" ]; then
             kill -TERM "$_jp" 2>/dev/null; sleep 2; kill -KILL "$_jp" 2>/dev/null
             exit 124   # conventional timeout code; surfaces as a coverage failure, never a hang
           fi
           sleep 2
         done
         wait "$_jp"); rc=$?  # #3606 --no-install: fail loud, never download
        if [ "$rc" -ne 0 ] && [ -n "$_cov_prev" ] && [ -s "$_cov_prev" ]; then
          mv -f "$_cov_prev" "$dir/coverage/coverage-summary.json"
        fi
        [ -n "$_cov_prev" ] && rm -f "$_cov_prev"
        sj="$dir/coverage/coverage-summary.json"
      else rc=127; fi
      [ -f "$sj" ] && pct=$(python3 -c "import json;print(json.load(open('$sj'))['total']['statements']['pct'])" 2>/dev/null || true)
    elif [ "$lang" = "rust" ]; then
      if [ -n "$dry" ] && [ -n "$fix" ]; then sj="$fix/$rel/llvm-cov-summary.json"
      elif [ -d "$dir" ]; then
        # #3734 AC2 — write to a TEMP file and move into place only on success.
        # `>"$dir/llvm-cov-summary.json"` truncated the artifact BEFORE cargo ran,
        # so a crate that stopped compiling destroyed its own last good summary and
        # the next run had no baseline either — the failure erased its evidence.
        _cov_tmp="$dir/.llvm-cov-summary.$$.tmp"
        (cd "$dir" && cargo llvm-cov --summary-only --json >"$_cov_tmp" 2>/dev/null); rc=$?
        if [ "$rc" -eq 0 ] && [ -s "$_cov_tmp" ]; then mv -f "$_cov_tmp" "$dir/llvm-cov-summary.json"; fi
        rm -f "$_cov_tmp"
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

  # #3734 AC4 — REPORT THE DENOMINATOR. Until now the coverage tier could only
  # enumerate what it COVERS; a component absent from coverage-floors.yml was
  # invisible, not unmeasured-and-named. 3 of 25 Rust crates carry a floor and
  # nothing said so — including werk-test, the verb that gates every land.
  # The finding is the missing denominator, not the missing entries: adding one
  # crate would leave the same hole. Naming the unconfigured ones is the point,
  # because a bare ratio still hides WHICH components are unmeasured.
  _cov_denominator "$floors"
}

# #4015 — THE FOOTER. Jeff, 2026-08-26: "reports have headers and lines and data
# and at the end everything must cross reference — its not something u just throw
# away and start from scratch on every run."
#
# The ledger already exists: chorus:TestSuiteRun is the header (636 rows),
# chorus:TestResult the lines (190,941 rows), and `werk-test reconcile` computes
# registered-minus-executed — the cross-foot. It was built by #3592 in July and
# NOTHING has ever called it: no caller in this script, werk.yml, any plist or
# skill, and zero `tests.reconcile` events in the spine. So every night we recompute
# a summary string from scratch and never ask the ledger whether it adds up.
#
# Three states, kept separable (#3734). A footer that cannot fail is worse than no
# footer, because it certifies a ledger it never read:
#   pass        the census answered and every registered test ran
#   fail        the census answered and named tests that never ran
#   unmeasured  the census could not be taken (domain down, binary absent)
_reconcile_leg() {
  local bin="${NIGHTLY_RECONCILE_BIN:-$(command -v werk-test 2>/dev/null)}"
  if [ -z "$bin" ] || [ ! -x "$bin" ]; then
    echo "SUITE|reconcile|tests-domain|kade|unmeasured|0 pass, 0 fail (reconciler not found — the ledger was not cross-footed)"
    return 0
  fi
  local out rc
  # #4015 — the FLAG form, `werk-test --reconcile` (main.rs:39), not a positional
  # subcommand: a bare `reconcile` is parsed as a CARD and the verb goes looking
  # for a werk called kade-reconcile. Found by running it, twice — the footer
  # honestly reported unmeasured both times, which is correct behaviour and a
  # useless report. This is the difference between a check that refuses and a
  # check that works.
  out=$(ROLE="${NIGHTLY_ROLE:-kade}" "$bin" --reconcile 2>&1); rc=$?
  local registered; registered=$(printf '%s' "$out" | sed -n 's/.*registered \([0-9][0-9]*\).*/\1/p' | head -1)
  if [ "$rc" -ne 0 ] || [ -z "$registered" ]; then
    local why; why=$(printf '%s' "$out" | grep -v '^\s*$' | head -1 | cut -c1-110)
    echo "SUITE|reconcile|tests-domain|kade|unmeasured|0 pass, 0 fail (census could not be taken — ${why:-no output})"
    return 0
  fi
  local never; never=$(printf '%s' "$out" | sed -n 's/.*never-run (\([0-9][0-9]*\)).*/\1/p' | head -1)
  if [ -n "$never" ] && [ "$never" -gt 0 ] 2>/dev/null; then
    echo "SUITE|reconcile|tests-domain|kade|fail|0 pass, 1 fail (${never} registered test(s) never ran of ${registered} — the ledger does not cross-foot)"
  else
    echo "SUITE|reconcile|tests-domain|kade|pass|1 pass, 0 fail (${registered} registered, every one executed — ledger cross-foots)"
  fi
}

_cov_denominator() {
  local floors="$1"
  [ -d "$CHORUS_ROOT/platform/services" ] || return 0
  local configured present unconfigured=""
  configured=$(grep -c '^  platform/services/' "$floors" 2>/dev/null | tr -d ' ')
  present=0
  # #3974 — the denominator reads the REGISTRY (the same selection the runner
  # uses), so it can never disagree with the cargo lane about which crates
  # exist. Registry unreachable → LOUD glob fallback, named in the row.
  local _crates _src="registry"
  _crates=$(curl -sf -m 10 "${OWLAPI:-http://localhost:3360}/tests?limit=10000" 2>/dev/null     | python3 -c "import json,sys
rows=json.load(sys.stdin)['data']
seen=[]
for r in rows:
    fp=r.get('filePath','')
    if fp.startswith('platform/services/'):
        c=fp.split('/')[2]
        if c not in seen: seen.append(c)
print('\n'.join(sorted(seen)))" 2>/dev/null)
  if [ -z "$_crates" ]; then
    _src="glob-fallback (registry unreachable — LOUD, #3974)"
    _crates=$(for d in "$CHORUS_ROOT"/platform/services/*/; do
      [ -f "$d/Cargo.toml" ] && basename "$d"; done)
  fi
  # #4012 — a crate is a directory with a Cargo.toml. The registry yields crate
  # names from test filePaths, and platform/services/shared/ is a SOURCE directory
  # included by other crates, not a crate: it has no Cargo.toml and can never
  # carry a coverage floor. #4000 added tests referencing shared/scope_units.rs,
  # so it entered as a 21st "crate" and reddened this ratchet every night from
  # 08-22 on — a permanent red for a floor that is impossible to configure. The
  # detector must count what it actually means to count.
  while IFS= read -r c; do
    [ -z "$c" ] && continue
    [ -f "$CHORUS_ROOT/platform/services/$c/Cargo.toml" ] || continue
    present=$((present + 1))
    local rel="platform/services/$c"
    grep -q "^  ${rel}:" "$floors" 2>/dev/null || unconfigured="$unconfigured $c"
  done <<< "$_crates"
  [ "$_src" != "registry" ] && echo "coverage-denominator: source=$_src" >&2
  [ "$present" -gt 0 ] || return 0
  local n_un; n_un=$(printf '%s' "$unconfigured" | wc -w | tr -d ' ')

  # #3606 — RATCHET, not a binary gate. As first shipped (#3734, mine) this
  # failed unless EVERY crate carried a floor. 20 of 23 do not, so it was red the
  # night it landed and every night after: a permanent red teaches the team to
  # skim past red, which is precisely what a zero-red bar cannot survive. The gap
  # is real and worth surfacing — it is just not a per-RUN failure.
  #
  # Ratchet semantics, matching .clippy-baseline.json / .eslint-baseline.json:
  # the unconfigured count may only DECREASE. A new crate shipped without a floor
  # reds the nightly — the thing actually worth catching — while the standing 20
  # do not. A decrease rewrites the baseline so the gain is locked in.
  local baseline_file="${NIGHTLY_COV_DENOM_BASELINE:-$CHORUS_ROOT/.coverage-denominator-baseline}"
  local baseline
  # `< "$file"` fails in the SHELL before tr can swallow it, so a missing baseline
  # printed a spurious "No such file or directory" on every seed run. cat-then-tr
  # keeps the absent case silent, which is the normal first-run path.
  baseline=$(cat "$baseline_file" 2>/dev/null | tr -dc '0-9')
  if [ -z "$baseline" ]; then
    # First run seeds at the current count and reports the gap WITHOUT passing it
    # off as an achievement — seeding is not progress.
    printf '%s\n' "$n_un" > "$baseline_file" 2>/dev/null || true
    echo "SUITE|coverage-denominator|platform/services|kade|pass|1 pass, 0 fail (ratchet seeded at ${n_un} unconfigured of ${present} — gap recorded, may only decrease)"
    return 0
  fi

  if [ "$n_un" -eq 0 ]; then
    printf '0\n' > "$baseline_file" 2>/dev/null || true
    echo "SUITE|coverage-denominator|platform/services|kade|pass|1 pass, 0 fail (all ${present} rust crates carry a coverage floor)"
  elif [ "$n_un" -gt "$baseline" ]; then
    echo "SUITE|coverage-denominator|platform/services|kade|fail|0 pass, 1 fail (coverage-floor RATCHET DRIFTED: ${n_un} unconfigured vs baseline ${baseline} — a crate shipped without a coverage floor:${unconfigured})"
  else
    if [ "$n_un" -lt "$baseline" ]; then printf '%s\n' "$n_un" > "$baseline_file" 2>/dev/null || true; fi
    echo "SUITE|coverage-denominator|platform/services|kade|pass|1 pass, 0 fail (${configured} of ${present} crates have a floor; ${n_un} unconfigured vs baseline ${baseline} — standing gap, not drift)"
  fi
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

  # #3920/#3974 — ONE runner for every registered lane: cargo (nextest), npm
  # (jest or the package's own node:test/cucumber runner), bats, and shell —
  # selection from the registry, typed needs-stack skips (#3919), per-case
  # TestResult posts (#3592), via a single `werk-test --nightly`. The
  # list_npm/list_bats/list_shell/list_cucumber globs no longer drive
  # execution (kept for --list-* introspection only).
  run_cargo_lane

  # #4015 — THE FOOTER, LAST. Every leg above reports what it ran; this one asks
  # the ledger whether what ran accounts for what is registered. It goes last
  # because a footer cross-foots the lines above it.
  _reconcile_leg
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

  # #3922 — the security lane routes to the SECURITY owner as its own signal,
  # never buried in the per-owner wall. Owner is env-overridable; the model
  # (security domain ownedBy) is the authority when they disagree.
  local sec_owner="${NIGHTLY_SECURITY_OWNER:-silas}"
  local sec_reds sec_n
  sec_reds=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $2=="security" && $5=="fail" {k=split($3,a,"/"); print a[k]}' | paste -sd', ' -)
  sec_n=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $2=="security" && $5=="fail"' | grep -c .)
  if [ "${sec_n:-0}" -gt 0 ]; then
    "$ops_nudge" "$sec_owner" "SECURITY lane: $sec_n red — $sec_reds (#3922 — own cadence, own signal)" system >/dev/null 2>&1 || true
  fi

  local owner reds n
  while IFS= read -r owner; do
    [ -z "$owner" ] && continue
    reds=$(printf '%s\n' "$results" | awk -F'|' -v o="$owner" '$1=="SUITE" && $5=="fail" && $4==o {k=split($3,a,"/"); print a[k]}' | paste -sd', ' -)
    n=$(printf '%s\n' "$results" | awk -F'|' -v o="$owner" '$1=="SUITE" && $5=="fail" && $4==o' | grep -c .)
    "$ops_nudge" "$owner" "nightly: $n suite(s) red — $reds" system >/dev/null 2>&1 || true
  done <<< "$owners"

  # #3606 — THE AGGREGATE. Everything above is correctly scoped to ONE owner, and
  # that scoping is the gap: each role is told their slice and nobody is told the
  # run. On 2026-08-04 the board was 31 red (silas 25, kade 6). My nudge said
  # "6 suite(s) red" and it was CORRECT — I read my own slice as the total and
  # reported a 5x undercount that did not exist. That misread is the tell: two
  # roles can each receive an accurate nudge and both conclude the nightly is
  # nearly fine while main is 31 red.
  #
  # Jeff's bar is ZERO RED ACROSS THE BOARD. Nothing measured against that bar
  # because no signal carried a board-level number. This does.
  local total per_owner
  total=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="fail"' | grep -c . | tr -d ' ')
  per_owner=$(printf '%s\n' "$results" \
    | awk -F'|' '$1=="SUITE" && $5=="fail" {c[$4]++} END {for (o in c) printf "%s %d, ", o, c[o]}' \
    | sed 's/, $//')
  "$ops_nudge" kade "nightly TOTAL: $total red across the board ($per_owner) — bar is zero$skipmsg" system >/dev/null 2>&1 || true
}

# #3606 — the DURABLE half of the aggregate. notify_results pushes a nudge, which
# is read once and gone; this emits ONE queryable event per run carrying the
# board-level totals, so the zero-red bar has something a dashboard, the daily
# review, or a future consumer can read without re-aggregating 231 per-suite
# events or parsing the log.
#
# Mirrors #3484's reasoning one level up: that card made per-SUITE results
# queryable instead of stdout-only; the run itself was still only ever a count in
# a nudge. Emitted for GREEN runs too — "0 red" is the measurement that proves the
# bar was met, and a bar you only hear about when it breaks cannot be shown held.
emit_run_summary() {
  local results="$1" total suites passed failed skipped owners_csv zero_red
  suites=$(printf '%s\n' "$results" | grep -c '^SUITE|' | tr -d ' ')
  total=$(printf '%s\n'  "$results" | awk -F'|' '$1=="SUITE" && $5=="fail"' | grep -c . | tr -d ' ')
  passed=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="pass"' | grep -c . | tr -d ' ')
  skipped=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="skip"' | grep -c . | tr -d ' ')
  # #3753 — the typed fourth state: counted separately, never folded into red
  local unmeasurable
  unmeasurable=$(printf '%s\n' "$results" | awk -F'|' '$1=="SUITE" && $5=="unmeasurable"' | grep -c . | tr -d ' ')
  failed="$total"
  owners_csv=$(printf '%s\n' "$results" \
    | awk -F'|' '$1=="SUITE" && $5=="fail" {c[$4]++} END {for (o in c) printf "%s=%d;", o, c[o]}' \
    | sed 's/;$//')
  [ "${total:-0}" -eq 0 ] && zero_red=true || zero_red=false
  spine_emit nightly.run.summary \
    "suites=$suites" "passed=$passed" "failed=$failed" "skipped=$skipped" \
    "unmeasurable=$unmeasurable" \
    "red_by_owner=${owners_csv:-none}" "zero_red=$zero_red"
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
    # #3753 AC4 — the reporter contradiction class, closed at emit time: a
    # SUITE row must never say status=pass while its own summary counts
    # failures. Assert loud, emit the contradiction as its own event, and
    # carry the row as fail — the two states must stay separable (#3734).
    if [ "$status" = "pass" ] && [ "${failed:-0}" -gt 0 ]; then
      echo "nightly-suites: REPORTER CONTRADICTION — $suite says pass with failed=$failed; recording fail (#3753 AC4)" >&2
      spine_emit nightly.reporter.contradiction "suite=$suite" "kind=$kind" "failed=$failed"
      status="fail"
    fi
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
# #4008/#4009 — the trap freed the LOCK but never killed the lane, so a killed
# wrapper left its runner alive: on 2026-08-25 an orphan ran 1h52m while a new
# run took the freed lock and ran a second lane beside it. Kill the child's
# process group first, then release. _run_capped already setpgrp's the child;
# NIGHTLY_CHILD_PGID is set there so the trap knows what to reap.
release_single_flight_lock() {
  if [ -n "${NIGHTLY_CHILD_PGID:-}" ]; then
    kill -TERM -- "-$NIGHTLY_CHILD_PGID" 2>/dev/null || true
    sleep 1
    kill -KILL -- "-$NIGHTLY_CHILD_PGID" 2>/dev/null || true
  fi
  rm -rf "$NIGHTLY_LOCKDIR" 2>/dev/null || true
}

# #3709 — OWN THE RESULTS FILE. Until now --run-all only printed to stdout and
# the aggregate log existed solely because launchd redirected StandardOutPath
# into it. That redirect stopped landing on 2026-07-22: the suites still ran
# nightly and still wrote per-suite failure files, but nothing reached the
# aggregate, and daily-review-quality.sh reads only the aggregate. Seven days of
# red went unreported while --last-run said, accurately, "the 03:00 run did not
# write". A results file that exists only when an external redirect happens to
# work is a single point of failure for the whole reporting chain.
#
# Appends (never truncates): --last-run walks backward through concatenated runs
# to find the block boundary, so history must survive. #3720: this buffered
# writer is now the FALLBACK for an unappendable log only — the primary path
# streams per-suite via tee, so a killed run persists everything it finished.
# Failure to persist is LOUD; silence is what cost the week.
persist_run_results() {
  local _out="$1"
  local _log="${NIGHTLY_LOG_PATH:-$HOME/Library/Logs/Chorus/nightly-suites.log}"
  # No "is fd 1 already this file?" test here, deliberately. Two attempts at one
  # both failed on macOS: `$(stat … /dev/stdout)` reads the command
  # substitution's PIPE rather than the caller's fd 1, and `[ /dev/fd/1 -ef … ]`
  # is false under bash even when fd 1 IS the file, because /dev/fd reports a
  # synthetic st_dev and bash's -ef compares device AND inode. Rather than carry
  # a clever test that silently does nothing, the CALLER simply does not echo the
  # results when it hands them here — so this function is the single writer and
  # duplication is structurally impossible. Fewer moving parts, no fd forensics.
  mkdir -p "$(dirname "$_log")" 2>/dev/null
  if ! printf '%s\n' "$_out" >> "$_log" 2>/dev/null; then
    echo "nightly-suites: WARNING — could not persist results to $_log; this run's results reach NOBODY (daily-review reads only this file)" >&2
    return 1
  fi
  return 0
}

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
  --last-run)
    # #3606/#3272 — READ mode: replay the LATEST run's SUITE lines from the
    # log (launchd StandardOutPath appends runs with no separators; the block
    # boundary is the first repeated (kind,path) key walking backward).
    # daily-review consumes THIS instead of re-running --run-all in a launchd
    # env with no cargo (the 2026-07-04 false 30-red wall). No third state:
    # missing or stale (>26h) log = a loud, parseable SUITE fail + rc 1.
    _log="${NIGHTLY_LOG_PATH:-$HOME/Library/Logs/Chorus/nightly-suites.log}"
    if [ ! -f "$_log" ]; then
      echo "SUITE|meta|$_log|silas|fail|0 pass, 1 fail (nightly log MISSING — no run to read, run the 03:00 nightly)"
      exit 1
    fi
    _age=$(( $(date +%s) - $(stat -f %m "$_log" 2>/dev/null || echo 0) ))
    if [ "$_age" -gt 93600 ]; then
      echo "SUITE|meta|$_log|silas|fail|0 pass, 1 fail (nightly log STALE — ${_age}s old > 26h; the 03:00 run did not write)"
      exit 1
    fi
    python3 - "$_log" <<'PYEOF'
# #3725 AC2/AC3 — scope to ONE run using the RUN|start marker, and say so when a
# run never finished.
#
# AC2: the old boundary heuristic was "walk backward, stop at the first repeated
# (kind,path) key" (#3606/#3272), written before #3720 added RUN| markers. It
# assumes consecutive runs cover the SAME suite set. Both halves of that broke on
# 2026-08-02: the 03:00 run died partway (81 of ~229 suites), and the lines before
# it came from a werk root (chorus-werk/kade-3721/...) whose paths differ from
# canonical — so no key ever repeated, no boundary was found, and --last-run
# returned 289 suites / 41 fail spanning TWO runs. The real answer was 81 suites,
# 68 pass / 10 fail / 3 skip. Jeff was given the blended number twice.
#
# The marker is authoritative and already written; use it instead of guessing.
#
# AC3: RUN|start with no RUN|complete means the run was KILLED (#3720 put the
# markers there for exactly this forensic). Nothing read them, so a run that died
# at suite 81 looked like a normal short night. That is now a loud meta line — a
# partial red list must never be mistaken for a full one.
import sys
raw = open(sys.argv[1], errors='replace').read().splitlines()

start_idx = None
for i in range(len(raw) - 1, -1, -1):
    if raw[i].startswith('RUN|start|'):
        start_idx = i
        break

if start_idx is None:
    # Legacy log with no markers — fall back to the old heuristic rather than
    # returning nothing. Fidelity is lower; that is why the markers exist.
    lines = [l for l in raw if l.startswith('SUITE|')]
    seen = set(); run = []
    for l in reversed(lines):
        p = l.split('|'); key = (p[1], p[2])
        if key in seen:
            break
        seen.add(key); run.append(l)
    for l in reversed(run):
        print(l)
    sys.exit(0)

block = raw[start_idx:]
completed = any(l.startswith('RUN|complete|') for l in block)
suites = [l for l in block if l.startswith('SUITE|')]

if not completed:
    started = raw[start_idx].split('|')[2] if len(raw[start_idx].split('|')) > 2 else '?'
    print(
        'SUITE|meta|nightly-run-incomplete|silas|fail|0 pass, 1 fail '
        '(run started %s and never wrote RUN|complete — KILLED after %d suite(s); '
        'the results below are PARTIAL, not a full night)' % (started, len(suites))
    )

for l in suites:
    print(l)
PYEOF
    ;;
  --run-one)
    # #3606 — run a single suite exactly as the nightly would (stack-gate
    # included), emit its SUITE line, exit 0/1 on pass/fail. Gives a red suite
    # a one-command reproduction instead of a full --run-all.
    _kind="${2:-}"; _path="${3:-}"
    if [ -z "$_kind" ] || [ -z "$_path" ]; then
      echo "Usage: $0 --run-one {npm|cargo|shell|bats|cucumber} <path>" >&2; exit 2
    fi
    # #3974 — one owner rule; the dead owner_for_cargo call is gone.
    _owner=$(owner_for "$_path")
    _line=$(run_one "$_kind" "$_path" "$_owner")
    printf '%s\n' "$_line"
    echo "$_line" | grep -q '|fail|' && exit 1 || exit 0
    ;;
  --classify)
    # #3753 — test seam for the AC2 verdict remap (same rationale as --load-gate).
    _classify_verdict "${2:-fail}" "${3:-}"
    echo
    exit 0
    ;;
  --load-gate)
    # #3753 — expose the gate as its own verb so tests and peers (deep-health,
    # clearing-probe) share one predicate. rc 0 = measurable, 1 = loaded.
    _lg=$(_load_gate); _lg_rc=$?
    echo "$_lg"
    exit "$_lg_rc"
    ;;
  --run-all)
    # #3597 single-flight: refuse to run concurrently with another nightly.
    if ! acquire_single_flight_lock; then
      echo "nightly-suites: another run holds $NIGHTLY_LOCKDIR (pid $(cat "$NIGHTLY_LOCKDIR/pid" 2>/dev/null)) — exiting cleanly (single-flight, #3597)" >&2
      exit 0
    fi
    trap release_single_flight_lock EXIT
    # #3753 — load gate: defer while the box is busy; if it never quiets inside
    # the defer window, the run is UNMEASURABLE (typed, logged, spine-emitted),
    # never a wall of false red.
    _lg=$(_load_gate); _lg_rc=$?
    if [ "$_lg_rc" -ne 0 ]; then
      _deferred=0
      while [ "$_lg_rc" -ne 0 ] && [ "$_deferred" -lt "$NIGHTLY_LOAD_DEFER_SECS" ]; do
        echo "nightly-suites: box loaded ($_lg) — deferring ${NIGHTLY_LOAD_RECHECK_SECS}s (${_deferred}/${NIGHTLY_LOAD_DEFER_SECS}s used, #3753)" >&2
        sleep "$NIGHTLY_LOAD_RECHECK_SECS"
        _deferred=$((_deferred + NIGHTLY_LOAD_RECHECK_SECS))
        _lg=$(_load_gate); _lg_rc=$?
      done
      if [ "$_lg_rc" -ne 0 ]; then
        _um_log="${NIGHTLY_LOG_PATH:-$HOME/Library/Logs/Chorus/nightly-suites.log}"
        mkdir -p "$(dirname "$_um_log")" 2>/dev/null
        printf 'RUN|unmeasurable|%s|%s|deferred=%ss\n' "$(date '+%Y-%m-%dT%H:%M:%S')" "$_lg" "$_deferred" >> "$_um_log" 2>/dev/null
        spine_emit nightly.run.unmeasurable "$_lg" "deferred_s=$_deferred" "reason=load"
        spine_emit nightly.run.summary "suites=0" "passed=0" "failed=0" "skipped=0" \
          "unmeasurable=all" "red_by_owner=none" "zero_red=unmeasurable" "$_lg"
        echo "nightly-suites: UNMEASURABLE — $_lg after ${_deferred}s defer; zero-red bar NOT measured tonight (#3753)" >&2
        exit 0
      fi
    fi
    # #3722 — WERK ISOLATION: a run launched from a card's werk must NOT write
    # the canonical nightly log or fire the team red alert (Kade's kade-3721 run
    # did exactly that Aug 1: 34-red paged Jeff, and daily-review --last-run would
    # read a werk snapshot as the canonical record). If CHORUS_ROOT is a werk
    # path and no explicit NIGHTLY_LOG_PATH was set, auto-isolate to a werk-local
    # log and skip the team nudge.
    # #4022 — the two halves were coupled: an EXPLICIT NIGHTLY_LOG_PATH skipped
    # the whole block, so a werk run with its own log path still paged the team
    # (2026-08-28 13:17, "30 red across the board" from kade-4022's demo run —
    # the exact page #3722 exists to prevent). Isolation of the log and silence
    # toward the team are separate facts; a werk root implies both.
    case "$CHORUS_ROOT" in
      *"/chorus-werk/"*)
        if [ -z "${NIGHTLY_LOG_PATH:-}" ]; then
          export NIGHTLY_LOG_PATH="/tmp/nightly-$(basename "$CHORUS_ROOT").log"
        fi
        export NIGHTLY_NO_NUDGE=1
        echo "nightly-suites: WERK RUN — isolated to $NIGHTLY_LOG_PATH, team nudge suppressed (#3722)" >&2 ;;
    esac
    # #3720 — INCREMENTAL persistence: every SUITE line lands in the log AS IT
    # COMPLETES (tee is the single writer), so a killed run leaves its partial
    # evidence — the suite in flight at death is the last line's successor.
    # The buffered persist_run_results path (#3709) proved worthless against
    # the recurring mid-run killer: `out=$(run_all)` holds everything in
    # memory, and a SIGKILL 13 minutes in left the log untouched since Jul 22.
    # RUN|start / RUN|complete markers bracket the block: start-without-
    # complete = the run was killed (forensics for free); SUITE parsers
    # (--last-run, daily-review) ignore non-SUITE lines by construction.
    _run_log="${NIGHTLY_LOG_PATH:-$HOME/Library/Logs/Chorus/nightly-suites.log}"
    mkdir -p "$(dirname "$_run_log")" 2>/dev/null
    if printf 'RUN|start|%s|pid=%s\n' "$(date '+%Y-%m-%dT%H:%M:%S')" "$$" >> "$_run_log" 2>/dev/null; then
      out=$(run_all | tee -a "$_run_log")
      printf 'RUN|complete|%s|suites=%s\n' "$(date '+%Y-%m-%dT%H:%M:%S')" "$(printf '%s\n' "$out" | grep -c '^SUITE|' | tr -d ' ')" >> "$_run_log"
    else
      # log unappendable: run anyway, fall back to the buffered writer, say so
      echo "nightly-suites: WARNING — cannot append to $_run_log; no incremental persistence this run" >&2
      out=$(run_all)
      persist_run_results "$out"
    fi
    # Echo to stdout only for a human at a terminal; under launchd fd 1 IS the
    # log and printing here would duplicate what tee already wrote.
    [ -t 1 ] && printf '%s\n' "$out"
    emit_run_summary "$out"
    emit_suite_results "$out"; [ "${NIGHTLY_NO_NUDGE:-0}" = "1" ] || notify_results "$out"
    ;;
  *)
    echo "Usage: $0 {--list-npm|--list-cargo|--list-shell|--list-bats|--list-cucumber|--list-all|--run-all|--last-run|--run-one <kind> <path>}" >&2
    exit 2
    ;;
esac
