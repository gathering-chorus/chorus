#!/usr/bin/env bats
# @test-type: integration:security — runs real trivy over seeded lockfile fixtures; no live service.
# @domain: security — the product domain this suite guards (#4334)
# #4034 — the nightly SCA scan is SCOPED (skip target/, node_modules/, .git)
# because the unscoped 17GB walk ran 72+ min under contention and blew the
# lane cap (2026-08-30). Negative proofs (#3734), both directions:
#   1. the scoped flags still FAIL on a planted HIGH CVE — faster, not blinder;
#   2. the same CVE hidden inside a target/ dir is invisible scoped and
#      VISIBLE deep — the skip provably skips, and the weekly deep covers it.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/test-security-scan.sh"
# Covers: platform/scripts/test-security-scan.sh  (literal path for the #3917 linker)
FIX="${BATS_TEST_DIRNAME}/../security/sca-fixtures/vulnerable"

setup() {
  command -v trivy >/dev/null 2>&1 || skip "trivy not installed"
  # The artifact-only tree is BUILT here, not committed: .gitignore ignores
  # target/ everywhere (the reviewers' catch — a committed copy silently
  # vanishes from the diff), and a test brings its own world (#3528) anyway.
  ART="$BATS_TEST_TMPDIR/artifact-only"
  mkdir -p "$ART/target/nested"
  cp "$FIX/package-lock.json" "$ART/target/nested/package-lock.json"
}

@test "NEGATIVE PROOF: scoped scan still fails on the planted HIGH CVE (lodash 4.17.15)" {
  run bash "$SCRIPT" sca-selftest "$FIX"
  # #4034 reopen 1 — exit 1 is a finding; 2 is unmeasured and must not pass here
  [ "$status" -eq 1 ] || { echo "want 1 (finding), got $status: $output"; return 1; }
  [[ "$output" == *"lodash"* ]] || return 1
}

@test "the skip provably skips: the same lockfile under target/ is invisible scoped, visible deep" {
  run bash "$SCRIPT" sca-selftest "$ART"
  local scoped="$status"
  run bash "$SCRIPT" sca-selftest-deep "$ART"
  local deep="$status"
  [ "$scoped" -eq 0 ]
  [ "$deep" -ne 0 ]
}

# #4034 reopen 1 — the runtime log dirs (ops/logs 1.3 GB, git-ignored) are
# skipped scoped and still walked deep, the same proof as target/ above.
@test "#4034 reopen 1: a lockfile under ops/logs is invisible scoped, visible deep" {
  LOGS="$BATS_TEST_TMPDIR/logs-only"
  mkdir -p "$LOGS/ops/logs"
  cp "$FIX/package-lock.json" "$LOGS/ops/logs/package-lock.json"
  run bash "$SCRIPT" sca-selftest "$LOGS"
  local scoped="$status"
  run bash "$SCRIPT" sca-selftest-deep "$LOGS"
  [ "$scoped" -eq 0 ]
  [ "$status" -eq 1 ]
}

# #4034 reopen 1 — 2026-10-15 05:19: trivy hit its own 4-minute limit and the
# suite counted the timeout as a finding. NEGATIVE PROOF of the two states: a
# trivy that dies FATAL reads UNMEASURED (exit 2, no "SCA FINDINGS"), and the
# planted HIGH above still reads exit 1.
@test "NEGATIVE PROOF #4034: a trivy that never finishes is UNMEASURED, not a finding" {
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  cat > "$BATS_TEST_TMPDIR/bin/trivy" <<'EOS'
#!/bin/bash
echo "2026-10-15T05:24:39-04:00	FATAL	Fatal error	run error: fs scan error: semaphore acquire: context deadline exceeded" >&2
exit 1
EOS
  chmod +x "$BATS_TEST_TMPDIR/bin/trivy"
  PATH="$BATS_TEST_TMPDIR/bin:$PATH" run bash "$SCRIPT" sca-selftest "$FIX"
  [ "$status" -eq 2 ] || { echo "want 2 (unmeasured), got $status: $output"; return 1; }
  [[ "$output" == *"SCA UNMEASURED"* ]] || return 1
  [[ "$output" == *"context deadline exceeded"* ]] || return 1
  [[ "$output" != *"SCA FINDINGS"* ]] || return 1
}

# #4034 reopen 1 — in a git repo the scoped scan reads the COMMITTED tree (git
# archive HEAD), not the working tree. NEGATIVE PROOF both ways: a vulnerable
# lockfile that is committed is still caught; the same file left untracked is
# not walked scoped and is still caught deep.
@test "NEGATIVE PROOF #4034: in a repo, a committed HIGH is caught; an untracked one waits for the deep scan" {
  REPO="$BATS_TEST_TMPDIR/repo"
  mkdir -p "$REPO/app" "$REPO/stray"
  git -C "$REPO" init -q
  echo '{}' > "$REPO/README.json"
  git -C "$REPO" add README.json
  git -C "$REPO" -c user.email=t@t -c user.name=t commit -qm base
  cp "$FIX/package-lock.json" "$REPO/stray/package-lock.json"
  run bash "$SCRIPT" sca-selftest "$REPO"
  [ "$status" -eq 0 ] || { echo "untracked lockfile must not be walked scoped, got $status: $output"; return 1; }
  [[ "$output" == *"scanning the committed tree"* ]] || return 1
  run bash "$SCRIPT" sca-selftest-deep "$REPO"
  [ "$status" -eq 1 ] || { echo "deep must still catch it, got $status"; return 1; }
  cp "$FIX/package-lock.json" "$REPO/app/package-lock.json"
  git -C "$REPO" add app/package-lock.json
  git -C "$REPO" -c user.email=t@t -c user.name=t commit -qm vuln
  run bash "$SCRIPT" sca-selftest "$REPO"
  [ "$status" -eq 1 ] || { echo "committed lockfile must be caught scoped, got $status: $output"; return 1; }
  [[ "$output" == *"lodash"* ]] || return 1
}
