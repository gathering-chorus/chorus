#!/usr/bin/env bash
# test-security-probes.sh (#3900) — nightly leg for the security probe suite.
#
# Auto-discovered by nightly-suites.sh (test-*.sh). Deliberately a LIVE-SYSTEM
# probe suite, not a hermetic test: these are fitness functions (ADR-056) whose
# entire job is to compare the CLAIMED security posture against the RUNNING
# system — identity gate refusing, keys present at 0600, public surface
# refusing anon. Read-only + refused-writes only; nothing mutates.
#
# Jeff 2026-08-15: "security changes consistently overstated and under
# implemented" / tracker cards "get built and then decay." A probe that decays
# reds this leg — decay is visible by construction.
set -uo pipefail
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
exec bash "$R/platform/scripts/security-probes-run.sh"
