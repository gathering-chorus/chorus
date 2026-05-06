#!/usr/bin/env bash
# build-signed.sh — cargo build + post-build codesign with stable identity.
#
# Why: cargo build --release emits ad-hoc-signed Mach-O with a rebuild-
# dependent identifier suffix (e.g., chorus_hook_shim-d1da85efc2bf3103),
# AND ad-hoc signing produces a fresh ephemeral identity per build. macOS
# TCC binds AppleEvents permission to the cdhash, so every rebuild silently
# revokes the grant. Pinning the identifier AND signing with a stable
# keychain identity keeps the cdhash deterministic across rebuilds.
#
# Identity:    Chorus Local Signing (self-signed cert in the user keychain;
#              hash 9086CB9855BC4642CA03D0B6415A50BD90B86AE3 by default,
#              override via CHORUS_SIGNING_IDENTITY env var).
#
# Cross-machine note: this cert lives in Library Mac's user keychain only.
# Today only Library runs chorus-inject, so identity availability isn't a
# concern. If a second machine ever needs to build (e.g., Bedroom doing
# chorus-inject builds), either: (a) export the cert from Library
# (Keychain Access → My Certificates → "Chorus Local Signing" → File →
# Export Items, save as .p12 with password) and import to the new
# machine's user keychain; OR (b) generate a fresh self-signed code-signing
# cert there (`Keychain Access → Certificate Assistant → Create a
# Certificate, type=Code Signing, Self Signed Root, set name`), grab the
# resulting hash with `security find-identity -v -p codesigning`, and
# override SIGNING_IDENTITY via env. Either way, the new machine also has
# to grant Accessibility once on first chorus-inject run. Without these
# steps build-signed.sh falls back to ad-hoc signing on the new machine
# and re-introduces the cdhash-churn bug locally there.
#
# History note: prior version of this script (silas #2218) used `--sign -`
# (ad-hoc) with a stable identifier. That fixed the identifier-suffix churn
# but did NOT survive TCC's keychain-identity check — ad-hoc binaries get
# different TCC treatment than identity-signed ones. #2548 (2026-05-04)
# upgraded to keychain-identity signing after the System Events daemon
# cache + cdhash-rebuild interaction was reproduced.
#
# Usage:
#   build-signed.sh <shortcut>
#   build-signed.sh <crate-dir> <identifier> <binary-name>
#
# Shortcuts:
#   build-signed.sh chorus-hooks    → signs chorus-hook-shim as com.chorus.hook-shim
#                                     AND chorus-hooks as com.chorus.hooks
#   build-signed.sh chorus-inject   → signs chorus-inject as com.chorus.inject
set -euo pipefail

SIGNING_IDENTITY="${CHORUS_SIGNING_IDENTITY:-9086CB9855BC4642CA03D0B6415A50BD90B86AE3}"

ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

resolve_crate() {
  # chorus-hooks shortcut signs BOTH binaries (chorus-hook-shim + chorus-hooks)
  # via a follow-up sign step at the bottom. Returned spec drives the primary
  # binary; the second is handled inline post-build.
  case "$1" in
    chorus-hooks)  echo "$ROOT/platform/services/chorus-hooks|com.chorus.hook-shim|chorus-hook-shim" ;;
    chorus-inject) echo "$ROOT/platform/services/chorus-inject|com.chorus.inject|chorus-inject" ;;
    *)             echo "" ;;
  esac
}

if [ $# -eq 1 ]; then
  spec="$(resolve_crate "$1")"
  if [ -z "$spec" ]; then
    echo "build-signed: unknown shortcut '$1' (known: chorus-hooks, chorus-inject)" >&2
    exit 2
  fi
  IFS='|' read -r crate_dir identifier binary_name <<< "$spec"
elif [ $# -eq 3 ]; then
  crate_dir="$1"
  identifier="$2"
  binary_name="$3"
else
  echo "Usage: build-signed.sh <chorus-hooks|chorus-inject>" >&2
  echo "   or: build-signed.sh <crate-dir> <identifier> <binary-name>" >&2
  exit 2
fi

if [ ! -d "$crate_dir" ]; then
  echo "build-signed: crate dir not found: $crate_dir" >&2
  exit 1
fi

echo "build-signed: cargo build --release in $crate_dir"
(cd "$crate_dir" && cargo build --release)

binary="$crate_dir/target/release/$binary_name"
if [ ! -f "$binary" ]; then
  echo "build-signed: binary not produced: $binary" >&2
  exit 1
fi

echo "build-signed: codesign --force --sign $SIGNING_IDENTITY --identifier $identifier"
codesign --force --sign "$SIGNING_IDENTITY" --identifier "$identifier" "$binary"

# chorus-hooks shortcut: also sign the second crate binary (chorus-hooks)
if [ "${1:-}" = "chorus-hooks" ]; then
  HOOKS_BIN="$crate_dir/target/release/chorus-hooks"
  if [ -f "$HOOKS_BIN" ]; then
    codesign --force --sign "$SIGNING_IDENTITY" --identifier "com.chorus.hooks" "$HOOKS_BIN"
    echo "build-signed: $(basename "$HOOKS_BIN") signed identifier=com.chorus.hooks"
  fi
fi

echo "build-signed: verify"
codesign -dvvv "$binary" 2>&1 | grep -E "^Identifier=|^Authority=" | head -2
CDHASH=$(codesign -dvvv "$binary" 2>&1 | grep "^CDHash=" | head -1 | sed 's/^CDHash=//')
echo "build-signed: cdhash=$CDHASH"

# Install to ~/.chorus/bin/ — the canonical deploy location (#2734).
# target/release/ is a build artifact; ~/.chorus/bin/ is what the running
# system calls. Splitting build from install means cdhash stays stable
# across rebuilds-without-source-change AND the installed binary is
# traceable to a commit via the binary.deployed spine event.
INSTALL_SCRIPT="$(dirname "${BASH_SOURCE[0]}")/chorus-bin-install"
if [ -x "$INSTALL_SCRIPT" ]; then
  "$INSTALL_SCRIPT" "$binary" "$binary_name"
  # chorus-hooks shortcut also installs the second binary
  if [ "${1:-}" = "chorus-hooks" ] && [ -f "${HOOKS_BIN:-}" ]; then
    "$INSTALL_SCRIPT" "$HOOKS_BIN" "chorus-hooks"
  fi
else
  echo "build-signed: WARN — chorus-bin-install not found; binary signed but not installed to ~/.chorus/bin/" >&2
fi

echo "build-signed: done"
