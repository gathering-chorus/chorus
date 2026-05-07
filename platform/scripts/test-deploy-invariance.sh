#!/usr/bin/env bash
# test-deploy-invariance.sh — proving gate for the deploy-domain (#2775).
#
# Same artifact must reach the same running state, every time.
# Install the same source binary twice via chorus-bin-install; assert
# the on-disk sha256 + cdhash are identical across both installs and
# emit deploy.artifact.verified for each.
#
# Usage: ./test-deploy-invariance.sh [<target>]
#   target defaults to chorus-hook-shim
#
# Exits:
#   0  invariance holds
#   1  invariance violated (cdhash or sha256 differs across installs)
#   2  setup error (artifact missing, install failed, etc.)

set -euo pipefail

TARGET="${1:-chorus-hook-shim}"
CHORUS_HOME="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"
INSTALL_LOC="${HOME}/.chorus/bin/${TARGET}"
INSTALL_SCRIPT="${CHORUS_HOME}/platform/scripts/chorus-bin-install"
# Cargo builds always land in the canonical chorus tree's target/release/
# regardless of CHORUS_HOME (the crate path in build-signed.sh resolves there).
# Source the test artifact from canonical, not from CHORUS_HOME, so this test
# works whether invoked from werk or canonical.
CARGO_TREE="/Users/jeffbridwell/CascadeProjects/chorus"

case "$TARGET" in
  chorus-hook-shim)
    SRC="${CARGO_TREE}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
    ;;
  chorus-hooks)
    SRC="${CARGO_TREE}/platform/services/chorus-hooks/target/release/chorus-hooks"
    ;;
  chorus-inject)
    SRC="${CARGO_TREE}/platform/services/chorus-inject/target/release/chorus-inject"
    ;;
  *)
    echo "test-deploy-invariance: unknown target '$TARGET'" >&2
    exit 2
    ;;
esac

if [ ! -x "$SRC" ]; then
  echo "test-deploy-invariance: source artifact not found or not executable: $SRC" >&2
  echo "  Run build-signed.sh $TARGET first." >&2
  exit 2
fi

if [ ! -x "$INSTALL_SCRIPT" ]; then
  echo "test-deploy-invariance: chorus-bin-install not found at $INSTALL_SCRIPT" >&2
  exit 2
fi

SRC_SHA=$(shasum -a 256 "$SRC" | awk '{print $1}')
echo "test-deploy-invariance: target=$TARGET source=$SRC source_sha256=$SRC_SHA"

emit_verified() {
  local cdhash="$1"
  local label="$2"
  if command -v chorus-log >/dev/null 2>&1; then
    chorus-log deploy.artifact.verified "$ROLE" \
      "artifact_sha256=$SRC_SHA" "target=$TARGET" \
      "installed_cdhash=$cdhash" "deployed_at=$INSTALL_LOC" >/dev/null 2>&1 || true
  elif [ -x "${CHORUS_HOME}/platform/scripts/chorus-log" ]; then
    "${CHORUS_HOME}/platform/scripts/chorus-log" \
      deploy.artifact.verified "$ROLE" \
      "artifact_sha256=$SRC_SHA" "target=$TARGET" \
      "installed_cdhash=$cdhash" "deployed_at=$INSTALL_LOC" >/dev/null 2>&1 || true
  fi
  echo "test-deploy-invariance: $label — deploy.artifact.verified emitted"
}

install_and_capture() {
  local label="$1"
  "$INSTALL_SCRIPT" "$SRC" "$TARGET" >&2
  if [ ! -f "$INSTALL_LOC" ]; then
    echo "test-deploy-invariance: $label — install location missing after install: $INSTALL_LOC" >&2
    exit 2
  fi
  local installed_sha cdhash
  installed_sha=$(shasum -a 256 "$INSTALL_LOC" | awk '{print $1}')
  cdhash=$(codesign -dvvv "$INSTALL_LOC" 2>&1 | grep "^CDHash=" | head -1 | sed 's/^CDHash=//')
  echo "${installed_sha}|${cdhash}"
}

R1=$(install_and_capture "install 1")
S1="${R1%|*}"; C1="${R1#*|}"
emit_verified "$C1" "install 1"
echo "test-deploy-invariance: install 1 sha256=$S1 cdhash=$C1"

R2=$(install_and_capture "install 2")
S2="${R2%|*}"; C2="${R2#*|}"
emit_verified "$C2" "install 2"
echo "test-deploy-invariance: install 2 sha256=$S2 cdhash=$C2"

PASS=1
if [ "$S1" != "$S2" ]; then
  echo "test-deploy-invariance: FAIL — installed sha256 drift" >&2
  echo "  install 1: $S1" >&2
  echo "  install 2: $S2" >&2
  PASS=0
fi
if [ "$C1" != "$C2" ]; then
  echo "test-deploy-invariance: FAIL — installed cdhash drift" >&2
  echo "  install 1: $C1" >&2
  echo "  install 2: $C2" >&2
  PASS=0
fi
if [ "$S1" != "$SRC_SHA" ]; then
  echo "test-deploy-invariance: FAIL — installed sha256 differs from source" >&2
  echo "  source:    $SRC_SHA" >&2
  echo "  installed: $S1" >&2
  PASS=0
fi

if [ "$PASS" = "1" ]; then
  echo "test-deploy-invariance: PASS — same artifact reached same running state across two installs"
  exit 0
else
  exit 1
fi
