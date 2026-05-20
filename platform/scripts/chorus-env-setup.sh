# chorus-env-setup.sh — canonical CHORUS_ROOT + CHORUS_ROLE source.
# #2571 — retire shell-rc-discipline. Source this script as line 1 of any
# chorus shell script that needs CHORUS_ROOT or CHORUS_ROLE; it self-locates
# from BASH_SOURCE so cwd, pre-set env, and shell rc state don't matter.
#
# Source pattern (bash/zsh):
#   source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"
# or from outside platform/scripts/:
#   source /path/to/chorus/platform/scripts/chorus-env-setup.sh
#
# LaunchAgent pattern (plist):
#   <key>ProgramArguments</key>
#   <array>
#     <string>/bin/bash</string>
#     <string>-c</string>
#     <string>source /path/to/chorus/platform/scripts/chorus-env-setup.sh && exec your-command</string>
#   </array>
#
# Authoritative: ignores stale CHORUS_ROOT in env; re-derives from script
# location (matches #2505 fail-loud philosophy — don't trust caller's env).
# Consolidates platform/shell/chorus-role-env.sh CHORUS_ROLE derivation.

# --- self-locate -------------------------------------------------------------
# BASH_SOURCE[0] is this script's path when sourced from bash. zsh uses %x.
__chorus_env_self="${BASH_SOURCE[0]:-${(%):-%x}}"
__chorus_env_dir="$(cd "$(dirname "$__chorus_env_self")" && pwd -P)"
# This script lives at $CHORUS_ROOT/platform/scripts/chorus-env-setup.sh.
__chorus_env_root="$(cd "$__chorus_env_dir/../.." && pwd -P)"

# --- export CHORUS_ROOT (authoritative, ignores prior env) -------------------
export CHORUS_ROOT="$__chorus_env_root"

# --- derive CHORUS_ROLE from cwd (if in a role dir) --------------------------
case "$PWD" in
  */roles/wren*)  export CHORUS_ROLE=wren  DEPLOY_ROLE=wren  ;;
  */roles/silas*) export CHORUS_ROLE=silas DEPLOY_ROLE=silas ;;
  */roles/kade*)  export CHORUS_ROLE=kade  DEPLOY_ROLE=kade  ;;
esac

