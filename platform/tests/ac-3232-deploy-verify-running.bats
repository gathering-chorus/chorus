#!/usr/bin/env bats
# #3232 AC1 — a canonical deploy of a DAEMON crate must verify the service actually
# came UP after kickstart (launchctl reports running + its socket is live), retrying a
# bounded number of times, and FAIL LOUD (exit 1, deploy.failed reason=not-running) if it
# never comes up. Proven need 2026-06-04: `chorus-deploy chorus-hooks` kickstarted, printed
# success, and left the daemon DOWN (socket gone, all guards offline team-wide) — green
# while down. chorus-api already smokes itself before reporting success; the rust-daemon
# path didn't. This closes that asymmetry. werk-build/chorus-api paths are unaffected.
#
# Harness mirrors ac-3222-chorus-deploy-verify-gate.bats: real git canonical + PATH stubs.
# launchctl `print` is marker-driven so the test controls whether the daemon "is running",
# and a socket path override (CHORUS_HOOKS_SOCK) lets the test toggle socket liveness.

DEPLOY_SCRIPT="${DEPLOY_SCRIPT:-$BATS_TEST_DIRNAME/../scripts/chorus-deploy}"
gitc() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git "$@"; }

setup() {
  BASE=$(mktemp -d -t ac3232vr.XXXXXX)
  CANON="$BASE/canon"; STUBDIR="$BASE/stubs"
  mkdir -p "$STUBDIR"
  git init -q -b main "$CANON"
  ( cd "$CANON"; echo a > f; gitc add -A; gitc commit -q -m c1 )

  # chorus-hooks crate (KICKSTART_SERVICE=com.chorus.hooks in chorus-deploy's case).
  local cd="$CANON/platform/services/chorus-hooks"
  mkdir -p "$cd/target/release"
  printf '[package]\nname = "chorus-hooks"\nversion = "0.1.0"\n\n[[bin]]\nname = "chorus-hooks"\npath = "src/main.rs"\n\n[[bin]]\nname = "chorus-hook-shim"\npath = "src/shim.rs"\n' > "$cd/Cargo.toml"
  printf 'd' > "$cd/target/release/chorus-hooks"
  printf 's' > "$cd/target/release/chorus-hook-shim"

  printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/chorus-bin-install"; chmod +x "$STUBDIR/chorus-bin-install"
  # codesign: same cdhash for built + installed so the #3222 verify-gate passes and we
  # reach the post-kickstart verify-running step under test.
  printf '#!/bin/bash\necho "CDHash=SAMEHASH"\nexit 0\n' > "$STUBDIR/codesign"; chmod +x "$STUBDIR/codesign"
  # launchctl: kickstart always "succeeds"; `print` reports running ONLY if $UP_MARKER exists.
  cat > "$STUBDIR/launchctl" <<EOF
#!/bin/bash
case "\$1" in
  kickstart) exit 0 ;;
  print) if [ -f "$BASE/up.marker" ]; then echo "state = running"; echo "pid = 4242"; exit 0; else echo "state = not running"; exit 0; fi ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$STUBDIR/launchctl"
  for cmd in cargo npm build-signed.sh cards chorus-log rsync chorus-build; do
    printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/$cmd"; chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  export CHORUS_ROOT="$CANON"
  export CHORUS_BIN_INSTALL="$STUBDIR/chorus-bin-install"
  # bound the retry wait so the failing path doesn't hang the test
  export CHORUS_DEPLOY_VERIFY_RETRIES=3
  export CHORUS_DEPLOY_VERIFY_DELAY=0
  export CHORUS_HOOKS_SOCK="$BASE/hooks.sock"
}
teardown() { rm -rf "$BASE"; }

@test "daemon never comes up after kickstart → retries then FAILS LOUD (exit 1, not-running)" {
  # no up.marker, no socket → launchctl print says not running
  run "$DEPLOY_SCRIPT" chorus-hooks
  [ "$status" -eq 1 ]
  [[ "$output" == *"not running"* || "$output" == *"did not come up"* ]]
}

@test "daemon comes up (running + socket live) → deploy succeeds" {
  : > "$BASE/up.marker"          # launchctl print → running
  : > "$BASE/hooks.sock"          # socket present
  run "$DEPLOY_SCRIPT" chorus-hooks
  [ "$status" -eq 0 ]
  [[ "$output" == *"deployed"* ]]
}
