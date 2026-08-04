#!/usr/bin/env bash
# #2478 — the ONE bash resolver for chorus-hook-shim, mirror of the TS
# resolveShimPath() (platform/api/src/shim-path.ts, #2474). Resolution order is
# the CONTRACT the parity test pins (shim-path-parity.test.ts), REORDERED by
# #3750 to deploy-artifact-first (#2734: ~/.chorus/bin is what RUNS;
# target/release is what was last COMPILED — probing the build path produced
# the 2026-08-04 false "shim missing" fire during build churn and a suite
# testing a stale pre-membrane leftover):
#   1. CHORUS_SHIM_BIN — explicit override (CI, alternate builds)
#   2. deployed: ~/.chorus/bin/chorus-hook-shim, else command -v
#   3. fallback for pre-deploy systems: CHORUS_ROOT-derived build path,
#      else the canonical default root. Host-relative by nature — the test
#      pins "non-empty absolute", not the string, for this leg.
# Source this file; do not execute. Prints the path; never fails the caller.
SHIM_REL="platform/services/chorus-hooks/target/release/chorus-hook-shim"

resolve_shim_path() {
  if [ -n "${CHORUS_SHIM_BIN:-}" ]; then
    printf '%s\n' "${CHORUS_SHIM_BIN}"
    return 0
  fi
  if [ -x "$HOME/.chorus/bin/chorus-hook-shim" ]; then
    printf '%s\n' "$HOME/.chorus/bin/chorus-hook-shim"
    return 0
  fi
  local deployed
  deployed="$(command -v chorus-hook-shim 2>/dev/null || true)"
  if [ -n "$deployed" ]; then
    printf '%s\n' "$deployed"
    return 0
  fi
  if [ -n "${CHORUS_ROOT:-}" ]; then
    printf '%s/%s\n' "${CHORUS_ROOT%/}" "${SHIM_REL}"
    return 0
  fi
  printf '%s/%s\n' "/Users/jeffbridwell/CascadeProjects/chorus" "${SHIM_REL}"
}
