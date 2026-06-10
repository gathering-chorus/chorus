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

# #2931 — inherit trace_id + card_id from the commit being built. chorus_acp
# writes `Chorus-Trace-Id:` / `Chorus-Card-Id:` git trailers on every acp
# commit; reading them here lets chorus-log's env-bridge (#2857) tag every
# downstream build.* event with the same trace_id the ACP step used, so
# chorus_logs_for_trace returns one continuous chain pull → acp → build →
# deploy. Env wins over trailer (caller can override); trailer fills the gap
# when the pipeline picks up a commit cold.
if [ -z "${CHORUS_TRACE_ID:-}" ] || [ -z "${CHORUS_CARD_ID:-}" ]; then
  _trailers=$(git -C "$ROOT" log -1 --format=%B 2>/dev/null \
              | git interpret-trailers --parse 2>/dev/null || true)
  if [ -z "${CHORUS_TRACE_ID:-}" ]; then
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
    [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
  fi
  if [ -z "${CHORUS_CARD_ID:-}" ]; then
    _cid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Card-Id"{print $2;exit}')
    [ -n "$_cid" ] && export CHORUS_CARD_ID="$_cid"
  fi
fi

# #2931 — failure-emit trap. AC5 of #2931: simulated failure at build produces
# result=fail event with error= field visible in chorus_logs_for_card. ERR
# trap fires on any uncaught nonzero (set -e in effect), captures the failing
# line + command + exit code, emits build.failed via chorus-log, then exits.
# chorus-log's env-bridge (#2857) attaches CHORUS_TRACE_ID / CHORUS_CARD_ID
# automatically, so the failure joins the same trace chain as the
# build.artifact.hashed success events.
_build_role="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"
_emit_build_failed() {
  local exit_code="$1" line_no="$2" failed_cmd="$3"
  local err_msg="line=${line_no} cmd=${failed_cmd} exit=${exit_code}"
  if command -v chorus-log >/dev/null 2>&1; then
    chorus-log build.failed "$_build_role" \
      "domain=chorus" "result=fail" "error=$err_msg" "exit_code=$exit_code" >/dev/null 2>&1 || true
  elif [ -x "${CHORUS_HOME:-$ROOT}/platform/scripts/chorus-log" ]; then
    "${CHORUS_HOME:-$ROOT}/platform/scripts/chorus-log" \
      build.failed "$_build_role" \
      "domain=chorus" "result=fail" "error=$err_msg" "exit_code=$exit_code" >/dev/null 2>&1 || true
  fi
}
trap '_emit_build_failed $? $LINENO "$BASH_COMMAND"' ERR

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

# Per-binary post-codesign work: hash, emit spine event, add manifest entry.
# Refactored into a function (#2791) so chorus-hooks shortcut's two binaries
# (chorus-hook-shim primary + chorus-hooks secondary) BOTH get spine events
# AND manifest entries — closes the secondary-emit gap.
#
# Globals read: SIGNING_IDENTITY, crate_dir
# Args:
#   $1 — binary path (target/release/<name>)
#   $2 — installed name (passed to chorus-bin-install)
emit_artifact_records() {
  local bin_path="$1" installed_name="$2"
  echo "build-signed: verify $installed_name"
  codesign -dvvv "$bin_path" 2>&1 | grep -E "^Identifier=|^Authority=" | head -2
  local cdhash
  cdhash=$(codesign -dvvv "$bin_path" 2>&1 | grep "^CDHash=" | head -1 | sed 's/^CDHash=//')
  echo "build-signed: cdhash=$cdhash"

  # Build-invariance evidence (#2775). cdhash is the binding identity (#2734);
  # sha256 is informational and drifts per cargo+codesign timestamp behavior.
  local sha256 commit crate builder_host role build_time
  sha256=$(shasum -a 256 "$bin_path" | awk '{print $1}')
  commit=$(git -C "$(dirname "$crate_dir")" rev-parse --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null || echo "unknown")
  crate=$(basename "$crate_dir")
  builder_host=$(hostname -s)
  role="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"
  build_time=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  echo "build-signed: sha256=$sha256"

  # build.artifact.hashed spine event (existing — used by test-build-invariance.sh)
  if command -v chorus-log >/dev/null 2>&1; then
    chorus-log build.artifact.hashed "$role" \
      "domain=chorus" "commit=$commit" "crate=$crate" "identifier=$installed_name" \
      "cdhash=$cdhash" "sha256=$sha256" "builder_host=$builder_host" >/dev/null 2>&1 || true
  elif [ -x "${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/chorus-log" ]; then
    "${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/chorus-log" \
      build.artifact.hashed "$role" \
      "domain=chorus" "commit=$commit" "crate=$crate" "identifier=$installed_name" \
      "cdhash=$cdhash" "sha256=$sha256" "builder_host=$builder_host" >/dev/null 2>&1 || true
  fi

  # Manifest entry (#2791) — idempotent on {commit, crate, identifier, cdhash}.
  # chorus-manifest emits manifest.entry.added on new entry only.
  local manifest_script="$(dirname "${BASH_SOURCE[0]}")/chorus-manifest"
  if [ -x "$manifest_script" ]; then
    "$manifest_script" add \
      "$commit" "$crate" "$installed_name" "$cdhash" "$sha256" \
      "$build_time" "$builder_host" "$role" >/dev/null 2>&1 || \
      echo "build-signed: WARN — chorus-manifest add failed (non-fatal)" >&2
  fi
}

# Primary binary: emit + record
emit_artifact_records "$binary" "$binary_name"

# chorus-hooks shortcut: also emit + record for the second binary (closes the
# secondary-binary gap that was a TODO before #2791).
if [ "${1:-}" = "chorus-hooks" ] && [ -f "${HOOKS_BIN:-}" ]; then
  emit_artifact_records "$HOOKS_BIN" "chorus-hooks"
fi

# Install to ~/.chorus/bin/ — the canonical deploy location (#2734).
# target/release/ is a build artifact; ~/.chorus/bin/ is what the running
# system calls. Splitting build from install means cdhash stays stable
# across rebuilds-without-source-change AND the installed binary is
# traceable to a commit via the binary.deployed spine event.
#
# #2774: BUILD_SKIP_INSTALL=1 splits build from deploy in the building-pipeline.
# chorus-build (build-only) sets this; werk-deploy invokes chorus-bin-install
# separately. Default unset → install runs (preserves existing call sites).
if [ -n "${BUILD_SKIP_INSTALL:-}" ]; then
  echo "build-signed: BUILD_SKIP_INSTALL set — skipping install to ~/.chorus/bin/"
else
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
fi

echo "build-signed: done"
