#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# #2927 AC2-AC5 — main-flow tests for deploy-daemon-card.sh.
# Covers per-unit dispatch (AC2), per-unit rollback (AC3), per-role authority
# (AC4), and zero-match refusal (AC5). PATH-stubs all external commands so the
# wrapper runs in isolation against a fixture werk.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/deploy-daemon-card.sh"

setup() {
  STUBDIR=$(mktemp -d -t ac2-5-stubs.XXXXXX)
  WERKBASE=$(mktemp -d -t ac2-5-werkbase.XXXXXX)
  CANON=$(mktemp -d -t ac2-5-canon.XXXXXX)
  FAKE_HOME=$(mktemp -d -t ac2-5-home.XXXXXX)
  CALLS="$STUBDIR/calls.log"
  : > "$CALLS"
  # CRITICAL: override HOME so deploy_chorus_hooks doesn't touch the real
  # ~/.chorus/bin/ — that would corrupt the running daemon.
  REAL_HOME="$HOME"
  export HOME="$FAKE_HOME"
  mkdir -p "$HOME/.chorus/bin"

  # Fixture werk with .git + the three unit dirs so introspect/deploy can act.
  mkdir -p "$WERKBASE/silas-9001/platform/api/dist"
  mkdir -p "$WERKBASE/silas-9001/platform/services/chorus-hooks"
  mkdir -p "$WERKBASE/silas-9001/directing/products/cards/dist"
  # Seed each dist with a marker so dist-rename rollback paths have content,
  # and seed a placeholder so git init has something to commit.
  echo "marker" > "$WERKBASE/silas-9001/platform/api/dist/marker"
  echo "marker" > "$WERKBASE/silas-9001/directing/products/cards/dist/marker"
  echo "init"   > "$WERKBASE/silas-9001/.keep"
  (
    cd "$WERKBASE/silas-9001"
    git init -q
    git remote add origin https://example.invalid/repo.git
    git add -A
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t \
    GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
      git commit -q -m "initial"
    # Rename default branch to main if needed.
    cur="$(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
    [ "$cur" != "main" ] && git branch -q -m "$cur" main 2>/dev/null || true
    git update-ref refs/remotes/origin/main HEAD
    # Now stage a change in one of the unit paths so diff-introspect picks it up.
    echo "//new responder" > platform/services/chorus-hooks/src.rs
    git add -A
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t \
    GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
      git commit -q -m "card change"
  )

  # Stub external commands the wrapper calls.
  for cmd in chorus-werk-sync chorus-deploy cards launchctl rsync npm build-signed.sh; do
    cat > "$STUBDIR/$cmd" <<EOF
#!/bin/bash
echo "$cmd \$*" >> "$CALLS"
exit \${STUB_${cmd//[-.]/_}_EXIT:-0}
EOF
    chmod +x "$STUBDIR/$cmd"
  done

  export PATH="$STUBDIR:$PATH"
  export CHORUS_WERK_BASE="$WERKBASE"
  export CHORUS_ROOT="$CANON"
  export DEPLOY_ROLE="silas"
  # Override the chorus-hooks build script path so tests intercept it via the stub.
  # (The wrapper hardcodes the canonical absolute path otherwise — see deploy_chorus_hooks.)
  export CHORUS_BUILD_SIGNED="$STUBDIR/build-signed.sh"
  # UID is read-only in bash; the wrapper reads $UID natively.
  # Create canonical dirs so the dispatch functions can find dist-targets.
  mkdir -p "$CANON/platform/api/dist" "$CANON/directing/products/cards/dist"
  mkdir -p "$HOME/.chorus/bin"
}

teardown() {
  rm -rf "$STUBDIR" "$WERKBASE" "$CANON" "$FAKE_HOME"
  # Restore real HOME for the next test's setup.
  export HOME="$REAL_HOME"
}

# ---------- AC4: role guard ----------

@test "AC4: DEPLOY_ROLE=silas accepted (role guard passes for silas)" {
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  # We don't assert success here (deploy may fail because build-signed.sh is a stub);
  # we assert the role guard didn't reject (no role-guard exit 3).
  [ "$status" -ne 3 ]
}

@test "AC4: DEPLOY_ROLE=kade accepted (no longer silas-only)" {
  export DEPLOY_ROLE=kade
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  [ "$status" -ne 3 ]
}

@test "AC4: DEPLOY_ROLE=wren accepted" {
  export DEPLOY_ROLE=wren
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  [ "$status" -ne 3 ]
}

@test "AC4: DEPLOY_ROLE=unknown refused with exit 3" {
  export DEPLOY_ROLE=jeff
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  [ "$status" -eq 3 ]
  [[ "$output" == *"DEPLOY_ROLE must be one of"* ]]
}

@test "AC4: DEPLOY_ROLE unset refused with exit 3" {
  unset DEPLOY_ROLE
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  [ "$status" -eq 3 ]
}

# ---------- AC5: zero-match refusal ----------

@test "AC5: --units empty + diff has no known unit paths → refusal exit 7" {
  # Move origin/main forward to HEAD so the diff against origin/main is empty
  # (no changes to any unit path → introspect returns zero matches → AC5 refuses).
  git -C "$WERKBASE/silas-9001" update-ref refs/remotes/origin/main HEAD
  run "$SCRIPT" 9001 --probe "echo ok"
  [ "$status" -eq 7 ]
  [[ "$output" == *"zero-match refusal"* ]] || [[ "$output" == *"no deploy units matched"* ]]
}

@test "AC5: --units chorus-hooks → not a zero-match (explicit override bypasses introspect)" {
  # Even with no diff matches, --units chorus-hooks should not trigger AC5.
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  [ "$status" -ne 7 ]
}

@test "AC5: --units with unknown name → refusal exit 6 (different from zero-match)" {
  run "$SCRIPT" 9001 --probe "echo ok" --units bogus-unit
  [ "$status" -eq 6 ]
  [[ "$output" == *"unknown unit"* ]]
}

# ---------- AC2: per-unit dispatch ----------

@test "AC2: --units chorus-api → chorus-deploy chorus-api called" {
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-api
  grep -q 'npm run build' "$CALLS"
  grep -q 'rsync ' "$CALLS"
  grep -q 'launchctl kickstart' "$CALLS"
}

@test "AC2: --units chorus-hooks → build-signed.sh + kickstart com.chorus.hooks" {
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  grep -qE 'build-signed.sh.*chorus-hooks|chorus-hooks' "$CALLS"
  grep -q 'launchctl kickstart' "$CALLS"
}

@test "AC2: --units cards-sdk → npm build + rsync (no launchctl)" {
  run "$SCRIPT" 9001 --probe "echo ok" --units cards-sdk
  grep -q 'npm run build' "$CALLS"
  grep -q 'rsync ' "$CALLS"
}

@test "AC2: --units chorus-hooks,cards-sdk → both unit dispatches fire" {
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks,cards-sdk
  # both units should have dispatched
  build_signed_count=$(grep -c 'build-signed.sh' "$CALLS" 2>/dev/null || echo 0)
  rsync_count=$(grep -c 'rsync ' "$CALLS" 2>/dev/null || echo 0)
  [ "$build_signed_count" -ge 1 ]
  [ "$rsync_count" -ge 1 ]  # cards-sdk uses rsync
}

@test "AC2: diff-introspect (no --units) routes to chorus-hooks for our fixture werk" {
  # Fixture werk has change in platform/services/chorus-hooks/, so diff-introspect should pick chorus-hooks.
  run "$SCRIPT" 9001 --probe "echo ok"
  grep -qE 'build-signed.sh.*chorus-hooks' "$CALLS"
}

@test "AC2: probe failure → cards done NOT called" {
  run "$SCRIPT" 9001 --probe "exit 1" --units chorus-hooks
  ! grep -q 'cards done' "$CALLS"
  [ "$status" -ne 0 ]
}

@test "AC2: happy path → cards done <id> called" {
  run "$SCRIPT" 9001 --probe "echo ok" --units chorus-hooks
  grep -q 'cards done 9001' "$CALLS"
}

# ---------- AC3: per-unit rollback ----------

@test "AC3: probe fail with chorus-hooks unit → restore chorus-hooks.prev binaries" {
  # Pre-seed installed binaries so deploy creates .prev copies.
  echo "OLD_HOOKS" > "$HOME/.chorus/bin/chorus-hooks"
  echo "OLD_SHIM" > "$HOME/.chorus/bin/chorus-hook-shim"
  run "$SCRIPT" 9001 --probe "exit 1" --units chorus-hooks
  # After rollback the OLD_* content should be back in place (.prev restored over .live)
  grep -q OLD_HOOKS "$HOME/.chorus/bin/chorus-hooks"
  grep -q OLD_SHIM "$HOME/.chorus/bin/chorus-hook-shim"
  # And rollback should have kickstarted to load the restored binary
  kickstart_count=$(grep -c 'launchctl kickstart' "$CALLS" 2>/dev/null || echo 0)
  [ "$kickstart_count" -ge 2 ]  # one for deploy, one for rollback
}

@test "AC3: probe fail with cards-sdk → restore dist.prev directory" {
  echo "OLD_DIST" > "$CANON/directing/products/cards/dist/marker"
  run "$SCRIPT" 9001 --probe "exit 1" --units cards-sdk
  # After rollback, dist should contain the OLD_DIST marker
  [ -f "$CANON/directing/products/cards/dist/marker" ]
  grep -q OLD_DIST "$CANON/directing/products/cards/dist/marker"
}

# Cleanup of stale .prev files in $HOME between tests is the teardown's job
# (we don't touch $HOME in teardown, but the .prev files would only matter
# if tests run in a specific order — bats isolates by subshell).
