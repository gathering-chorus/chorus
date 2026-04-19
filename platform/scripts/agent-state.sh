#!/bin/bash
# agent-state.sh — LaunchAgent lifecycle management
# Usage: agent-state.sh {status|start|stop|restart|orphans|health} [service-name]
#
# The app-state.sh equivalent for native LaunchAgent services.
# All process lifecycle goes through this script — no manual kill.

set -euo pipefail

LABEL_PREFIXES=("com.chorus" "com.gathering")
UID_NUM=$(id -u)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

FORCE=false

usage() {
  echo "Usage: agent-state.sh {status|start|stop|restart|orphans|health} [service-name] [--force]"
  echo ""
  echo "Commands:"
  echo "  status  [name]  — Show state of all agents or one agent"
  echo "  start   <name>  — Start/restart a LaunchAgent"
  echo "  stop    <name>  — Stop a LaunchAgent"
  echo "  restart <name>  — Stop then start"
  echo "  orphans         — Find and kill orphan processes (ppid=1) on known ports"
  echo "  health          — Summary: running, crashed, dead, duplicates"
  echo ""
  echo "Flags:"
  echo "  --force         — Non-interactive mode (kill orphans without prompting)"
  echo ""
  echo "Name matching: 'api' matches 'com.chorus.api', 'hooks' matches 'com.chorus.hooks'"
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

cmd_start() {
  local name="$1"
  local label
  label=$(resolve_label "$name")
  if [[ -z "$label" ]]; then
    echo "ERROR: No agent found matching '$name'"
    return 1
  fi
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
  else
    echo -e "${YELLOW}Started but no PID${NC} $label — may be a periodic agent"
  fi
}

cmd_stop() {
  local name="$1"
  local label
  label=$(resolve_label "$name")
  if [[ -z "$label" ]]; then
    echo "ERROR: No agent found matching '$name'"
    return 1
  fi
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
  else
    echo "$label was not running"
  fi
}

cmd_restart() {
  local name="$1"
  cmd_stop "$name"
  sleep 1
  cmd_start "$name"
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

  # Socket-based services (not on TCP ports) — scan by binary path
  local SOCKET_MAP=(
    "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hooks:hooks"
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
  status)  cmd_status "${ARGS[1]:-}" ;;
  start)   [[ -z "${ARGS[1]:-}" ]] && usage; cmd_start "${ARGS[1]}" ;;
  stop)    [[ -z "${ARGS[1]:-}" ]] && usage; cmd_stop "${ARGS[1]}" ;;
  restart) [[ -z "${ARGS[1]:-}" ]] && usage; cmd_restart "${ARGS[1]}" ;;
  orphans) cmd_orphans ;;
  health)  cmd_health ;;
  *)       usage ;;
esac
