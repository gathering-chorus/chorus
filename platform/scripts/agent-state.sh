#!/bin/bash
# agent-state.sh — LaunchAgent lifecycle management
# Usage: agent-state.sh {status|start|stop|restart|deploy|rollback|orphans|health} [service-name]
#
# The app-state.sh equivalent for native LaunchAgent services. All process
# lifecycle goes through this script — no manual kill, no manual deploy.
#
# Spine emit (#2605): every verb emits service.<verb>.{started,completed,failed}
# with role + service + PID + cdhash so "did the daemon pick up the new binary?"
# is a Loki query, not a triangulation. Same shape as build.completed (#2774).
#
# Exit codes:
#   0  success
#   1  work failure (resolve-fail, kickstart-fail, deploy-fail, verify-fail)
#   2  usage error (unknown verb, missing arg, unknown service)

set -euo pipefail

LABEL_PREFIXES=("com.chorus" "com.gathering")
UID_NUM=$(id -u)
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# Spine emit helper (#2605). Best-effort — failure of chorus-log must never
# affect the verb's outcome. Mirrors the chorus-build / chorus-deploy pattern.
SCRIPT_DIR_ASTATE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# CHORUS_LOG_BIN resolution: env override → PATH lookup → script-dir sibling.
# Env override lets tests stub chorus-log without symlinking it next to this script.
CHORUS_LOG_BIN="${CHORUS_LOG_BIN:-$(command -v chorus-log || echo "$SCRIPT_DIR_ASTATE/chorus-log")}"

spine_emit() {
  local event="$1"; shift
  if [ -x "$CHORUS_LOG_BIN" ]; then
    "$CHORUS_LOG_BIN" "$event" "$ROLE" "$@" >/dev/null 2>&1 || true
  fi
}

# Read the cdhash of a running daemon's binary, or empty if not running.
# Used for the deployed-equals-running assertion (#2605).
running_cdhash() {
  local label="$1"
  local pid
  pid=$(launchctl list "$label" 2>/dev/null | grep '"PID"' | grep -o '[0-9]*' || true)
  if [ -z "$pid" ]; then echo ""; return; fi
  local bin_path
  # #3662 — `|| true`: awk's early `exit` SIGPIPEs lsof when the target holds
  # more fds than the pipe buffer absorbs (the wedged nightly bash did), and
  # under set -euo pipefail the whole verb died rc=141 mid-stop with the value
  # already captured. Size-dependent, so small services never showed it.
  bin_path=$(lsof -p "$pid" 2>/dev/null | awk '$4=="txt" {print $NF; exit}' || true)
  if [ -z "$bin_path" ] || [ ! -e "$bin_path" ]; then echo ""; return; fi
  codesign -d --verbose=4 "$bin_path" 2>&1 | awk -F'=' '/^CDHash/{print $2; exit}' || true
}

# Read the cdhash of an installed binary (without running).
installed_cdhash() {
  local installed_path="$1"
  if [ ! -e "$installed_path" ]; then echo ""; return; fi
  codesign -d --verbose=4 "$installed_path" 2>&1 | awk -F'=' '/^CDHash/{print $2; exit}'
}

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

FORCE=false

usage() {
  echo "Usage: agent-state.sh {status|start|stop|restart|deploy|rollback|orphans|health} [service-name] [--force]"
  echo ""
  echo "Commands:"
  echo "  status   [name]  — Show state of all agents or one agent"
  echo "  start    <name>  — Start/restart a LaunchAgent"
  echo "  stop     <name>  — Stop a LaunchAgent"
  echo "  restart  <name>  — Stop then start, verify running cdhash matches installed"
  echo "  deploy   <crate> — werk-deploy crate <crate> + kickstart + cdhash verify (#2605/#3317)"
  echo "  rollback <crate> — Reinstall prior cdhash from manifest, kickstart, verify (#2605)"
  echo "  orphans          — Find and kill orphan processes (ppid=1) on known ports"
  echo "  health           — Summary: running, crashed, dead, duplicates"
  echo ""
  echo "Flags:"
  echo "  --force          — Non-interactive mode (kill orphans without prompting)"
  echo ""
  echo "Name matching: 'api' matches 'com.chorus.api', 'hooks' matches 'com.chorus.hooks'"
  echo ""
  echo "Spine events (#2605): every lifecycle verb emits service.<verb>.{started,completed,failed}"
  echo "  with {service, role, pre_pid, post_pid, pre_cdhash, post_cdhash}."
  exit 1
}

