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
# #4107 — the TARGET is resolved the same way the ruleset is: the tree this
# script lives in wins. CHORUS_ROOT is the LAST resort, not the first, because
# in a werk shell it points at canonical — which meant a werk scanned canonical
# and the gate could neither go red nor green on what the card actually changed.
# The bats copy (see RULES below) is why CHORUS_ROOT stays in the ladder at all.
ROOT=""
for cand in "${BATS_TEST_DIRNAME:-}/../.." "$SELF_DIR/../.." "${CHORUS_ROOT:-}"; do
  [ -n "$cand" ] && [ -d "$cand/platform" ] && ROOT="$(cd "$cand" && pwd)" && break
done
[ -n "$ROOT" ] || { echo "test-security-scan: cannot resolve repo root (tried BATS_TEST_DIRNAME, script dir, \$CHORUS_ROOT)" >&2; exit 2; }
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
    # #4004 — PRINT the findings. This said only "run this command yourself",
    # so the security lane went red every day for a week naming nothing, and
    # nobody could tell a new finding from the same three. `$out` already holds
    # semgrep's report; a red that cannot say what it found is not actionable.
    echo "  SAST FINDINGS:"
    printf '%s\n' "$out" | sed 's/^/    /'
    echo "  reproduce: semgrep --config $RULES $target"
    return 1
  fi
}

# #4034 — the scoped flag set. The unscoped walk covered 17GB (12GB one
# target/ dir, 1GB .git, 10 node_modules trees); under nightly contention that
# I/O ran 72+ minutes and blew the lane cap (2026-08-30). npm/cargo CVEs come
# from lockfiles at package ROOTS, so skipping build artifacts loses no
# dependency coverage; the WEEKLY deep scan (mode `deep`) still walks
# everything so secrets in artifacts are found on cadence, not never.
SCA_SCOPE=(--skip-dirs "**/target" --skip-dirs "**/node_modules" --skip-dirs ".git" --skip-dirs "platform/security/sca-fixtures")

# #4107 — trivy walks the WORKING TREE, so it reads git-ignored local files.
# `.env.*` is git-ignored repo-wide (.gitignore), so those secrets are real but
# never ship; counting them made the gate red every run for something no commit
# can carry. Skipping them would be a blind spot on its own, so `assert_no_tracked_env`
# below fails loud if one is ever actually tracked — the coverage moves, it does
# not disappear.
SCA_SCOPE+=(--skip-files "**/.env.*" --skip-files ".env.*")

# The secret-scan exemption ledger: every entry names why it is not a credential.
SCA_IGNOREFILE="$ROOT/platform/security/trivy-ignore.yaml"

assert_no_tracked_env() {
  local tracked
  tracked=$(git -C "$ROOT" ls-files -- '.env' '.env.*' '**/.env' '**/.env.*' 2>/dev/null)
  if [ -n "$tracked" ]; then
    echo "  SECURITY: env file(s) TRACKED in git — the skip-files rule above is hiding them:" >&2
    printf '%s\n' "$tracked" | sed 's/^/    /' >&2
    return 1
  fi
  return 0
}

sca_flags() {
  # Cached DB: skip the update when the local DB is <24h old — the nightly
  # must never pay a download, and a stale-DB skip self-heals next day.
  local meta="$HOME/Library/Caches/trivy/db/metadata.json"
  local out=""
  if [ -f "$meta" ] && [ -n "$(find "$meta" -mtime -1 2>/dev/null)" ]; then
    out="--skip-db-update"
  fi
  if [ -f "${SCA_IGNOREFILE:-}" ]; then
    out="$out --ignorefile ${SCA_IGNOREFILE}"
  fi
  echo "$out"
}

