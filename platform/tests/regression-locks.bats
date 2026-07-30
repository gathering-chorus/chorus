#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# regression-locks.bats
#
# Invariant tests for recurring regressions. Each test asserts a structural
# property that has been "fixed" before and reintroduced. A regression lock
# lives here so the next reintroduction fails at commit, not at user surface.
#
# Current locks:
#   1. Werk version is a version, not a session counter.
#      Generating CLAUDE.md without CLAUDEMD_BUMP=1 must NOT change
#      manifest.json version. (Jeff asked 5× in 3 months.)
#
#   2. All osascript calls go through chorus-inject (#2077).
#      chorus-hooks must not invoke osascript directly.
#
#   3. Docker is retired (#2020, #2119).
#      Live code paths (excluding ADRs, journal, guardrails, knowledge docs)
#      must not contain the literal token `docker`.

CHORUS_ROOT="${CHORUS_ROOT:-${CHORUS_ROOT}}"

# ---------------------------------------------------------------------------
# Lock 1: werk-version does not bump on plain `generate`
# ---------------------------------------------------------------------------

@test "lock: claudemd-gen generate does NOT bump manifest version without CLAUDEMD_BUMP=1" {
  # #3710, two fixes:
  #  KEY  — the counter is `_build`, not `version`; the manifest's own
  #         _versioning_rule names _build as the monotonic integer that bumps on
  #         protocol changes. Reading ['version'] threw KeyError, so this lock
  #         had stopped guarding anything.
  #  SCOPE — it used to run the generator against the REAL manifest and restore
  #         it afterwards. A test that mutates production state and hopes to put
  #         it back is an incident waiting for a mid-run failure; it now works on
  #         its own copy, so canonical is never written at all.
  local work="${BATS_TEST_TMPDIR:-/tmp}/claudemd-lock-$$"
  mkdir -p "$work"
  cp -R "$CHORUS_ROOT/designing/claudemd/." "$work/"

  read_build() { python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['_build'])" "$1"; }
  before=$(read_build "$work/manifest.json")

  python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
    "$work/manifest.json" "$work" generate "" "" >/dev/null 2>&1 || true

  after=$(read_build "$work/manifest.json")
  rm -rf "$work"

  [ "$before" = "$after" ]
}

# ---------------------------------------------------------------------------
# Lock 2: no direct osascript in chorus-hooks (must route via chorus-inject)
# ---------------------------------------------------------------------------

@test "lock: chorus-hooks contains no direct Command::new(\"osascript\") — must route via chorus-inject" {
  cd "$CHORUS_ROOT"
  # Scan Rust source of chorus-hooks. Allow osascript in health.rs and
  # context_cache.rs — those are Finder queries (Automation, not Accessibility)
  # and do not conflict with the chorus-inject keystroke grant. The
  # anti-pattern is a NEW osascript keystroke call in nudge/process.
  offenders=$(grep -rn 'Command::new("osascript")\|Cmd::new("osascript")' \
    platform/services/chorus-hooks/src/nudge.rs \
    platform/services/chorus-hooks/src/process.rs \
    2>/dev/null || true)
  if [ -n "$offenders" ]; then
    echo "Direct osascript call reintroduced (should route via chorus-inject):" >&2
    echo "$offenders" >&2
    false
  fi
}

# ---------------------------------------------------------------------------
# Lock 3: docker absent from live code paths (#2020 retirement, #2119 purge)
# ---------------------------------------------------------------------------

