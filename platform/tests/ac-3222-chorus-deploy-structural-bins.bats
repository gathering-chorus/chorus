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

# #3250 — the LIVE BREAK: the werk-accept crate declares ONE [[bin]] (werk-accept) but
# carries werk-do-more.rs + werk-finalize.rs in src/bin/. cargo builds all three (its
# SECOND default-binary rule: src/bin/*.rs autobins); the old crate_binaries() read only
# [[bin]] and installed one → the two verbs went absent from ~/.chorus/bin, breaking the
# next card's finalize step. The enumeration must be the UNION of [[bin]] AND src/bin/*.rs.
@test "structural: installs src/bin/*.rs autobins alongside the [[bin]] entry (the #3250 break)" {
  local cd="$CANON/platform/services/chorus-inject"
  cat > "$cd/Cargo.toml" <<'TOML'
[package]
name = "chorus-inject"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "chorus-inject"
path = "src/main.rs"
TOML
  mkdir -p "$cd/src/bin"
  printf 'fn main(){}' > "$cd/src/bin/chorus-inject-do-more.rs"
  printf 'fn main(){}' > "$cd/src/bin/chorus-inject-finalize.rs"
  printf 'm' > "$cd/target/release/chorus-inject-do-more"
  printf 'f' > "$cd/target/release/chorus-inject-finalize"
  : > "$INSTALL_LOG"
  run "$DEPLOY_SCRIPT" chorus-inject
  grep -qx "chorus-inject" "$INSTALL_LOG"
  grep -qx "chorus-inject-do-more" "$INSTALL_LOG"
  grep -qx "chorus-inject-finalize" "$INSTALL_LOG"
}

# A crate that ONLY uses src/bin/*.rs (no [[bin]], no src/main.rs default) — pure autobins.
@test "structural: a crate with only src/bin/*.rs installs each autobin" {
  local cd="$CANON/platform/services/chorus-inject"
  cat > "$cd/Cargo.toml" <<'TOML'
[package]
name = "chorus-inject"
version = "0.1.0"
edition = "2021"
TOML
  rm -f "$cd/target/release/chorus-inject"
  mkdir -p "$cd/src/bin"
  printf 'fn main(){}' > "$cd/src/bin/alpha.rs"
  printf 'fn main(){}' > "$cd/src/bin/beta.rs"
  printf 'a' > "$cd/target/release/alpha"
  printf 'b' > "$cd/target/release/beta"
  : > "$INSTALL_LOG"
  run "$DEPLOY_SCRIPT" chorus-inject
  grep -qx "alpha" "$INSTALL_LOG"
  grep -qx "beta" "$INSTALL_LOG"
}