# Resolve a short name to a full launchd label
resolve_label() {
  local name="$1"
  # If it's already a full label, use it
  if launchctl list "$name" &>/dev/null; then
    echo "$name"
    return
  fi
  # Try prefixes
  for prefix in "${LABEL_PREFIXES[@]}"; do
    local candidate="$prefix.$name"
    if launchctl list "$candidate" &>/dev/null; then
      echo "$candidate"
      return
    fi
  done
  # Try removing prefix hints: "chorus-api" -> "com.chorus.api"
  local stripped="${name#chorus-}"
  stripped="${stripped#gathering-}"
  for prefix in "${LABEL_PREFIXES[@]}"; do
    local candidate="$prefix.$stripped"
    if launchctl list "$candidate" &>/dev/null; then
      echo "$candidate"
      return
    fi
  done
  echo ""
}

cmd_status() {
  local filter="${1:-}"
  local running=0 crashed=0 dead=0 total=0

  printf "%-45s %8s %6s %s\n" "AGENT" "PID" "EXIT" "STATE"
  printf "%-45s %8s %6s %s\n" "-----" "---" "----" "-----"

  while IFS=$'\t' read -r pid exit_code label; do
    # Filter if name given
    if [[ -n "$filter" ]] && [[ "$label" != *"$filter"* ]]; then
      continue
    fi

    total=$((total + 1))
    local state=""
    if [[ "$pid" != "-" ]]; then
      if [[ "$exit_code" != "0" && "$exit_code" != "-" ]]; then
        state="${RED}CRASHED${NC}"
        crashed=$((crashed + 1))
      else
        state="${GREEN}RUNNING${NC}"
        running=$((running + 1))
      fi
    else
      state="${YELLOW}DEAD${NC}"
      dead=$((dead + 1))
    fi

    printf "%-45s %8s %6s $(echo -e "$state")\n" "$label" "$pid" "$exit_code"
  done < <(launchctl list | grep -E 'com\.(chorus|gathering)\.' | sort -t$'\t' -k3)

  echo ""
  echo -e "Total: $total | ${GREEN}Running: $running${NC} | ${RED}Crashed: $crashed${NC} | ${YELLOW}Dead: $dead${NC}"
}

# Map a crate name to its launchd label.
crate_to_label() {
  case "$1" in
    chorus-api)   echo "com.chorus.api" ;;
    chorus-hooks) echo "com.chorus.hooks" ;;
    chorus-inject) echo "com.chorus.hooks" ;;
    *)            echo "" ;;
  esac
}

# Get running PID of a launchd label, or empty.
label_pid() {
  # #3606: `|| true` — under set -euo pipefail, a not-running (periodic) agent
  # makes this grep exit 1, which silently KILLED cmd_start/cmd_restart before
  # any output or kickstart (test-agent-state Test 6's red; also why
  # `agent-state.sh restart clearing` no-oped on 2026-07-03).
  launchctl list "$1" 2>/dev/null | grep '"PID"' | grep -o '[0-9]*' || true
}

