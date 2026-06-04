#!/usr/bin/env bats
# #3222 (Silas #5 / #3132) — on a CANONICAL deploy, chorus-deploy must verify the
# INSTALLED binary by CONTENT: its codesign cdhash must equal the BUILT binary's
# cdhash. Never trust the deploy self-report — a stale/corrupt install (rebuild
# didn't take, partial move) ships green otherwise. This is the #3132 hardening
# werk-deploy already had; the converge-on-chorus-deploy path must keep it so
# delegation doesn't regress prod-deploy safety. target=werk (isolated demo slot,
# no live service) is exempt, mirroring werk-deploy's canonical-only verify.

DEPLOY_SCRIPT="${DEPLOY_SCRIPT:-$BATS_TEST_DIRNAME/../scripts/chorus-deploy}"
gitc() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git "$@"; }

setup() {
  BASE=$(mktemp -d -t ac3222vg.XXXXXX)
  CANON="$BASE/canon"; STUBDIR="$BASE/stubs"
  mkdir -p "$STUBDIR"
  git init -q -b main "$CANON"
  ( cd "$CANON"; echo a > f; gitc add -A; gitc commit -q -m c1 )

  local cd="$CANON/platform/services/chorus-inject"
  mkdir -p "$cd/target/release"
  printf '[package]\nname = "chorus-inject"\nversion = "0.1.0"\n\n[[bin]]\nname = "chorus-inject"\npath = "src/main.rs"\n' > "$cd/Cargo.toml"
  printf 'x' > "$cd/target/release/chorus-inject"

  printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/chorus-bin-install"
  chmod +x "$STUBDIR/chorus-bin-install"
  # Path-aware codesign: the BUILT file (…/target/release/…) hashes to $CS_BUILT;
  # the INSTALLED file hashes to $CS_INST. Defaults differ → mismatch (the unhappy
  # path). A happy-path test sets CS_INST == CS_BUILT.
  cat > "$STUBDIR/codesign" <<'EOF'
#!/bin/bash
case "$*" in
  *target/release*) echo "CDHash=${CS_BUILT:-BUILTHASH}" ;;
  *)                echo "CDHash=${CS_INST:-INSTHASH}" ;;
esac
exit 0
EOF
  chmod +x "$STUBDIR/codesign"
  for cmd in cargo npm launchctl build-signed.sh cards chorus-log rsync chorus-build; do
    printf '#!/bin/bash\nexit 0\n' > "$STUBDIR/$cmd"; chmod +x "$STUBDIR/$cmd"
  done
  export PATH="$STUBDIR:$PATH"
  export DEPLOY_ROLE="silas"
  export CHORUS_ROOT="$CANON"
  export CHORUS_BIN_INSTALL="$STUBDIR/chorus-bin-install"
}

teardown() { rm -rf "$BASE"; }

@test "canonical: built != installed cdhash → VERIFY FAILED, exit 1" {
  run "$DEPLOY_SCRIPT" chorus-inject
  [ "$status" -eq 1 ]
  [[ "$output" == *"VERIFY FAILED"* ]]
}

@test "canonical: built == installed cdhash → verified, deploy proceeds" {
  CS_INST=BUILTHASH run "$DEPLOY_SCRIPT" chorus-inject
  [ "$status" -eq 0 ]
  [[ "$output" == *"verified chorus-inject"* ]]
}

@test "werk target: cdhash mismatch is NOT gated (isolated demo slot)" {
  export WERK_SILAS_BIN="$BASE/werkslot"; mkdir -p "$WERK_SILAS_BIN"
  run "$DEPLOY_SCRIPT" --target werk chorus-inject
  [ "$status" -eq 0 ]
  [[ "$output" != *"VERIFY FAILED"* ]]
}