# --- werk + bin (#2735) -----------------------------------------------------
# CHORUS_HOME is the canonical chorus checkout (the read-only-during-sessions
# tree). When sourced from canonical, CHORUS_HOME == CHORUS_ROOT. When
# sourced from a werk (e.g., /chorus-werk/kade), CHORUS_HOME points at the
# sibling /chorus directory — the role's session-start anchor and read
# surface for role state.
__chorus_env_parent="$(cd "$CHORUS_ROOT/.." && pwd -P)"
case "$CHORUS_ROOT" in
  *chorus-werk/*)
    # Werk: canonical lives at <parent of chorus-werk>/chorus
    __chorus_env_werk_parent="$(cd "$__chorus_env_parent/.." && pwd -P)"
    export CHORUS_HOME="$__chorus_env_werk_parent/chorus"
    unset __chorus_env_werk_parent ;;
  *)
    # Canonical or unknown: CHORUS_HOME == CHORUS_ROOT
    export CHORUS_HOME="$CHORUS_ROOT" ;;
esac

# CHORUS_WERK_BASE is where per-role git worktrees live. Default sibling to
# CHORUS_HOME so chorus-werk and chorus stay symmetric on disk.
# Caller-provided value is preserved (tests / CI may override).
if [ -z "${CHORUS_WERK_BASE:-}" ]; then
  __chorus_env_home_parent="$(cd "$CHORUS_HOME/.." && pwd -P)"
  export CHORUS_WERK_BASE="$__chorus_env_home_parent/chorus-werk"
  unset __chorus_env_home_parent
fi
unset __chorus_env_parent

# Per-role werk var: <ROLE>_WERK points at the role's ephemeral werk —
# but only when exactly one exists. The ephemeral model (#2913/#2917) has
# no single persistent werk per role; a role has 0, 1, or N
# chorus-werk/<role>-<card>/ worktrees. Resolve like resolveWorkingTree:
# exactly one match → set it; zero or many → leave unset, so callers see
# absence rather than a guessed or deleted path. `find` (not a shell glob
# or array) keeps this identical under bash and zsh — this script is
# sourced from both.
if [ -n "${CHORUS_ROLE:-}" ]; then
  # `find` exits 1 if CHORUS_WERK_BASE doesn't exist yet (fresh role, no werks
  # ever created); `|| true` keeps that from aborting callers under `set -e`.
  __chorus_env_werk_dir="$(find "$CHORUS_WERK_BASE" -maxdepth 1 -type d -name "$CHORUS_ROLE-*" 2>/dev/null || true)"
  # `grep -c .` exits 1 on empty input (zero werks); `|| true` keeps the
  # count at "0" instead of aborting callers that run `set -e` (#3012).
  __chorus_env_werk_count="$(printf '%s' "$__chorus_env_werk_dir" | grep -c . || true)"
  if [ "$__chorus_env_werk_count" = "1" ]; then
    case "$CHORUS_ROLE" in
      kade)  export KADE_WERK="$__chorus_env_werk_dir"   ;;
      wren)  export WREN_WERK="$__chorus_env_werk_dir"   ;;
      silas) export SILAS_WERK="$__chorus_env_werk_dir"  ;;
    esac
  fi
  unset __chorus_env_werk_dir __chorus_env_werk_count
fi

# CHORUS_BIN: single deployed location for chorus-* binaries (#2734 target).
# Prepend to PATH so signed installs override target/release builds.
# Idempotent: re-sourcing does not duplicate the entry.
export CHORUS_BIN="$HOME/.chorus/bin"
case ":$PATH:" in
  *":$CHORUS_BIN:"*) ;;  # already present, no-op
  *) export PATH="$CHORUS_BIN:$PATH" ;;
esac

# #2995 — WERK_<ROLE>_BIN: per-role-per-werk binary slot. When the role has
# an active werk, expose a bin path inside it and PATH-prefix it before
# CHORUS_BIN. Result: the role's session resolves werk-built binaries before
# canonical ones — the binary the role is iterating on runs in the role's
# own session as it builds. /acp's promote step then moves the slot's
# contents into CHORUS_BIN; /unpull tears it down with the werk.
#
# Variable shape: WERK_${ROLE^^}_BIN — KADE / WREN / SILAS in uppercase to
# match the existing <ROLE>_WERK convention.
#
# Other roles' sessions don't have this set (their CHORUS_ROLE differs or
# their werk is missing); they continue resolving canonical and stay
# undisturbed by this role's in-flight binary.
if [ -n "${CHORUS_ROLE:-}" ]; then
  case "$CHORUS_ROLE" in
    kade)  __chorus_env_role_werk="${KADE_WERK:-}"  ;;
    wren)  __chorus_env_role_werk="${WREN_WERK:-}"  ;;
    silas) __chorus_env_role_werk="${SILAS_WERK:-}" ;;
    *)     __chorus_env_role_werk=""                ;;
  esac
  if [ -n "$__chorus_env_role_werk" ]; then
    __chorus_env_werk_bin="$__chorus_env_role_werk/.werk-bin"
    mkdir -p "$__chorus_env_werk_bin" 2>/dev/null || true
    case "$CHORUS_ROLE" in
      kade)  export WERK_KADE_BIN="$__chorus_env_werk_bin"  ;;
      wren)  export WERK_WREN_BIN="$__chorus_env_werk_bin"  ;;
      silas) export WERK_SILAS_BIN="$__chorus_env_werk_bin" ;;
    esac
    case ":$PATH:" in
      *":$__chorus_env_werk_bin:"*) ;;  # already present, no-op
      *) export PATH="$__chorus_env_werk_bin:$PATH" ;;
    esac
    unset __chorus_env_werk_bin
  fi
  unset __chorus_env_role_werk
fi

# #3016 — CHORUS_MCP_PORT: per-session chorus-mcp endpoint for daemon
# try-before-buy. The daemon equivalent of WERK_<ROLE>_BIN above: a binary is
# isolated per session via PATH-prefix, but a daemon is one shared process on a
# fixed port, so isolation is per-session ENDPOINT instead. Canonical chorus-mcp
# listens on CHORUS_MCP_PORT_CANONICAL (:3341). When the role has an active werk
# whose werk-mcp daemon is deployed (marker file present, written by
# `chorus-deploy --target werk chorus-mcp`), the session resolves its own daemon
# on a deterministic per-role port; .mcp.json interpolates CHORUS_MCP_PORT into
# its url. No active werk daemon → canonical :3341 (no-regression default, AC6).
export CHORUS_MCP_PORT_CANONICAL="${CHORUS_MCP_PORT_CANONICAL:-3341}"
export CHORUS_MCP_PORT="$CHORUS_MCP_PORT_CANONICAL"
if [ -n "${CHORUS_ROLE:-}" ]; then
  # Deterministic per-role werk port — distinct, no collision with chorus-api
  # :3340 or canonical chorus-mcp :3341.
  # LIMITATION (#3016, flagged by Wren): the port is per-ROLE, not per-werk. A
  # role running two werks (two cards in flight) would have both resolve to the
  # same port — collision. Acceptable today (one-card-per-role is the norm);
  # port-per-werk is the follow-on if concurrent-cards-per-role becomes real.
  case "$CHORUS_ROLE" in
    silas) __chorus_mcp_werk_port=3351 ;;
    kade)  __chorus_mcp_werk_port=3352 ;;
    wren)  __chorus_mcp_werk_port=3353 ;;
    *)     __chorus_mcp_werk_port="" ;;
  esac
  if [ -n "$__chorus_mcp_werk_port" ]; then
    export CHORUS_MCP_WERK_PORT="$__chorus_mcp_werk_port"
    case "$CHORUS_ROLE" in
      kade)  __chorus_mcp_role_werk="${KADE_WERK:-}"  ;;
      wren)  __chorus_mcp_role_werk="${WREN_WERK:-}"  ;;
      silas) __chorus_mcp_role_werk="${SILAS_WERK:-}" ;;
      *)     __chorus_mcp_role_werk=""                ;;
    esac
    if [ -n "$__chorus_mcp_role_werk" ] && [ -f "$__chorus_mcp_role_werk/.werk-mcp/active" ]; then
      export CHORUS_MCP_PORT="$__chorus_mcp_werk_port"
    fi
    unset __chorus_mcp_role_werk
  fi
  unset __chorus_mcp_werk_port
fi

# --- cleanup tmp vars --------------------------------------------------------
unset __chorus_env_self __chorus_env_dir __chorus_env_root