cmd_start() {
  local name="$1"
  local label
  label=$(resolve_label "$name")
  if [[ -z "$label" ]]; then
    echo "ERROR: No agent found matching '$name'"
    spine_emit service.start.failed "service=$name" "reason=service-not-found"
    return 1
  fi
  local pre_pid pre_cdhash
  pre_pid=$(label_pid "$label")
  pre_cdhash=$(running_cdhash "$label")
  spine_emit service.start.started "service=$label" "pre_pid=$pre_pid" "pre_cdhash=$pre_cdhash"
  echo "Starting $label..."
  # Clear suppress markers immediately — a failed start should not leave alerts dark
  rm -f "/tmp/deploy-in-progress-${label}.marker" "/tmp/chorus-alert-suppress"
  launchctl kickstart -k "gui/$UID_NUM/$label" 2>&1
  sleep 1
  local info
  info=$(launchctl list "$label" 2>/dev/null)
  local pid
  pid=$(echo "$info" | grep '"PID"' | grep -o '[0-9]*')
  if [[ -n "$pid" ]]; then
    echo -e "${GREEN}Started${NC} $label (PID $pid)"
    spine_emit service.start.completed "service=$label" "pre_pid=$pre_pid" "post_pid=$pid" "post_cdhash=$(running_cdhash "$label")"
  else
    echo -e "${YELLOW}Started but no PID${NC} $label — may be a periodic agent"
    spine_emit service.start.completed "service=$label" "pre_pid=$pre_pid" "post_pid=" "note=periodic-agent-no-pid"
  fi
}

cmd_stop() {
  local name="$1"
  local label
  label=$(resolve_label "$name")
  if [[ -z "$label" ]]; then
    echo "ERROR: No agent found matching '$name'"
    spine_emit service.stop.failed "service=$name" "reason=service-not-found"
    return 1
  fi
  local pre_pid pre_cdhash
  pre_pid=$(label_pid "$label")
  pre_cdhash=$(running_cdhash "$label")
  spine_emit service.stop.started "service=$label" "pre_pid=$pre_pid" "pre_cdhash=$pre_cdhash"
  echo "Stopping $label..."
  # Write deploy suppression marker — prevents deep-health alerts during restart window
  echo $(( $(date +%s) + 90 )) > "/tmp/chorus-alert-suppress"
  touch "/tmp/deploy-in-progress-${label}.marker"
  local pid
  pid=$(launchctl list "$label" 2>/dev/null | grep '"PID"' | grep -o '[0-9]*')
  if [[ -n "$pid" ]]; then
    launchctl bootout "gui/$UID_NUM/$label" 2>/dev/null || true
    local plist
    plist=$(find "$HOME/Library/LaunchAgents" -name "${label}.plist" 2>/dev/null | head -1)
    if [[ -n "$plist" ]]; then
      launchctl bootstrap "gui/$UID_NUM" "$plist" 2>/dev/null || true
    fi
    echo -e "${GREEN}Stopped${NC} $label (was PID $pid)"
    spine_emit service.stop.completed "service=$label" "pre_pid=$pre_pid" "post_pid="
  else
    echo "$label was not running"
    spine_emit service.stop.completed "service=$label" "pre_pid=" "note=already-stopped"
  fi
}

cmd_restart() {
  local name="$1"
  local label
  label=$(resolve_label "$name")
  if [[ -z "$label" ]]; then
    echo "ERROR: No agent found matching '$name'"
    spine_emit service.restart.failed "service=$name" "reason=service-not-found"
    return 1
  fi
  local pre_pid pre_cdhash
  pre_pid=$(label_pid "$label")
  pre_cdhash=$(running_cdhash "$label")
  spine_emit service.restart.started "service=$label" "pre_pid=$pre_pid" "pre_cdhash=$pre_cdhash"
  cmd_stop "$name"
  sleep 1
  cmd_start "$name"
  local post_pid post_cdhash
  post_pid=$(label_pid "$label")
  post_cdhash=$(running_cdhash "$label")
  # cdhash verify: running cdhash should match installed (still the same binary)
  if [[ -n "$post_cdhash" && -n "$pre_cdhash" && "$post_cdhash" != "$pre_cdhash" ]]; then
    echo -e "${YELLOW}WARN${NC} cdhash changed across restart: $pre_cdhash → $post_cdhash"
    spine_emit service.verify.divergence "service=$label" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash"
  fi
  spine_emit service.restart.completed "service=$label" "pre_pid=$pre_pid" "post_pid=$post_pid" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash"
}

