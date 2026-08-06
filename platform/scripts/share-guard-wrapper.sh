#!/usr/bin/env bash
# share-guard-wrapper.sh (#3744, rewritten by #3770) — launch the share guard.
#
# There is no credential to inject any more. #3770 retired HTTP basic auth: people
# sign in as themselves at the identity provider, and the guard holds no shared
# secret at all. What it does hold — a cookie signing key and its OIDC client
# registration — it generates on first run and keeps in ~/.chorus/share-oidc.json,
# which is runtime state and never committed.
#
# Two governed files carry the policy, both version-controlled and diffable:
#   config/share-allowlist.txt   WHAT of the upstream is reachable
#   config/share-principals.txt  WHO may reach it
# Either one missing or empty stops the guard rather than defaulting open.
#
# set -e is deliberately ON here (it was not before): this wrapper's only job is
# to exec the guard, so any failure before that point should be loud. A wrapper
# that swallows its own errors and execs anyway is how a door ends up open for
# reasons nobody can reconstruct.
set -euo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

# A stale SHARE_AUTH left in someone's environment must not look like it is doing
# something. The guard itself exits on it; refusing here too makes the reason
# visible in this log rather than only in the guard's.
if [ -n "${SHARE_AUTH:-}" ]; then
  echo "share-guard-wrapper: SHARE_AUTH is set, but basic auth was retired in #3770. Sign-in is via the identity provider; remove it." >&2
  exit 2
fi

exec python3 "$CHORUS_ROOT/platform/scripts/chorus-share-guard.py"
