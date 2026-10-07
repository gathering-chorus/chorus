#!/usr/bin/env bash
# #4446 — a com.chorus.* bash service logs its own start, stop and failure.
#
# The bash twin of platform/services/shared/service_lifecycle.rs and
# platform/chorus-sdk/lifecycle/service-lifecycle.js: same events, same fields.
# Source this file, then call ONE of:
#
#   service_lifecycle_daemon com.chorus.session-watcher
#       always-on (KeepAlive): service.started now; service.stopped on SIGTERM
#       or exit 0; service.failed on any other exit. The start also reports the
#       previous run's kill (read from launchd), which no process can log itself.
#
#   service_lifecycle_job com.chorus.deep-health "$@"
#       scheduled job: service.failed when a run exits non-zero, nothing when it
#       succeeds. A 15-second job logging every run would write 11,520 lines a
#       day saying it ran; launchd keeps the run count.
#
# The label argument is the name used when launchd did not start the script
# (XPC_SERVICE_NAME unset: by hand, or under a test). The daemon form installs
# TERM/INT/EXIT traps; the job form runs the script as a child and needs none.
# Source this file; do not execute. Never fails the caller.

_sl_emit() {  # _sl_emit <event> key=value...
  local home="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}"
  local level=()
  [ "$1" = service.failed ] && level=(--level=error)
  bash "$home/platform/scripts/chorus-log" "$1" system "${@:2}" "${level[@]}" >/dev/null 2>&1 || true
}

# launchd's label when launchd started this script (parent pid 1); everything a
# service starts inherits XPC_SERVICE_NAME, so the variable alone is not enough
_sl_launchd_label() {
  [ "$PPID" = 1 ] || return 0
  case "${XPC_SERVICE_NAME:-}" in com.chorus.*) printf '%s' "$XPC_SERVICE_NAME" ;; esac
}

_sl_label() {
  local l; l="$(_sl_launchd_label)"
  printf '%s' "${l:-$1}"
}

# _sl_previous_end <launchctl print text> → "signal=<sig>" | "exit_code=<n>" | "" (clean or none)
_sl_previous_end() {
  local sig code
  sig="$(printf '%s\n' "$1" | awk -F' = ' '/^\tlast terminating signal = /{print $2; exit}')"
  code="$(printf '%s\n' "$1" | awk -F' = ' '/^\tlast exit code = /{split($2,a,":"); print a[1]; exit}')"
  if [ -n "$sig" ]; then
    case "$sig" in Terminated*) ;; *) printf 'signal=%s' "$sig" ;; esac
  # "(never exited)" on a first run is not a failure: only a number counts
  elif [[ "$code" =~ ^-?[0-9]+$ ]] && [ "$code" != 0 ]; then
    printf 'exit_code=%s' "$code"
  fi
}

_SL_SERVICE=""
_SL_MODE=""
_SL_STOP_REASON=""

service_lifecycle_exit() {  # service_lifecycle_exit <status>
  local rc="$1"
  [ -n "$_SL_SERVICE" ] || return 0
  if [ -n "$_SL_STOP_REASON" ] || { [ "$rc" = 0 ] && [ "$_SL_MODE" = daemon ]; }; then
    [ "$_SL_MODE" = daemon ] && _sl_emit service.stopped "service=$_SL_SERVICE" "pid=$$" "reason=${_SL_STOP_REASON:-exit 0}"
  elif [ "$rc" != 0 ]; then
    _sl_emit service.failed "service=$_SL_SERVICE" "pid=$$" "reason=exited $rc" "exit_code=$rc"
  fi
  _SL_SERVICE=""
}

service_lifecycle_daemon() {
  _SL_SERVICE="$(_sl_label "$1")"; _SL_MODE=daemon
  if [ -n "$(_sl_launchd_label)" ]; then
    local prev; prev="$(_sl_previous_end "$(launchctl print "gui/$(id -u)/$_SL_SERVICE" 2>/dev/null)")"
    [ -n "$prev" ] && _sl_emit service.failed "service=$_SL_SERVICE" "reason=previous run ended abnormally" "$prev"
  fi
  local version; version="$(shasum -a 256 "$0" 2>/dev/null | cut -c1-12)"
  _sl_emit service.started "service=$_SL_SERVICE" "pid=$$" "version=${version:-unknown}"
  trap '_SL_STOP_REASON=SIGTERM; exit 0' TERM
  trap '_SL_STOP_REASON=SIGINT; exit 0' INT
  trap 'service_lifecycle_exit $?' EXIT
}

service_lifecycle_job() {  # service_lifecycle_job <label> "$@"
  # Runs the rest of the script as a child and reports its exit, so the job's
  # own EXIT traps and any `exec` at its end are left alone.
  shift  # the label is launchd's; the argument names the job for a reader
  # only a run launchd started is the service; an agent running the script by
  # hand is using a tool, and its failure is that agent's to report
  [ -n "$(_sl_launchd_label)" ] || return 0
  # sourced by another script (a test, a helper): $0 is the caller, not this job
  [ "${BASH_SOURCE[1]:-$0}" = "$0" ] || return 0
  "$BASH" "$0" "$@"
  local rc=$?
  if [ "$rc" != 0 ]; then
    _sl_emit service.failed "service=$XPC_SERVICE_NAME" "pid=$$" "reason=exited $rc" "exit_code=$rc"
  fi
  exit "$rc"
}
