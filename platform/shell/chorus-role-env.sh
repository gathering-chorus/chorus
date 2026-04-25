# chorus-role-env.sh — derive CHORUS_ROLE from cwd at chpwd time so MCP
# transport headers (X-Chorus-Role: ${CHORUS_ROLE} in chorus/.mcp.json) expand
# correctly when claude launches.
#
# Convention (existing): CWD = identity. roles/<name>/ → role <name>.
# This snippet binds the convention into env at the only place it matters
# for MCP — the shell that exec's claude.
#
# Source from ~/.zshrc:
#   [ -f /Users/jeffbridwell/CascadeProjects/chorus/platform/shell/chorus-role-env.sh ] && \
#     source /Users/jeffbridwell/CascadeProjects/chorus/platform/shell/chorus-role-env.sh
#
# #2476 (silas + wren chat silas-wren-1777131059, 2026-04-25).

# Idempotent: re-sourcing .zshrc doesn't redefine.
if [[ -n ${__CHORUS_ROLE_ENV_LOADED:-} ]]; then
  return 0
fi
__CHORUS_ROLE_ENV_LOADED=1

__chorus_set_role_env() {
  case "$PWD" in
    */chorus/roles/wren*)
      export CHORUS_ROLE=wren
      export DEPLOY_ROLE=wren
      ;;
    */chorus/roles/silas*)
      export CHORUS_ROLE=silas
      export DEPLOY_ROLE=silas
      ;;
    */chorus/roles/kade*)
      export CHORUS_ROLE=kade
      export DEPLOY_ROLE=kade
      ;;
  esac
}

# zsh: chpwd_functions hook fires once per directory change.
# bash fallback (PROMPT_COMMAND): trade-off is it fires per prompt rather than
# per cd, but the case branches are cheap and the assignment is idempotent.
if [[ -n ${ZSH_VERSION:-} ]]; then
  autoload -U add-zsh-hook 2>/dev/null
  add-zsh-hook chpwd __chorus_set_role_env
elif [[ -n ${BASH_VERSION:-} ]]; then
  PROMPT_COMMAND="__chorus_set_role_env;${PROMPT_COMMAND:-}"
fi

# Initial fire so already-cd'd shells get the env without waiting for the
# next chpwd / prompt.
__chorus_set_role_env