run_sca() {
  if ! command -v trivy >/dev/null 2>&1; then
    echo "SCA: trivy not installed — SKIPPED (brew install trivy, or use osv-scanner)"; return 0
  fi
  local target="${1:-$ROOT}" depth="${2:-scoped}"
  local scope=()
  if [ "$depth" = "scoped" ]; then
    scope=("${SCA_SCOPE[@]}")
    # the skip-files rule above only holds because nothing matching it is tracked
    assert_no_tracked_env || return 1
  fi
  echo "SCA: trivy fs (deps + secrets + config) — $depth"
  # #4107 — trivy reads ~/.docker/config.json before it scans, and ours names a
  # credsStore. With Docker Desktop stopped, the helper it spawns
  # (docker-credential-desktop get) blocks forever on a socket that will never
  # answer, and trivy's own --timeout does not cover a hung child. Measured
  # 2026-09-04 13:13: 13 minutes at 0.0% CPU, the whole test stage wedged behind
  # it, for a filesystem scan that needs no registry login at all.
  #
  # Point DOCKER_CONFIG at an empty config for the scan: no credsStore, no
  # helper, no hang. The explicit --timeout is the second line of defence.
  local _sca_dockercfg
  _sca_dockercfg="$(mktemp -d)"
  printf '{}' > "$_sca_dockercfg/config.json"
  # #4004 — capture instead of discarding to /dev/null, so a red names its CVEs.
  # NOTE bash 3.2 + set -u: never expand an empty array unguarded.
  local sca_out sca_json
  if [ ${#scope[@]} -gt 0 ]; then
    sca_out=$(DOCKER_CONFIG="$_sca_dockercfg" trivy fs --timeout 4m --scanners vuln,secret --exit-code 1 --severity HIGH,CRITICAL --quiet $(sca_flags) "${scope[@]}" "$target" 2>&1)
  else
    sca_out=$(DOCKER_CONFIG="$_sca_dockercfg" trivy fs --timeout 4m --scanners vuln,secret --exit-code 1 --severity HIGH,CRITICAL --quiet $(sca_flags) "$target" 2>&1)
  fi
  local sca_rc=$?
  rm -rf "$_sca_dockercfg"
  if [ $sca_rc -eq 0 ]; then
    echo "  SCA clean (no HIGH/CRITICAL)"; return 0
  else
    # #4107 — #4004 captured the output so a red could name its findings, but
    # trivy's table opens with one Report Summary row PER lockfile (40+ here) and
    # the head -60 cut the report before the detail. Measured on a planted-token
    # fixture: exit 1, 63 lines, and not one of them named the secret. So print
    # the findings from JSON, which is short and always names target + rule + line,
    # and keep the human table only as the tail nobody has to read.
    echo "  SCA FINDINGS:"
    if [ ${#scope[@]} -gt 0 ]; then
      sca_json=$(DOCKER_CONFIG="$_sca_dockercfg" trivy fs --timeout 4m --scanners vuln,secret --severity HIGH,CRITICAL --quiet --format json $(sca_flags) "${scope[@]}" "$target" 2>/dev/null)
    else
      sca_json=$(DOCKER_CONFIG="$_sca_dockercfg" trivy fs --timeout 4m --scanners vuln,secret --severity HIGH,CRITICAL --quiet --format json $(sca_flags) "$target" 2>/dev/null)
    fi
    printf '%s' "$sca_json" | python3 -c '
import json,sys
try: d=json.load(sys.stdin)
except Exception: print("    (could not parse trivy json — raw table follows)"); sys.exit(0)
n=0
for r in d.get("Results") or []:
    for v in r.get("Vulnerabilities") or []:
        n+=1; print("    VULN   %s  %s %s -> %s  [%s]" % (r["Target"], v.get("PkgName"), v.get("InstalledVersion"), v.get("FixedVersion") or "no fix", v.get("VulnerabilityID")))
    for s in r.get("Secrets") or []:
        n+=1; print("    SECRET %s:%s  %s" % (r["Target"], s.get("StartLine"), s.get("RuleID")))
print("    total: %d finding(s)" % n)
'
    echo "  reproduce: trivy fs --scanners vuln,secret --severity HIGH,CRITICAL $target"
    return 1
  fi
}

case "$MODE" in
  selftest) run_sast "${2:?selftest needs a DIR}" include-fixtures; exit $? ;;
  # #4034 — the negative-proof seams: run the SAME scoped/deep flag sets
  # against a caller-supplied dir, so a fixture can prove the scope still
  # catches a planted HIGH (not blinder) and that skip-dirs really skips.
  sca-selftest)      run_sca "${2:?sca-selftest needs a DIR}" scoped; exit $? ;;
  sca-selftest-deep) run_sca "${2:?sca-selftest-deep needs a DIR}" deep; exit $? ;;
  sca)  run_sca;  exit $? ;;
  deep)
    run_sast || rc=1
    run_sca "$ROOT" deep || rc=1
    echo "-----------------------------------------"
    if [ "$rc" -eq 0 ]; then echo "SECURITY SCAN (deep): clean"; echo "=== Results: 2 passed, 0 failed ==="
    else echo "SECURITY SCAN (deep): findings — see above"; echo "=== Results: 0 passed, 1 failed ==="; fi
    exit $rc ;;
  sast) run_sast; exit $? ;;
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
  *) echo "usage: security-scan.sh [full|deep|sast|sca|selftest DIR|sca-selftest DIR|sca-selftest-deep DIR]" >&2; exit 2 ;;
esac
