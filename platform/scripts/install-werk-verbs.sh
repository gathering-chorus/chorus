#!/usr/bin/env bash
# install-werk-verbs.sh — RETIRED 2026-05-28 (#3110).
#
# This script's body retired in favor of `chorus-deploy --all werk`, which
# absorbs the install path into the single deploy substrate for all
# chorus-managed binaries (chorus:principle-no-competing-implementations).
#
# Backward-compat shim: any caller that hard-codes this script name still
# works during the transition. Delete this file once no callers remain
# (grep platform/ + .github/ for "install-werk-verbs.sh"; expect 0 hits).
#
# History:
#   2026-05-09 #2864 — show-gate.sh + accept_gate.rs as PreToolUse hook
#   2026-05-?? #3064 AC8 — original install-werk-verbs.sh authored as the
#       bootstrap path for werk-* verb binaries
#   2026-05-28 #3110 — retired; chorus-deploy --all werk is the single path

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$SCRIPT_DIR/chorus-deploy" --all werk "$@"
