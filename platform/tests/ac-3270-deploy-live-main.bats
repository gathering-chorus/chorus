#!/usr/bin/env bats
# #3270 — a canonical deploy's `deploy.completed` spine event must carry a `live_main`
# field stating whether what's now live IS origin/main. Today the event proves the binary
# is installed (#3222 cdhash verify) and running (#3232 verify-running), but never states
# the merged≠live truth in the event itself — so #3269's flow-report has to INFER it. This
# closes that: the deploy emits live_main=true|false as ground truth, killing the silent
# false-green where a "completed" deploy doesn't actually correspond to main.
#
# Harness mirrors ac-3232-deploy-verify-running.bats: real git canonical + PATH stubs.
# chorus-log is stubbed to CAPTURE argv so we can assert the emitted deploy.completed payload.

DEPLOY_SCRIPT="${DEPLOY_SCRIPT:-$BATS_TEST_DIRNAME/../scripts/chorus-deploy}"
gitc() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git "$@"; }

setup() {
  BASE=$(mktemp -d -t ac3270lm.XXXXXX)
  CANON="$BASE/canon"; STUBDIR="$BASE/stubs"
  mkdir -p "$STUBDIR"
  git init -q -b main "$CANON"
  ( cd "$CANON"; echo a > f; gitc add -A; gitc commit -q -m c1 )
  # an origin/main ref so the live_main comparison has something to read
  ( cd "$CANON"; gitc update-ref refs/remotes/origin/main HEAD )

  local cd="$CANON/platform/services/chorus-hooks"
  mkdir -p "$cd/target/release"
  printf '[package]\nname = "chorus-hooks"\nversion = "0.1.0"\n\n[[bin]]\nname = "chorus-hooks"\npath = "src/main.rs"\n\n[[bin]]\nname = "chorus-hook-shim"\npath = "src/shim.rs"\n' > "$cd/Cargo.toml"
  printf 'd' > "$cd/target/release/chorus-hooks"
  printf 's' > "$cd/target/release/chorus-hook-shim"

  printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/chorus-bin-install"; chmod +x "$STUBDIR/chorus-bin-install"
  # same cdhash for built + installed so the #3222 verify-gate passes and we reach the emit
  printf '#!/bin/bash\necho "CDHash=SAMEHASH"\nexit 0\n' > "$STUBDIR/codesign"; chmod +x "$STUBDIR/codesign"
  cat > "$STUBDIR/launchctl" <<EOF
#!/bin/bash
case "\$1" in
  kickstart) exit 0 ;;
  print) echo "state = running"; echo "pid = 4242"; exit 0 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$STUBDIR/launchctl"
  # chorus-log: CAPTURE argv (this is what we assert against)
  CLOG="$BASE/chorus-log.argv"
  cat > "$STUBDIR/chorus-log" <<EOF
#!/bin/bash
echo "\$@" >> "$CLOG"
exit 0
EOF
  chmod +x "$STUBDIR/chorus-log"
  for cmd in cargo npm build-signed.sh cards rsync chorus-build; do
    printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/$cmd"; chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  export CHORUS_ROOT="$CANON"
  export CHORUS_LOG_BIN="$STUBDIR/chorus-log"
  export CHORUS_LOG="$STUBDIR/chorus-log"
  export CHORUS_BIN_INSTALL="$STUBDIR/chorus-bin-install"
  export CHORUS_DEPLOY_VERIFY_RETRIES=3
  export CHORUS_DEPLOY_VERIFY_DELAY=0
  export CHORUS_HOOKS_SOCK="$BASE/hooks.sock"
  : > "$BASE/hooks.sock"   # socket live so verify-running passes
}
teardown() { rm -rf "$BASE"; }

@test "canonical deploy.completed carries live_main=true when HEAD == origin/main" {
  run "$DEPLOY_SCRIPT" chorus-hooks
  [ "$status" -eq 0 ]
  grep -q "deploy.completed" "$BASE/chorus-log.argv" || { echo "no deploy.completed emitted: $(cat "$BASE/chorus-log.argv")"; false; }
  grep "deploy.completed" "$BASE/chorus-log.argv" | grep -q "live_main=true" \
    || { echo "deploy.completed missing live_main=true: $(grep deploy.completed "$BASE/chorus-log.argv")"; false; }
}

@test "deploy.completed carries the #3270 ground-truth envelope (verb/step/outcome/cdhash/artifact_class)" {
  run "$DEPLOY_SCRIPT" chorus-hooks
  [ "$status" -eq 0 ]
  line=$(grep "deploy.completed" "$BASE/chorus-log.argv")
  for f in "verb=deploy" "step=deploy" "outcome=success" "cdhash=SAMEHASH" "artifact_class=rust-daemon" "live_main="; do
    echo "$line" | grep -q "$f" || { echo "deploy.completed missing $f: $line"; false; }
  done
}

@test "canonical deploy.completed carries live_main=false when HEAD is ahead of origin/main" {
  # HEAD ahead of origin/main (deployed code is NOT what main has) → the merged≠live case.
  ( cd "$CANON"; echo b >> f; gitc commit -q -am c2 )   # HEAD now ahead of refs/remotes/origin/main
  run "$DEPLOY_SCRIPT" chorus-hooks
  [ "$status" -eq 0 ]
  grep "deploy.completed" "$BASE/chorus-log.argv" | grep -q "live_main=false" \
    || { echo "expected live_main=false (HEAD ahead of main): $(grep deploy.completed "$BASE/chorus-log.argv")"; false; }
}
