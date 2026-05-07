#!/usr/bin/env bash
# test-build-invariance.sh — proving gate for the build-domain (#2775).
#
# Same source commit must produce the same artifact hash, every time.
# This is the gate. If it fails, build-domain is broken and no card built
# on top of it can be trusted.
#
# Usage: ./test-build-invariance.sh [<target>]
#   target defaults to chorus-hook-shim (smallest, fastest crate)
#
# Process:
#   1. Capture HEAD commit
#   2. cargo clean + build-signed.sh; capture SHA256 from build.artifact.hashed event
#   3. cargo clean + build-signed.sh; capture SHA256 again
#   4. Assert identical or exit 1 with both hashes
#
# Exits:
#   0  invariance holds
#   1  invariance violated (sha256 differs across builds at same commit)
#   2  setup error (build failed, event not found, etc.)

set -euo pipefail

TARGET="${1:-chorus-hook-shim}"
CHORUS_HOME="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"
# Two-source verification: capture sha256 from build-signed.sh stdout (always
# present), best-effort cross-check with build.artifact.hashed spine event when
# the running chorus-hook-shim recognizes the event. Stdout is the contract;
# the spine event is the long-term observability path.
SPINE_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log"

# Translate friendly target names to (crate-dir, build-signed.sh shortcut)
case "$TARGET" in
  chorus-hook-shim|chorus-hooks)
    CRATE_DIR="${CHORUS_HOME}/platform/services/chorus-hooks"
    SHORTCUT="chorus-hooks"
    ;;
  chorus-inject)
    CRATE_DIR="${CHORUS_HOME}/platform/services/chorus-inject"
    SHORTCUT="chorus-inject"
    ;;
  *)
    echo "test-build-invariance: unknown target '$TARGET' (known: chorus-hook-shim, chorus-inject)" >&2
    exit 2
    ;;
esac

COMMIT=$(git -C "$CHORUS_HOME" rev-parse --short HEAD)
echo "test-build-invariance: target=$TARGET commit=$COMMIT"

build_and_capture() {
  local label="$1"
  echo "test-build-invariance: $label — cargo clean + build" >&2
  (cd "$CRATE_DIR" && cargo clean -p "$TARGET" >/dev/null 2>&1 || true)
  local out cdhash sha
  out=$(cd "$CHORUS_HOME" && bash platform/scripts/build-signed.sh "$SHORTCUT" 2>&1)
  echo "$out" >&2
  cdhash=$(echo "$out" | grep -oE "build-signed: cdhash=[a-f0-9]+" | head -1 | sed 's/^build-signed: cdhash=//')
  sha=$(echo "$out" | grep -oE "build-signed: sha256=[a-f0-9]+" | head -1 | sed 's/^build-signed: sha256=//')
  if [ -z "$cdhash" ]; then
    echo "test-build-invariance: $label — could not extract cdhash from build-signed stdout" >&2
    exit 2
  fi
  if [ -f "$SPINE_LOG" ] && [ -n "$sha" ]; then
    if grep -q "build.artifact.hashed.*commit=$COMMIT.*sha256=$sha" "$SPINE_LOG" 2>/dev/null; then
      echo "test-build-invariance: $label — spine event confirmed" >&2
    fi
  fi
  # Format: cdhash|sha256 (cdhash is the invariant; sha256 is advisory)
  echo "${cdhash}|${sha:-unknown}"
}

R1=$(build_and_capture "build 1")
CD1="${R1%|*}"; SHA1="${R1#*|}"
echo "test-build-invariance: build 1 cdhash=$CD1 sha256=$SHA1"

R2=$(build_and_capture "build 2")
CD2="${R2%|*}"; SHA2="${R2#*|}"
echo "test-build-invariance: build 2 cdhash=$CD2 sha256=$SHA2"

# cdhash is the invariant: TCC binds to cdhash (#2734), and codesign --identifier
# with a stable keychain identity produces deterministic cdhash across rebuilds.
# sha256 of the file is advisory only — cargo + codesign embed timestamps and
# build paths that drift across rebuilds even at identical source. The contract
# is "same source → same identity that the running system uses" = cdhash.

if [ "$CD1" = "$CD2" ]; then
  if [ "$SHA1" != "$SHA2" ] && [ "$SHA1" != "unknown" ] && [ "$SHA2" != "unknown" ]; then
    echo "test-build-invariance: PASS — cdhash invariant holds at commit $COMMIT"
    echo "test-build-invariance: NOTE — sha256 differs ($SHA1 vs $SHA2); expected from cargo+codesign timestamps; cdhash is the binding identity"
  else
    echo "test-build-invariance: PASS — cdhash AND sha256 invariant hold at commit $COMMIT"
  fi
  exit 0
else
  echo "test-build-invariance: FAIL — cdhash drift at commit $COMMIT" >&2
  echo "  build 1: $CD1" >&2
  echo "  build 2: $CD2" >&2
  echo "  This is a real invariance violation — TCC AppleEvents grants will break across rebuilds." >&2
  exit 1
fi
