#!/usr/bin/env bash
# fuseki-shiro-deploy.sh (#3630) — deploy the canonical Fuseki Shiro config to
# the runtime location, substituting the write credential from its canonical home.
#
# The committed canonical config (launchagents-canonical/fuseki-shiro.ini) carries
# ${FUSEKI_ADMIN_PASSWORD} as a PLACEHOLDER — never the secret. This step reads the
# real value from the gathering app .env (the SAME file the fuseki service and
# chorus-api-wrapper.sh already source — no new secret home) and writes the runtime
# ~/.gathering/data/shiro.ini (0600). The password is never echoed, never in the repo.
#
# DEPLOY-BEFORE-REQUIRE: this flips :3030 writes to require Basic auth. Every writer
# must already carry the credential (TS via fusekiWriteAuthFromEnv #3566; shell via
# fuseki-auth.sh) BEFORE running this, or its writes 401. Run --check first.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="$SCRIPT_DIR/launchagents-canonical/fuseki-shiro.ini"
RUNTIME="${FUSEKI_SHIRO_RUNTIME:-$HOME/.gathering/data/shiro.ini}"
# #3611 UNTANGLE — the credential's canonical home is shared infra beside the
# store: $FUSEKI_BASE/fuseki-write.env (0600, owner: Silas/ops). This script is
# its PROVISIONER. Bootstrap (first run, file absent) reads the credential from
# GATHERING_APP_ENV — an explicit, operator-supplied path, no hardcoded reach
# into gathering's tree — and writes the shared-infra file. Every later run,
# and every chorus writer, reads only the shared-infra home.
CRED_ENV="${FUSEKI_WRITE_ENV:-$HOME/.gathering/data/fuseki-write.env}"

# --- read the credential (fail-closed; targeted, never echo it) ---
# Absent key → empty string, NOT a set -e death: grep exits 1 on no-match and a
# failing $(...) in an assignment kills the script silently. FUSEKI_ADMIN_USER
# is legitimately absent (defaults to admin), so tolerate no-match here and let
# the explicit -z check below own the fail-closed decision.
read_key() { grep -E "^$2=" "$1" 2>/dev/null | head -1 | cut -d= -f2- || true; }
if [ -r "$CRED_ENV" ]; then
  FUSEKI_ADMIN_PASSWORD="$(read_key "$CRED_ENV" FUSEKI_ADMIN_PASSWORD)"
  FUSEKI_ADMIN_USER="$(read_key "$CRED_ENV" FUSEKI_ADMIN_USER)"
elif [ -n "${GATHERING_APP_ENV:-}" ] && [ -r "$GATHERING_APP_ENV" ]; then
  FUSEKI_ADMIN_PASSWORD="$(read_key "$GATHERING_APP_ENV" FUSEKI_ADMIN_PASSWORD)"
  FUSEKI_ADMIN_USER="$(read_key "$GATHERING_APP_ENV" FUSEKI_ADMIN_USER)"
else
  echo "fuseki-shiro-deploy: no credential source — $CRED_ENV absent and GATHERING_APP_ENV not set/readable (bootstrap) — refusing (fail-closed)" >&2
  exit 1
fi
export FUSEKI_ADMIN_PASSWORD
if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  echo "fuseki-shiro-deploy: FUSEKI_ADMIN_PASSWORD not found in credential source — refusing (fail-closed)" >&2
  exit 1
fi

# --- provision/refresh the shared-infra credential file (0600, atomic) ---
provision_credenv() {
  umask 077
  local tmp
  tmp="$(mktemp "$(dirname "$CRED_ENV")/.fuseki-write.XXXXXX")" || { echo "provision: mktemp failed" >&2; return 1; }
  {
    printf 'FUSEKI_ADMIN_USER=%s\n' "${FUSEKI_ADMIN_USER:-admin}"
    printf 'FUSEKI_ADMIN_PASSWORD=%s\n' "$FUSEKI_ADMIN_PASSWORD"
  } > "$tmp"
  chmod 600 "$tmp"
  mv -f "$tmp" "$CRED_ENV"
}

# --- substitute ONLY the credential placeholder; write 0600 ATOMICALLY ---
# (#3630 review, Kade): render to a temp file in the SAME dir, then mv into place.
# A mid-stream envsubst failure can never truncate/corrupt the live shiro.ini —
# Fuseki would refuse to start on a half-written config. Live file is only ever
# replaced by a complete, chmod'd temp via atomic rename.
render() {
  umask 077
  local dest="$1" tmp
  tmp="$(mktemp "$(dirname "$dest")/.shiro.XXXXXX")" || { echo "render: mktemp failed" >&2; return 1; }
  if ! envsubst '${FUSEKI_ADMIN_PASSWORD}' < "$CANONICAL" > "$tmp"; then
    rm -f "$tmp"; echo "render: substitution failed — live config left untouched" >&2; return 1
  fi
  chmod 600 "$tmp"
  mv -f "$tmp" "$dest"
}

case "${1:-deploy}" in
  --check|-n)
    # Prove the substitution to a temp file WITHOUT touching the runtime; verify
    # no placeholder survives and (safety) no plaintext secret is printed.
    tmp="$(mktemp)"
    render "$tmp"
    if grep -q 'FUSEKI_ADMIN_PASSWORD' "$tmp"; then
      echo "CHECK FAIL: placeholder survived substitution" >&2; rm -f "$tmp"; exit 1
    fi
    if grep -qE '^/\*\*/update = localhostFilter,authcBasic' "$tmp"; then
      echo "CHECK OK: writes require localhostFilter+authcBasic; credential substituted (value not shown)"
    else
      echo "CHECK FAIL: write paths not locked to authcBasic" >&2; rm -f "$tmp"; exit 1
    fi
    rm -f "$tmp"
    ;;
  deploy)
    provision_credenv
    render "$RUNTIME"
    # #3630 review (Wren): witness the flip operation on the spine.
    "${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/platform/scripts/chorus-log" \
      fuseki.shiro.deployed "${DEPLOY_ROLE:-silas}" runtime="$RUNTIME" writes=authcBasic credenv="$CRED_ENV" 2>/dev/null || true
    echo "fuseki-shiro-deploy: wrote $RUNTIME (0600) + provisioned $CRED_ENV (0600). Fuseki must reload to apply —"
    echo "  launchctl kickstart -k gui/$(id -u)/com.gathering.fuseki   (run on GO; this is the live flip)"
    ;;
  *)
    echo "usage: fuseki-shiro-deploy.sh [--check | deploy]" >&2; exit 2;;
esac
