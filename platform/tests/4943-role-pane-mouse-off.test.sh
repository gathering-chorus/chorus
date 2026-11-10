#!/usr/bin/env bash
# @test-type: integration — reads the live tmux server that hosts the role panes
# @domain: identity
# @needs: machine — runs as jeffbridwell (the account whose tmux server hosts every role pane); prod after the land
# @card: #4943 · owner: wren
# #4943 AC2 — every live role session (chorus-<role>) reports mouse off, so Jeff copies a
# role pane's text with a plain drag. A session started before the land keeps its old value
# until that role's next login, which goes through start_in_pane_args.
# Fails loudly when the server is unreachable or holds no role session: never a vacuous pass.
set -u
TMUX_BIN="${TMUX_BIN:-tmux}"
SOCK_ARGS=()
[ -n "${TMUX_SOCKET:-}" ] && SOCK_ARGS=(-L "$TMUX_SOCKET")

sessions=$("$TMUX_BIN" ${SOCK_ARGS[@]+"${SOCK_ARGS[@]}"} list-sessions -F '#{session_name}' 2>&1) || {
  echo "RED: cannot reach the tmux server as $(id -un): $sessions"; exit 1; }
roles=$(printf '%s\n' "$sessions" | grep '^chorus-' || true)
[ -n "$roles" ] || { echo "RED: no chorus-<role> session on this tmux server"; exit 1; }

fail=0
for s in $roles; do
  v=$("$TMUX_BIN" ${SOCK_ARGS[@]+"${SOCK_ARGS[@]}"} show-options -t "$s" -v mouse 2>/dev/null)
  [ -n "$v" ] || v=$("$TMUX_BIN" ${SOCK_ARGS[@]+"${SOCK_ARGS[@]}"} show-options -g -v mouse 2>/dev/null)
  if [ "$v" = "off" ]; then echo "GREEN $s: mouse off"; else echo "RED $s: mouse ${v:-unreadable}"; fail=1; fi
done
exit $fail
