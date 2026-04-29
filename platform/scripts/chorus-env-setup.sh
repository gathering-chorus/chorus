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

# --- cleanup tmp vars --------------------------------------------------------
unset __chorus_env_self __chorus_env_dir __chorus_env_root
