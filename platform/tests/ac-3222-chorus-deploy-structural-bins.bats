#!/usr/bin/env bats
# #3222 (Silas #4 / #3179 port) — chorus-deploy must install EVERY binary a crate
# emits, enumerated STRUCTURALLY from the crate's Cargo.toml `[[bin]]` entries, not
# from a hardcoded per-crate array (lib 527-542). The hardcoded map was the same
# drift class #3179 fixed in werk-deploy: a crate's new binary that nobody added to
# the array ships stale/absent behind a green deploy. Reading `[[bin]]` (falling back
# to the package name, exactly cargo's own rule) is drift-proof — one source of truth.
#
# Harness mirrors ac-3181-chorus-deploy-ff-canonical.bats: real git canonical + PATH
# stubs so the run reaches the install loop. chorus-bin-install stub logs each
# install_name so the test can assert the full set.

DEPLOY_SCRIPT="${DEPLOY_SCRIPT:-$BATS_TEST_DIRNAME/../scripts/chorus-deploy}"
gitc() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git "$@"; }

setup() {
  BASE=$(mktemp -d -t ac3222sb.XXXXXX)
  CANON="$BASE/canon"
  STUBDIR="$BASE/stubs"
  INSTALL_LOG="$BASE/install.log"
  mkdir -p "$STUBDIR"

  # Canonical: a real (already-in-sync) git repo so the #3181 ff-guard no-ops and we
  # fall straight through to the crate-resolution + install loop.
  git init -q -b main "$CANON"
  ( cd "$CANON"; echo a > f; gitc add -A; gitc commit -q -m c1 )

  # Fixture crate `chorus-inject` declaring TWO [[bin]] entries. The hardcoded array
  # only knows `chorus-inject` (one) → installs 1. Structural reads both → installs 2.
  local cd="$CANON/platform/services/chorus-inject"
  mkdir -p "$cd/target/release"
  cat > "$cd/Cargo.toml" <<'TOML'
[package]
name = "chorus-inject"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "chorus-inject"
path = "src/main.rs"

[[bin]]
name = "chorus-inject-probe"
path = "src/probe.rs"
TOML
  printf 'x' > "$cd/target/release/chorus-inject"
  printf 'y' > "$cd/target/release/chorus-inject-probe"

  # Stub build/deploy tools. chorus-bin-install logs the install_name (last arg) so
  # we can assert the full set of binaries that were installed.
  cat > "$STUBDIR/chorus-bin-install" <<EOF
#!/bin/bash
# args: [--target werk] <src> <install_name>
echo "\${@: -1}" >> "$INSTALL_LOG"
exit 0
EOF
  chmod +x "$STUBDIR/chorus-bin-install"
  # codesign must emit a CDHash line — chorus-deploy captures it under set -e
  # (VAR=$(... | grep '^CDHash=' ...)), so an empty stub would abort the script.
  printf '#!/bin/bash\necho "CDHash=TESTHASH"\nexit 0\n' > "$STUBDIR/codesign"
  chmod +x "$STUBDIR/codesign"
  for cmd in cargo npm launchctl build-signed.sh cards chorus-log rsync chorus-build; do
    printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/$cmd"
    chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  export CHORUS_ROOT="$CANON"
  # Test seam: chorus-deploy resolves the installer as ${CHORUS_BIN_INSTALL:-$SCRIPT_DIR/chorus-bin-install}.
  export CHORUS_BIN_INSTALL="$STUBDIR/chorus-bin-install"
}

teardown() { rm -rf "$BASE"; }

@test "structural: installs every [[bin]] the crate declares, not just the hardcoded one" {
  run "$DEPLOY_SCRIPT" chorus-inject
  grep -qx "chorus-inject" "$INSTALL_LOG"
  grep -qx "chorus-inject-probe" "$INSTALL_LOG"
}

@test "structural: a crate with no [[bin]] falls back to the package name (cargo's rule)" {
  local cd="$CANON/platform/services/chorus-inject"
  cat > "$cd/Cargo.toml" <<'TOML'
[package]
name = "chorus-inject"
version = "0.1.0"
TOML
  : > "$INSTALL_LOG"
  run "$DEPLOY_SCRIPT" chorus-inject
  grep -qx "chorus-inject" "$INSTALL_LOG"
}
