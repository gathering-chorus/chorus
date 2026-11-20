#!/usr/bin/env bats
# @test-type: unit — reads the repo's committed files; builds nothing.
# @domain: builds — every cargo build in a checkout uses one build folder (#4555)
#
# #4555 AC1 — the repo root is one Cargo workspace. Every build of a member writes
# to <checkout>/target/<account> (build-signed.sh, werk-build, werk-test) and every
# reader of a built binary asks built_binary() for that same place (werk-deploy,
# demo_env). A path made of a crate's own folder plus target/release is the old
# per-crate layout: the binary is not there any more, so the scan refuses it.
#
# Scanned: the verbs that build, test or deploy, and build-signed.sh. Comments are
# skipped. Coverage builds are cargo llvm-cov's own folder (<target>/llvm-cov-target).

ROOT="$BATS_TEST_DIRNAME/../.."
SCANNED=(
  platform/scripts/build-signed.sh
  platform/services/werk-build/src
  platform/services/werk-test/src
  platform/services/werk-deploy/src
)
# a crate folder joined to target/: "{}/{}/target/release", $crate_dir/target, rel).join("target
OLD_LAYOUT='services/[a-z-]+/target/release|\{\}/\{\}/target/release|\{\}/target/release/\{\}|crate_dir[^;]{0,20}/target|join\(\"target/release\"\)|working_dir, binary'

scan() {  # $1 = root to scan; prints offending lines
  local root="$1" p
  for p in "${SCANNED[@]}"; do
    [ -e "$root/$p" ] || { echo "MISSING $p"; continue; }
    grep -rnE "$OLD_LAYOUT" "$root/$p" --include='*.rs' --include='*.sh' 2>/dev/null \
      | grep -vE ':[0-9]+:\s*(//|#)' || true
  done
}

@test "4555 AC1: no build, test or deploy path uses a crate's own target folder" {
  out="$(scan "$ROOT")"
  [ -z "$out" ] || { echo "per-crate target paths:"; echo "$out"; return 1; }
}

@test "4555 AC1: build-signed.sh defaults to the checkout's folder for this account" {
  grep -qE 'CARGO_TARGET_DIR="\$\{CARGO_TARGET_DIR:-\$checkout/target/\$\(id -un\)\}"' "$ROOT/platform/scripts/build-signed.sh" || return 1
}

@test "4555 AC1: built_binary is <checkout>/target/<account>/release/<bin>" {
  src="$ROOT/platform/services/shared/target_seed.rs"
  grep -q 'pub fn built_binary(checkout: &str, account: &str, bin: &str)' "$src" || return 1
  grep -q 'format!("{}/release/{}", account_target_dir(checkout, account), bin)' "$src" || return 1
}

@test "4555 AC1 NEGATIVE PROOF: a planted per-crate path is refused" {
  tmp="$(mktemp -d)"
  for p in "${SCANNED[@]}"; do mkdir -p "$tmp/$(dirname "$p")"; cp -R "$ROOT/$p" "$tmp/$p"; done
  echo '    let built = format!("{}/{}/target/release/{}", werk_s, rel, bin);' >> "$tmp/platform/services/werk-deploy/src/lib.rs"
  out="$(scan "$tmp")"
  rm -rf "$tmp"
  [ -n "$out" ] || return 1
  echo "$out" | grep -q 'werk-deploy/src/lib.rs' || return 1
}

@test "4555 AC1 NEGATIVE PROOF: a missing scanned path fails, never passes empty" {
  tmp="$(mktemp -d)"
  out="$(scan "$tmp")"
  rm -rf "$tmp"
  echo "$out" | grep -q '^MISSING ' || return 1
}

# Coverage builds use other compiler flags: in the shared folder they would throw
# away every release and test build. The llvm-cov call must carry its own folder,
# <checkout>/target/<account>-cov.
cov_isolated() {  # $1 = all_scope.rs; ok when every llvm-cov call names the -cov folder
  local f="$1"
  grep -q '"llvm-cov"' "$f" || { echo "no llvm-cov call in $f"; return 1; }
  ! grep '"llvm-cov"' "$f" | grep -v 'CARGO_TARGET_DIR' | grep -q . || return 1
  grep -q 'format!("{}-cov", werk_test::account_target_dir(' "$f" || return 1
}

@test "4555 AC1: coverage builds write to their own <account>-cov folder" {
  cov_isolated "$ROOT/platform/services/werk-test/src/all_scope.rs" || return 1
}

@test "4555 AC1 NEGATIVE PROOF: an llvm-cov call without its own folder is refused" {
  tmp="$(mktemp -d)"
  sed 's/\.env("CARGO_TARGET_DIR", &cov)//' "$ROOT/platform/services/werk-test/src/all_scope.rs" > "$tmp/all_scope.rs"
  run cov_isolated "$tmp/all_scope.rs"
  rm -rf "$tmp"
  [ "$status" -ne 0 ] || return 1
}

@test "4555 AC1: clippy-ratchet builds in this account's folder, not the bare shared target/" {
  grep -q 'CARGO_TARGET_DIR": os.environ.get("CARGO_TARGET_DIR") or str(REPO_ROOT / "target" / getpass.getuser())' "$ROOT/platform/scripts/clippy-ratchet.py" || return 1
}