# cmd_deploy <crate> — werk-deploy crate <crate> + kickstart + cdhash verify.
# #3317: the deploy engine is the Rust verb (absorbed bash chorus-deploy).
# Production code path for #2605 AC3.
cmd_deploy() {
  local crate="$1"
  local label
  label=$(crate_to_label "$crate")
  if [[ -z "$label" ]]; then
    echo "ERROR: Unknown crate '$crate' — known: chorus-api, chorus-hooks, chorus-inject"
    spine_emit service.deploy.failed "service=$crate" "step=resolve" "reason=service-not-found" "exit_code=2"
    return 2
  fi
  local pre_pid pre_cdhash
  pre_pid=$(label_pid "$label")
  pre_cdhash=$(running_cdhash "$label")
  spine_emit service.deploy.started "service=$label" "crate=$crate" "pre_pid=$pre_pid" "pre_cdhash=$pre_cdhash"
  echo "Deploying $crate ($label)..."
  local werk_deploy="${CHORUS_WERK_DEPLOY_BIN:-$(command -v werk-deploy || echo "$HOME/.chorus/bin/werk-deploy")}"
  if [[ ! -x "$werk_deploy" ]]; then
    echo "ERROR: werk-deploy not executable at $werk_deploy"
    spine_emit service.deploy.failed "service=$label" "crate=$crate" "step=resolve" "reason=werk-deploy-missing" "exit_code=1"
    return 1
  fi
  if ! CHORUS_HOME="${CHORUS_HOME:-$SCRIPT_DIR_ASTATE/../..}" "$werk_deploy" crate "$crate"; then
    echo -e "${RED}werk-deploy failed${NC}"
    spine_emit service.deploy.failed "service=$label" "crate=$crate" "step=werk-deploy" "reason=build-fail" "exit_code=1"
    return 1
  fi
  if ! launchctl kickstart -k "gui/$UID_NUM/$label" 2>&1; then
    echo -e "${RED}kickstart failed${NC}"
    spine_emit service.deploy.failed "service=$label" "crate=$crate" "step=kickstart" "reason=kickstart-fail" "exit_code=1"
    return 1
  fi
  sleep 2
  local post_pid post_cdhash
  post_pid=$(label_pid "$label")
  post_cdhash=$(running_cdhash "$label")
  if [[ -n "$pre_cdhash" && "$pre_cdhash" == "$post_cdhash" ]]; then
    echo -e "${YELLOW}WARN${NC} running cdhash unchanged after deploy — daemon may not have picked up new binary"
    spine_emit service.deploy.failed "service=$label" "crate=$crate" "step=verify" "reason=cdhash-divergence" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash" "exit_code=1"
    return 1
  fi
  echo -e "${GREEN}Deployed${NC} $crate → $label (PID $pre_pid → $post_pid, cdhash $pre_cdhash → $post_cdhash)"
  spine_emit service.deploy.completed "service=$label" "crate=$crate" "pre_pid=$pre_pid" "post_pid=$post_pid" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash"
}

# cmd_rollback <crate> — invoke werk-deploy crate <crate> --rollback + kickstart + verify.
cmd_rollback() {
  local crate="$1"
  local label
  label=$(crate_to_label "$crate")
  if [[ -z "$label" ]]; then
    echo "ERROR: Unknown crate '$crate' — known: chorus-api, chorus-hooks, chorus-inject"
    spine_emit service.rollback.failed "service=$crate" "step=resolve" "reason=service-not-found" "exit_code=2"
    return 2
  fi
  local pre_pid pre_cdhash
  pre_pid=$(label_pid "$label")
  pre_cdhash=$(running_cdhash "$label")
  spine_emit service.rollback.started "service=$label" "crate=$crate" "pre_pid=$pre_pid" "pre_cdhash=$pre_cdhash" "target_cdhash=prior"
  echo "Rolling back $crate ($label)..."
  local werk_deploy="${CHORUS_WERK_DEPLOY_BIN:-$(command -v werk-deploy || echo "$HOME/.chorus/bin/werk-deploy")}"
  if ! CHORUS_HOME="${CHORUS_HOME:-$SCRIPT_DIR_ASTATE/../..}" "$werk_deploy" crate "$crate" --rollback; then
    echo -e "${RED}rollback failed${NC}"
    spine_emit service.rollback.failed "service=$label" "crate=$crate" "step=restore" "reason=no-prior-cdhash" "exit_code=1"
    return 1
  fi
  if ! launchctl kickstart -k "gui/$UID_NUM/$label" 2>&1; then
    echo -e "${RED}kickstart after rollback failed${NC}"
    spine_emit service.rollback.failed "service=$label" "crate=$crate" "step=kickstart" "reason=kickstart-fail" "exit_code=1"
    return 1
  fi
  sleep 2
  local post_pid post_cdhash
  post_pid=$(label_pid "$label")
  post_cdhash=$(running_cdhash "$label")
  if [[ -n "$pre_cdhash" && "$pre_cdhash" == "$post_cdhash" ]]; then
    echo -e "${YELLOW}WARN${NC} running cdhash unchanged after rollback"
    spine_emit service.rollback.failed "service=$label" "crate=$crate" "step=verify" "reason=cdhash-divergence" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash" "exit_code=1"
    return 1
  fi
  echo -e "${GREEN}Rolled back${NC} $crate → $label (cdhash $pre_cdhash → $post_cdhash)"
  spine_emit service.rollback.completed "service=$label" "crate=$crate" "pre_pid=$pre_pid" "post_pid=$post_pid" "pre_cdhash=$pre_cdhash" "post_cdhash=$post_cdhash"
}