@test "lock: chorus-hooks inject tests use CHORUS_INJECT_DRY_RUN seam, not a skip-gate" {
  cd "$CHORUS_ROOT"
  # On 2026-04-17 a global "skip unless opt-in" polarity flip meant nudge
  # delivery tests never ran; a global "fire unless opt-out" polarity stormed
  # Jeff's terminal. The correct shape (matching chorus-inject integration
  # tests): set CHORUS_INJECT_DRY_RUN in the test so the subprocess runs the
  # real path but short-circuits before osascript. Both fire-by-default and
  # skip-by-default are regressions.
  # #3710 — the seam moved crates. #2804 took delivery out of chorus-hooks and
  # into chorus-inject, so CHORUS_INJECT_DRY_RUN lives in chorus-inject/src now.
  # The lock kept pointing at chorus-hooks/src/process.rs and failed on a file
  # that had simply stopped owning the behaviour. The invariant is unchanged:
  # the dry-run seam must exist, and skip-polarity gates must not come back.
  inject_src=platform/services/chorus-inject/src
  has_seam=$(grep -rc "CHORUS_INJECT_DRY_RUN" "$inject_src" 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
  # Skip-polarity must stay absent from BOTH crates — either one reintroducing
  # it recreates the 2026-04-17 never-ran / always-fired pair.
  has_skip_polarity=$(grep -rlE 'RUN_LIVE_INJECT|HERMETIC_TEST_MODE' \
    "$inject_src" platform/services/chorus-hooks/src 2>/dev/null | wc -l | tr -d ' ')
  [ "$has_seam" -ge 1 ] || {
    echo "Missing CHORUS_INJECT_DRY_RUN test seam in $inject_src" >&2
    false
  }
  [ "$has_skip_polarity" -eq 0 ] || {
    echo "Skip-polarity gate reintroduced (RUN_LIVE_INJECT or HERMETIC_TEST_MODE) — use dry-run seam instead" >&2
    false
  }
}

@test "lock: no 'docker' in live code paths (ADRs, journal, knowledge docs exempted)" {
  cd "$CHORUS_ROOT"
  # Scan live code paths only: platform/scripts, platform/services, platform/api.
  # Exempt: knowledge docs, ADRs, journal entries, generated TTL, node_modules.
  #
  # #3710 — this lock matched the literal token anywhere and had gone red on
  # three hits, only one of which was docker doing anything:
  #   - doc-catalog.ts     the word inside a regex that CATEGORISES docs as
  #                        "Infrastructure". Classifying the word is not using it.
  #   - *.test.ts fixtures fixture data ABOUT the docker guard (DOCKER_BLOCKED,
  #                        cmd: 'docker rm -f x'). A test proving the guard fires
  #                        must be allowed to name what it blocks.
  #   - index-all-sources-deps.ts  a REAL `docker compose exec` — the Buzz
  #                        postgres read on Bedroom, which #3674 deliberately
  #                        runs on Bedroom's existing Docker Desktop.
  # A lock that is permanently red guards nothing, so the sanctioned exception is
  # named here explicitly. If the Buzz transport changes, delete the line and the
  # lock tightens again — that is the point of listing it rather than widening
  # the pattern.
  hits=$(grep -rni --include='*.sh' --include='*.rs' --include='*.ts' --include='*.py' \
    -l '\bdocker\b' \
    platform/scripts platform/services platform/api 2>/dev/null | \
    grep -v 'node_modules' | \
    grep -v '\.bak$' | \
    grep -v 'regression-locks.bats' | \
    grep -v '\.test\.ts$' | \
    grep -v 'platform/api/src/handlers/doc-catalog\.ts$' | \
    grep -v 'platform/api/src/index-all-sources-deps\.ts$' || true)
  if [ -n "$hits" ]; then
    echo "'docker' reintroduced in live code paths:" >&2
    echo "$hits" >&2
    false
  fi
}

@test "lock: the Buzz docker exception stays a single, named transport (#3674)" {
  cd "$CHORUS_ROOT"
  # The exemption above is only safe while it stays one call site. This fails if
  # the Buzz transport grows more docker invocations, so the exception cannot
  # quietly become a habit.
  # Count INVOCATIONS, not mentions — the file also documents the transport in a
  # comment, and prose describing the exception is not another exception.
  count=$(grep 'docker compose' platform/api/src/index-all-sources-deps.ts 2>/dev/null \
    | grep -vE '^\s*(//|\*|/\*)' | wc -l | tr -d ' ')
  [ "$count" -le 1 ]
}
