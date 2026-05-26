#!/usr/bin/env bash
# install-werk-verbs.sh — #3064 AC8 bootstrap.
#
# Builds each werk-* verb crate from canonical chorus and installs the release
# binary to ~/.chorus/bin/. This resolves the chicken-egg the #3047 PoC surfaced:
# the acp.yml workflow shells the verbs by name (werk-commit, werk-push, ...) so
# they MUST be on PATH before act can run the workflow. CI build-on-merge can
# call this on every verb-card landing; the /acp skill's preface step can call
# it as a safety net (it's idempotent — re-running is a fresh build+install).
#
# Idempotent: cargo build is no-op if nothing changed; install overwrites.
# Fail-loud: any cargo failure exits non-zero with the offending crate's error.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
BIN_DIR="${CHORUS_BIN_DIR:-$HOME/.chorus/bin}"

# #3092 — werk-demo added so the demo orchestrator binary ('#'3046) is on PATH
# alongside the other v2 verbs. Surfaced by the v2 maiden voyage on '#'3096:
# werk-demo wasn't installed by the bootstrap, so the demo step needed a manual
# build+install. Drive-by inside #3092's substrate sweep.
VERBS=(werk-pull werk-commit werk-push werk-build werk-deploy werk-accept werk-demo)

mkdir -p "$BIN_DIR"
echo "install-werk-verbs: building + installing ${#VERBS[@]} verb(s) to $BIN_DIR"
echo "                    from $CHORUS_ROOT"

failed=()
for v in "${VERBS[@]}"; do
  crate_dir="$CHORUS_ROOT/platform/services/$v"
  if [ ! -f "$crate_dir/Cargo.toml" ]; then
    echo "  ✗ $v: no crate at $crate_dir/Cargo.toml — skipping (verb not yet landed?)"
    failed+=("$v:missing")
    continue
  fi

  echo "  → building $v..."
  if ! cargo build --release --quiet --manifest-path "$crate_dir/Cargo.toml" 2>&1; then
    echo "  ✗ $v: cargo build failed"
    failed+=("$v:build")
    continue
  fi

  bin="$crate_dir/target/release/$v"
  if [ ! -x "$bin" ]; then
    echo "  ✗ $v: built but no executable at $bin"
    failed+=("$v:no-binary")
    continue
  fi

  cp -f "$bin" "$BIN_DIR/$v"
  installed_cdhash=$(codesign -dvvv "$BIN_DIR/$v" 2>&1 | awk '/CDHash=/{print $1}' | head -1)
  echo "  ✓ $v → $BIN_DIR/$v ${installed_cdhash:+($installed_cdhash)}"
done

if [ ${#failed[@]} -gt 0 ]; then
  echo
  echo "install-werk-verbs: FAILED — ${failed[*]}"
  exit 1
fi

echo
echo "install-werk-verbs: ${#VERBS[@]}/${#VERBS[@]} verbs installed."
echo "PATH check: ensure $BIN_DIR is on \$PATH (chorus-env-setup.sh prepends it)."