cmd_orphans() {
  echo "Scanning for orphan processes (ppid=1) on known service ports..."
  echo ""

  # bash-3 compatible: plain indexed array with port:name pairs (macOS default bash is 3.2)
  local PORT_MAP=(
    "3340:chorus-api"
    "3456:vikunja"
    "3100:grafana"
    "3102:loki"
    "3470:bridge"
    "3475:messaging"
    "9090:prometheus"
    "9100:node-exporter"
    "3030:fuseki"
  )

  local found=0
  local entry port service
  for entry in "${PORT_MAP[@]}"; do
    port="${entry%%:*}"
    service="${entry##*:}"
    while read -r pid; do
      if [[ -z "$pid" ]]; then continue; fi
      local ppid
      ppid=$(ps -p "$pid" -o ppid= 2>/dev/null | tr -d ' ')
      if [[ "$ppid" == "1" ]]; then
        # Skip if managed by launchd — ppid=1 is normal for LaunchAgent processes
        local bin
        bin=$(ps -p "$pid" -o command= 2>/dev/null | awk '{print $1}')
        if launchctl list | grep -q "$service" 2>/dev/null; then
          continue
        fi
        local cmd
        cmd=$(ps -p "$pid" -o command= 2>/dev/null | head -c 80)
        echo -e "${RED}ORPHAN${NC} port=$port service=$service pid=$pid"
        echo "  cmd: $cmd"
        found=$((found + 1))

        if [[ "$FORCE" == "true" ]]; then
          kill "$pid" 2>/dev/null && echo -e "  ${GREEN}Killed${NC}" || echo -e "  ${RED}Failed${NC}"
        elif [[ -t 0 ]]; then
          read -r -p "  Kill PID $pid? [y/N] " confirm
          if [[ "$confirm" =~ ^[Yy]$ ]]; then
            kill "$pid" 2>/dev/null && echo -e "  ${GREEN}Killed${NC}" || echo -e "  ${RED}Failed${NC}"
          fi
        else
          echo "  (non-interactive — use --force to kill)"
        fi
      fi
    done < <(lsof -ti :"$port" 2>/dev/null)
  done

  # Socket-based services (not on TCP ports) — scan by binary path.
  # #2823: also check ~/.chorus/bin/chorus-hooks (canonical deploy location per
  # #2734); pre-#2823 only the build-cache path was checked, missing orphans
  # launched from the deploy location.
  local SOCKET_MAP=(
    "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hooks:hooks"
    "/Users/jeffbridwell/.chorus/bin/chorus-hooks:hooks"
  )
  local bin name
  for entry in "${SOCKET_MAP[@]}"; do
    bin="${entry%%:*}"
    name="${entry##*:}"
    while read -r pid; do
      [[ -z "$pid" ]] && continue
      local ppid
      ppid=$(ps -p "$pid" -o ppid= 2>/dev/null | tr -d ' ')
      if [[ "$ppid" == "1" ]]; then
        # Skip if managed by launchd — ppid=1 is normal for LaunchAgent processes
        if launchctl list | grep -q "com.chorus.${name}" 2>/dev/null; then
          continue
        fi
        echo -e "${RED}ORPHAN${NC} socket-service=$name pid=$pid"
        found=$((found + 1))
        if [[ "$FORCE" == "true" ]]; then
          kill "$pid" 2>/dev/null && echo -e "  ${GREEN}Killed${NC}" || echo -e "  ${RED}Failed${NC}"
        fi
      fi
    done < <(pgrep -f "$bin$" 2>/dev/null)
  done

  if [[ $found -eq 0 ]]; then
    echo -e "${GREEN}No orphans found${NC}"
  fi
}

