#!/usr/bin/env bash
# #2478 — the ONE bash resolver for chorus-hook-shim, mirror of the TS
# resolveShimPath() (platform/api/src/shim-path.ts, #2474). Resolution order is
# the CONTRACT the parity test pins (shim-path-parity.test.ts):
#   1. CHORUS_SHIM_BIN — explicit override (CI, alternate builds)
#   2. CHORUS_ROOT-derived build path (same SHIM_REL as TS)
#   3. fallback: command -v (deployed ~/.chorus/bin per #2734), else the
#      canonical default root. Host-relative by nature — the test pins
#      "non-empty absolute", not the string, for this leg.
# Source this file; do not execute. Prints the path; never fails the caller.
SHIM_REL="platform/services/chorus-hooks/target/release/chorus-hook-shim"

resolve_shim_path() {
  if [ -n "${CHORUS_SHIM_BIN:-}" ]; then
    printf '%s\n' "${CHORUS_SHIM_BIN}"
    return 0
  fi
  if [ -n "${CHORUS_ROOT:-}" ]; then
    printf '%s/%s\n' "${CHORUS_ROOT%/}" "${SHIM_REL}"
    return 0
  fi
  local deployed
  deployed="$(command -v chorus-hook-shim 2>/dev/null || true)"
  if [ -n "$deployed" ]; then
    printf '%s\n' "$deployed"
    return 0
  fi
  printf '%s/%s\n' "/Users/jeffbridwell/CascadeProjects/chorus" "${SHIM_REL}"
}
