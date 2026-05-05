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

# Per-role werk var: <ROLE>_WERK points at this role's worktree path.
# Only set when CHORUS_ROLE is known; downstream callers shouldn't see a
# stale or guessed werk path otherwise.
if [ -n "${CHORUS_ROLE:-}" ]; then
  case "$CHORUS_ROLE" in
    kade)  export KADE_WERK="$CHORUS_WERK_BASE/kade"   ;;
    wren)  export WREN_WERK="$CHORUS_WERK_BASE/wren"   ;;
    silas) export SILAS_WERK="$CHORUS_WERK_BASE/silas" ;;
  esac
fi

# CHORUS_BIN: single deployed location for chorus-* binaries (#2734 target).
# Prepend to PATH so signed installs override target/release builds.
# Idempotent: re-sourcing does not duplicate the entry.
export CHORUS_BIN="$HOME/.chorus/bin"
case ":$PATH:" in
  *":$CHORUS_BIN:"*) ;;  # already present, no-op
  *) export PATH="$CHORUS_BIN:$PATH" ;;
esac

# --- cleanup tmp vars --------------------------------------------------------
unset __chorus_env_self __chorus_env_dir __chorus_env_root
