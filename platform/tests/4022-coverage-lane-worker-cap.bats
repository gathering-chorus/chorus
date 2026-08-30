#!/usr/bin/env bats
# @test-type: unit — hermetic: CHORUS_ROOT is a tmp tree with one fake package,
# a fake `npx` on PATH records its argv and writes the summary jest would,
# ops-nudge is a stub, load is stubbed low. No live stack, no real jest.
#
# #4022 — the coverage lane ran nine `jest --coverage` in a row at jest's
# default width (a worker per core) and took the 8-core box to load 100
# (2026-08-29 11:14) BEFORE the load-aware runner lane even started. Coverage
# now gets a fixed share: --maxWorkers=${NIGHTLY_COVERAGE_WORKERS:-2}.
# Negative proof (#3734): the flag is ASSERTED on the real argv the lane passes,
# so removing it from the script goes red here; the env override is the control
# that the flag comes from this knob and not from something else on the box.

NIGHTLY="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"

setup() {
  export CHORUS_ROOT="$BATS_TEST_TMPDIR/root"; mkdir -p "$CHORUS_ROOT/pkg"
  echo '{"name":"pkg","scripts":{"test":"jest"}}' > "$CHORUS_ROOT/pkg/package.json"
  printf 'ts:\n  pkg: 10\n' > "$CHORUS_ROOT/coverage-floors.yml"
  export NIGHTLY_LOG_PATH="$BATS_TEST_TMPDIR/nightly.log"
  export NIGHTLY_LOCKDIR="$BATS_TEST_TMPDIR/lock.d"
  export NIGHTLY_NO_NUDGE=1 NIGHTLY_LOAD_STUB=0.1 NIGHTLY_LOAD_DEFER_SECS=0 NIGHTLY_LOAD_RECHECK_SECS=1
  export NIGHTLY_RECONCILE_BIN=/nonexistent/werk-test
  export CHORUS_LOG_BIN="$BATS_TEST_TMPDIR/chorus-log-stub"; printf '#!/bin/bash\nexit 0\n' > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
  export OPS_NUDGE="$BATS_TEST_TMPDIR/ops-nudge-stub"; printf '#!/bin/bash\nexit 0\n' > "$OPS_NUDGE"; chmod +x "$OPS_NUDGE"
  export HOME="$BATS_TEST_TMPDIR/home"; mkdir -p "$HOME"
  # fake npx: record argv, produce the summary the lane reads
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  cat > "$BATS_TEST_TMPDIR/bin/npx" <<'NPX'
#!/bin/bash
echo "$@" >> "$NPX_ARGV_FILE"
mkdir -p coverage
echo '{"total":{"lines":{"pct":99}}}' > coverage/coverage-summary.json
exit 0
NPX
  chmod +x "$BATS_TEST_TMPDIR/bin/npx"
  export NPX_ARGV_FILE="$BATS_TEST_TMPDIR/npx-argv.txt"
  export PATH="$BATS_TEST_TMPDIR/bin:$PATH"
}

@test "negative proof: the coverage lane's jest carries --maxWorkers=2 by default (never jest's per-core default)" {
  unset NIGHTLY_COVERAGE_WORKERS
  run "$NIGHTLY" --run-all
  [ -s "$NPX_ARGV_FILE" ]
  grep -q -- '--coverage' "$NPX_ARGV_FILE"
  grep -q -- '--maxWorkers=2' "$NPX_ARGV_FILE"
}

@test "control: NIGHTLY_COVERAGE_WORKERS drives the flag" {
  export NIGHTLY_COVERAGE_WORKERS=3
  run "$NIGHTLY" --run-all
  grep -q -- '--maxWorkers=3' "$NPX_ARGV_FILE"
  ! grep -q -- '--maxWorkers=2' "$NPX_ARGV_FILE"
}
