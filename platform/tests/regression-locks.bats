#!/usr/bin/env bats
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

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

# ---------------------------------------------------------------------------
# Lock 1: werk-version does not bump on plain `generate`
# ---------------------------------------------------------------------------

@test "lock: claudemd-gen generate does NOT bump manifest version without CLAUDEMD_BUMP=1" {
  # Read current version
  before=$(python3 -c "import json; print(json.load(open('$CHORUS_ROOT/designing/claudemd/manifest.json'))['version'])")

  # Run generate (no bump env var set) in a mode that doesn't mutate files:
  # dry-check by snapshotting the manifest before/after a real generate run.
  cp "$CHORUS_ROOT/designing/claudemd/manifest.json" "/tmp/manifest-before-$$.json"

  python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
    "$CHORUS_ROOT/designing/claudemd/manifest.json" \
    "$CHORUS_ROOT/designing/claudemd" \
    generate "" "" >/dev/null 2>&1 || true

  after=$(python3 -c "import json; print(json.load(open('$CHORUS_ROOT/designing/claudemd/manifest.json'))['version'])")

  # Restore if anything mutated (defensive — generate shouldn't write manifest)
  if ! cmp -s "$CHORUS_ROOT/designing/claudemd/manifest.json" "/tmp/manifest-before-$$.json"; then
    cp "/tmp/manifest-before-$$.json" "$CHORUS_ROOT/designing/claudemd/manifest.json"
  fi
  rm -f "/tmp/manifest-before-$$.json"

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
  file=platform/services/chorus-hooks/src/process.rs
  has_seam=$(grep -c "CHORUS_INJECT_DRY_RUN" "$file" || true)
  has_skip_polarity=$(grep -cE 'RUN_LIVE_INJECT|HERMETIC_TEST_MODE' "$file" || true)
  [ "$has_seam" -ge 1 ] || {
    echo "Missing CHORUS_INJECT_DRY_RUN test seam in $file" >&2
    false
  }
  [ "$has_skip_polarity" -eq 0 ] || {
    echo "Skip-polarity gate reintroduced in $file (RUN_LIVE_INJECT or HERMETIC_TEST_MODE) — use dry-run seam instead" >&2
    false
  }
}

@test "lock: no 'docker' in live code paths (ADRs, journal, knowledge docs exempted)" {
  cd "$CHORUS_ROOT"
  # Scan live code paths only: platform/scripts, platform/services, platform/api.
  # Exempt: knowledge docs, ADRs, journal entries, generated TTL, node_modules.
  hits=$(grep -rni --include='*.sh' --include='*.rs' --include='*.ts' --include='*.py' \
    -l '\bdocker\b' \
    platform/scripts platform/services platform/api 2>/dev/null | \
    grep -v 'node_modules' | \
    grep -v '\.bak$' | \
    grep -v 'regression-locks.bats' || true)
  if [ -n "$hits" ]; then
    echo "'docker' reintroduced in live code paths:" >&2
    echo "$hits" >&2
    false
  fi
}
