#!/usr/bin/env bats
# @test-type: integration — service-hitting; skip-if-absent
# Subject: deep-health.sh always reports, even when a check reports a finding.
#
# The class, twice now. #3369: an unguarded grep under `set -e` killed the script
# and it exited 1 with ZERO output. 2026-09-04: the same shape at the boot-herd
# check — `_herd=$("$_BOOT_AUDIT"); _herd_rc=$?` — a command substitution whose
# non-zero exit trips `set -e` BEFORE $? is ever read. The check that exists to
# tell us what broke told us nothing, and four suites went red behind it.
#
# A monitor that dies on a finding is worse than no monitor: silence reads as
# "not run" and a finding reads as silence.
load test_helper

DEEP_HEALTH="$CHORUS_ROOT/platform/scripts/deep-health.sh"

setup() {
  membrane_world
  export HEALTH_JSON_OUT="$BATS_TEST_TMPDIR/deep-health.json"
  export HEALTH_STATE_FILE="$BATS_TEST_TMPDIR/last-failures.txt"
  export HEALTH_OPS_NUDGE="/usr/bin/true"
  export HEALTH_CHORUS_LOG="/usr/bin/true"
}

_stub_audit() {  # $1 = exit code
  local f="$BATS_TEST_TMPDIR/stub-audit.sh"
  cat > "$f" <<STUB
#!/usr/bin/env bash
echo "ran" > "$BATS_TEST_TMPDIR/audit-ran"
echo "boot-herd: com.chorus.fixture RunAtLoad=true with interval=3600 (/dev/null)"
exit $1
STUB
  chmod +x "$f"
  echo "$f"
}

@test "deep-health still reports when the boot-herd check finds a violation" {
  local stub; stub="$(_stub_audit 1)"
  run env HEALTH_BOOT_AUDIT="$stub" bash "$DEEP_HEALTH"
  # the stub must actually have been used — otherwise this test passes off a
  # clean real audit and proves nothing about the finding path
  [ -f "$BATS_TEST_TMPDIR/audit-ran" ]
  [ -n "$output" ]
  echo "$output" | grep -qiE "summary|failure|warning|passed"
  # the finding itself has to reach the report, not just the report survive
  grep -q 'boot-herd' "$HEALTH_JSON_OUT"
}

@test "deep-health still reports when the boot-herd check cannot read the dir" {
  local stub; stub="$(_stub_audit 2)"
  run env HEALTH_BOOT_AUDIT="$stub" bash "$DEEP_HEALTH"
  [ -f "$BATS_TEST_TMPDIR/audit-ran" ]
  [ -n "$output" ]
  echo "$output" | grep -qiE "summary|failure|warning|passed"
}

@test "NEGATIVE PROOF: the unguarded idiom dies before the summary, the guarded one does not" {
  # The two states this suite exists to separate, run side by side. Without
  # this, the tests above stay green against a script that never calls the
  # audit at all.
  local stub; stub="$(_stub_audit 1)"

  cat > "$BATS_TEST_TMPDIR/unguarded.sh" <<UNG
#!/usr/bin/env bash
set -euo pipefail
_h=\$("$stub"); _rc=\$?
echo "SUMMARY reached (rc=\$_rc)"
UNG
  cat > "$BATS_TEST_TMPDIR/guarded.sh" <<GRD
#!/usr/bin/env bash
set -euo pipefail
_rc=0; _h=\$("$stub") || _rc=\$?
echo "SUMMARY reached (rc=\$_rc)"
GRD
  chmod +x "$BATS_TEST_TMPDIR/unguarded.sh" "$BATS_TEST_TMPDIR/guarded.sh"

  run bash "$BATS_TEST_TMPDIR/unguarded.sh"
  ! echo "$output" | grep -q "SUMMARY reached"

  run bash "$BATS_TEST_TMPDIR/guarded.sh"
  echo "$output" | grep -q "SUMMARY reached (rc=1)"
}

@test "deep-health syntax is valid bash" {
  bash -n "$DEEP_HEALTH"
}
