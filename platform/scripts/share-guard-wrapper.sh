#!/usr/bin/env bash
# share-guard-wrapper.sh (#3744) — launch the share guard with its credential
# from the runtime env file, never from a committed one.
#
# The allowlist comes from config/share-allowlist.txt (governed, version
# controlled). The CREDENTIAL comes from ~/.chorus/share-auth.env, written by
# chorus-share per session and never committed. If that file is absent the guard
# refuses to start unauthenticated — the correct failure, not a silent naked door.
set -uo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
AUTH_ENV="${SHARE_AUTH_ENV:-$HOME/.chorus/share-auth.env}"

if [ -f "$AUTH_ENV" ]; then
  # shellcheck disable=SC1090
  . "$AUTH_ENV"
else
  echo "share-guard-wrapper: no credential file at $AUTH_ENV — the guard will refuse to start (by design, #3744)" >&2
fi

exec python3 "$CHORUS_ROOT/platform/scripts/chorus-share-guard.py"
