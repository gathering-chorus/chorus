#!/usr/bin/env bash
# fuseki-shiro-deploy.sh (#3630) — deploy the canonical Fuseki Shiro config to
# the runtime location, substituting the write credential from the realm env.
#
# The committed canonical config (launchagents-canonical/fuseki-shiro.ini) carries
# ${FUSEKI_ADMIN_PASSWORD} as a PLACEHOLDER — never the secret. This step reads the
# real value from ~/.chorus/secrets/chorus-realm.env (0600, owner-only) and writes
# the runtime ~/.gathering/data/shiro.ini (also 0600). The password is never echoed
# and never touches the repo.
#
# DEPLOY-BEFORE-REQUIRE: this flips :3030 writes to require Basic auth. Every writer
# must already carry the credential (TS via fusekiWriteAuthFromEnv #3566; shell via
# fuseki-auth.sh) BEFORE running this, or its writes 401. Run --check first.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="$SCRIPT_DIR/launchagents-canonical/fuseki-shiro.ini"
RUNTIME="${FUSEKI_SHIRO_RUNTIME:-$HOME/.gathering/data/shiro.ini}"
# Credential's canonical home is the gathering app .env (same file the fuseki
# service + chorus-api-wrapper source) — not a new secrets file.
APP_ENV="${GATHERING_APP_ENV:-${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/../jeff-bridwell-personal-site/.env}"

# --- read the credential (fail-closed; targeted, never echo it) ---
if [ ! -r "$APP_ENV" ]; then
  echo "fuseki-shiro-deploy: app .env unreadable at $APP_ENV — refusing (fail-closed)" >&2
  exit 1
fi
FUSEKI_ADMIN_PASSWORD="$(grep -E '^FUSEKI_ADMIN_PASSWORD=' "$APP_ENV" | head -1 | cut -d= -f2-)"
export FUSEKI_ADMIN_PASSWORD
if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
  echo "fuseki-shiro-deploy: FUSEKI_ADMIN_PASSWORD not found in app .env — refusing (fail-closed)" >&2
  exit 1
fi

# --- substitute ONLY the credential placeholder; write 0600 ---
render() {
  umask 077
  envsubst '${FUSEKI_ADMIN_PASSWORD}' < "$CANONICAL" > "$1"
  chmod 600 "$1"
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
    render "$RUNTIME"
    echo "fuseki-shiro-deploy: wrote $RUNTIME (0600). Fuseki must reload to apply —"
    echo "  launchctl kickstart -k gui/$(id -u)/com.gathering.fuseki   (run on GO; this is the live flip)"
    ;;
  *)
    echo "usage: fuseki-shiro-deploy.sh [--check | deploy]" >&2; exit 2;;
esac
