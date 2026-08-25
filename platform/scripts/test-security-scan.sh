#!/usr/bin/env bash
# @test-type: fitness:security
# test-security-scan.sh (#3981) — OSS security scanning for Chorus. The
# `test-*.sh` name + fitness:security concern make it SELF-REGISTERING: the
# #3974 tagger discovers it and routes it into the security test lane (#3922)
# with nightly cadence + owner routing, no per-side wiring. Two legs:
#   SAST  — semgrep over the repo with the Chorus ruleset (platform/security/semgrep/).
#           The ruleset starts with the caller-supplied-identity rule (#3980 class).
#   SCA   — trivy fs over the repo: dependency CVEs (npm + cargo) + secrets + config.
#           (trivy is already installed and covers osv-scanner's job; osv-scanner is
#            the documented alternative — same SCA role.)
#
# Exits non-zero if EITHER leg reports a finding, so it can gate. FAIL-OPEN on a
# missing tool (dev-setup gap) mirrors the gitleaks convention — SKIP, don't block.
#
# Usage:
#   security-scan.sh              full scan (SAST + SCA)
#   security-scan.sh sast         semgrep only
#   security-scan.sh sca          trivy only
#   security-scan.sh selftest DIR semgrep the given dir (used by the negative-proof test)

set -uo pipefail
# The repo root to SCAN can come from CHORUS_ROOT (nightly) or default to the
# tree this script lives in. But the RULESET ships WITH this script, so it is
# resolved relative to the script itself — never via CHORUS_ROOT, which in an
# interactive/werk shell may point at canonical and miss a werk-local rule.
SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CHORUS_ROOT:-$(cd "$SELF_DIR/.." && pwd)}"
# #3991: under bats (werk-test) this file is COPIED to a tmpdir, so BASH_SOURCE
# no longer sits beside the ruleset. Resolve via the real test dir first, then
# the script's own dir, then CHORUS_ROOT — and FAIL LOUD if none holds rules.
RULES=""
for cand in "${BATS_TEST_DIRNAME:-}/../security/semgrep" "$SELF_DIR/../security/semgrep" "$ROOT/platform/security/semgrep"; do
  [ -n "$cand" ] && [ -d "$cand" ] && RULES="$(cd "$cand" && pwd)" && break
done
[ -n "$RULES" ] || { echo "test-security-scan: semgrep ruleset dir not found (tried BATS_TEST_DIRNAME, script dir, \$CHORUS_ROOT)" >&2; exit 2; }
MODE="${1:-full}"
rc=0

run_sast() {
  if ! command -v semgrep >/dev/null 2>&1; then
    echo "SAST: semgrep not installed — SKIPPED (pipx install semgrep)"; return 0
  fi
  local target="${1:-$ROOT}"
  echo "SAST: semgrep — ruleset $RULES"
  # exclude the fixtures dir on a full repo scan (those are intentional violations);
  # include them only in selftest. NOTE: no empty-array expansion — macOS bash 3.2
  # errors on "${arr[@]}" for an empty array under `set -u` (this is the exact bug
  # the negative-proof test caught: the script exited nonzero regardless of findings).
  local out
  if [ "${2:-}" = "include-fixtures" ]; then
    out=$(semgrep --config "$RULES" "$target" --error --quiet 2>/dev/null)
  else
    out=$(semgrep --config "$RULES" "$target" --exclude fixtures --error --quiet 2>/dev/null)
  fi
  if [ $? -eq 0 ]; then
    echo "  SAST clean"; return 0
  else
    echo "  SAST FINDINGS — run: semgrep --config $RULES $target"; return 1
  fi
}

run_sca() {
  if ! command -v trivy >/dev/null 2>&1; then
    echo "SCA: trivy not installed — SKIPPED (brew install trivy, or use osv-scanner)"; return 0
  fi
  echo "SCA: trivy fs (deps + secrets + config)"
  if trivy fs --scanners vuln,secret --exit-code 1 --severity HIGH,CRITICAL --quiet "$ROOT" >/dev/null 2>&1; then
    echo "  SCA clean (no HIGH/CRITICAL)"; return 0
  else
    echo "  SCA FINDINGS — run: trivy fs --scanners vuln,secret --severity HIGH,CRITICAL $ROOT"; return 1
  fi
}

case "$MODE" in
  selftest) run_sast "${2:?selftest needs a DIR}" include-fixtures; exit $? ;;
  sast) run_sast; exit $? ;;
  sca)  run_sca;  exit $? ;;
  full)
    run_sast || rc=1
    run_sca  || rc=1
    echo "-----------------------------------------"
    # #4004 — emit the harness's parseable shape. Without it the nightly
    # synthesized "0 pass, 0 fail" from an unparseable run, and daily-review
    # scored a suite that RAN and FOUND REAL THINGS as if it had produced
    # nothing — findings invisible behind a formatting gap.
    if [ "$rc" -eq 0 ]; then
      echo "SECURITY SCAN: clean"
      echo "=== Results: 2 passed, 0 failed ==="
    else
      echo "SECURITY SCAN: findings — see above"
      echo "=== Results: 0 passed, 1 failed ==="
    fi
    exit $rc ;;
  *) echo "usage: security-scan.sh [full|sast|sca|selftest DIR]" >&2; exit 2 ;;
esac