cmd_health() {
  echo "=== LaunchAgent Health ==="
  echo ""

  local running crashed dead total
  running=$(launchctl list | grep -E 'com\.(chorus|gathering)\.' | awk '$1 != "-"' | wc -l | tr -d ' ')
  crashed=$(launchctl list | grep -E 'com\.(chorus|gathering)\.' | awk '$1 != "-" && $2 != 0 && $2 != "-"' | wc -l | tr -d ' ')
  dead=$(launchctl list | grep -E 'com\.(chorus|gathering)\.' | awk '$1 == "-"' | wc -l | tr -d ' ')
  total=$(launchctl list | grep -E 'com\.(chorus|gathering)\.' | wc -l | tr -d ' ')

  echo -e "Agents: $total total | ${GREEN}$running running${NC} | ${RED}$crashed crashed${NC} | ${YELLOW}$dead stopped${NC}"
  echo ""

  if [[ "$crashed" -gt 0 ]]; then
    echo -e "${RED}Crashed:${NC}"
    launchctl list | grep -E 'com\.(chorus|gathering)\.' | awk '$1 != "-" && $2 != 0 && $2 != "-" {print "  " $3 " (exit " $2 ")"}'
    echo ""
  fi

  echo "Duplicates across namespaces:"
  local dupes
  dupes=$(launchctl list | grep -E 'com\.(chorus|gathering)\.' | awk '{print $3}' | sed 's/com\.\(chorus\|gathering\)\.//' | sort | uniq -d)
  if [[ -n "$dupes" ]]; then
    echo -e "${YELLOW}$dupes${NC}" | sed 's/^/  /'
  else
    echo "  (none)"
  fi

  echo ""
  echo "Critical services:"
  for svc in com.chorus.api com.chorus.hooks com.chorus.clearing com.gathering.fuseki com.gathering.app com.gathering.vikunja com.gathering.loki com.gathering.grafana; do
    local info
    info=$(launchctl list "$svc" 2>/dev/null)
    local pid
    pid=$(echo "$info" | grep '"PID"' | grep -o '[0-9]*')
    if [[ -n "$pid" ]]; then
      echo -e "  ${GREEN}●${NC} $svc (PID $pid)"
    else
      echo -e "  ${RED}●${NC} $svc (not running)"
    fi
  done
}

# Parse --force from any position
ARGS=()
for arg in "$@"; do
  if [[ "$arg" == "--force" ]]; then
    FORCE=true
  else
    ARGS+=("$arg")
  fi
done

cmd="${ARGS[0]:-}"

case "$cmd" in
  status)   cmd_status "${ARGS[1]:-}" ;;
  start)    [[ -z "${ARGS[1]:-}" ]] && usage; cmd_start "${ARGS[1]}" ;;
  stop)     [[ -z "${ARGS[1]:-}" ]] && usage; cmd_stop "${ARGS[1]}" ;;
  restart)  [[ -z "${ARGS[1]:-}" ]] && usage; cmd_restart "${ARGS[1]}" ;;
  deploy)   [[ -z "${ARGS[1]:-}" ]] && usage; cmd_deploy "${ARGS[1]}" ;;
  rollback) [[ -z "${ARGS[1]:-}" ]] && usage; cmd_rollback "${ARGS[1]}" ;;
  orphans)  cmd_orphans ;;
  health)   cmd_health ;;
  *)        usage ;;
esac
