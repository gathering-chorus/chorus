#!/usr/bin/env bash
# daily-signal-scan.sh — Daily signal integrity scan (#2088)
# Cron: 6am ET via LaunchAgent. Produces a brief with codebase weather,
# trust verification, signal-to-noise, doc freshness, backlog coherence,
# sequence health, and golfball detection.
set -euo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

export PATH="/opt/homebrew/bin:/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

REPO="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"
CHORUS="${CHORUS_ROOT}/chorus"
CARDS="$CHORUS/platform/scripts/cards"
CHORUS_LOG="$CHORUS/platform/scripts/chorus-log"
DECISIONS_MD="$CHORUS/roles/wren/decisions.md"
PROJECTS_MD="$CHORUS/roles/wren/projects.md"
PULSE_LOG="$HOME/Library/Logs/Gathering/hooks.log"
DATE=$(TZ=America/New_York date '+%Y-%m-%d')
OUTPUT="/tmp/daily-signal-${DATE}.md"
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --output) OUTPUT="$2"; shift 2 ;;
    *) echo "Usage: daily-signal-scan.sh [--dry-run] [--output path]" >&2; exit 1 ;;
  esac
done

log() { echo "$(TZ=America/New_York date '+%H:%M:%S') $*" >&2; }

# --- AC1: Codebase Weather ---
codebase_weather() {
  echo "## Codebase Weather"
  echo ""

  # Test count trend
  local test_count
  test_count=$(cd "$REPO" && npx jest --listTests 2>/dev/null | wc -l | tr -d ' ')
  echo "- **Test files:** ${test_count}"

  # Lint warning count
  local lint_warnings
  lint_warnings=$(cd "$REPO" && npx eslint . --ext .js,.ts --max-warnings=999 2>&1 | grep -oE '[0-9]+ warning' | grep -oE '[0-9]+' || echo "0")
  echo "- **Lint warnings:** ${lint_warnings}"

  # Recent git churn (files touched by 3+ commits in 7 days)
  echo "- **Hot files (3+ commits, 7d):**"
  cd "$REPO" && git log --since="7 days ago" --name-only --pretty=format: | sort | uniq -c | sort -rn | awk '$1 >= 3 && $2 != "" {print "  - " $2 " (" $1 " commits)"}' | head -10
  echo ""
}

# --- AC2: Trust Verification ---
trust_verification() {
  echo "## Trust Verification"
  echo ""

  # Gate enforcement from pulse log (trailing 24h)
  if [ -f "$PULSE_LOG" ]; then
    local yesterday
    yesterday=$(TZ=America/New_York date -v-1d '+%Y-%m-%d')
    local deny_count allow_count
    deny_count=$(awk -v cutoff="$yesterday" '$1 >= cutoff' "$PULSE_LOG" | awk -F' \\| ' '{gsub(/^ +| +$/, "", $6)} tolower($6) == "deny"' | wc -l | tr -d ' ')
    allow_count=$(awk -v cutoff="$yesterday" '$1 >= cutoff' "$PULSE_LOG" | awk -F' \\| ' '{gsub(/^ +| +$/, "", $6)} tolower($6) == "allow"' | wc -l | tr -d ' ')
    echo "- **Gate decisions (24h):** ${allow_count} allow, ${deny_count} deny"
    if [ "$deny_count" -gt 0 ]; then
      echo "- **Top denying hooks:**"
      awk -v cutoff="$yesterday" '$1 >= cutoff' "$PULSE_LOG" | awk -F' \\| ' '{gsub(/^ +| +$/, "", $5); gsub(/^ +| +$/, "", $6)} tolower($6) == "deny" {print $5}' | sort | uniq -c | sort -rn | head -5 | awk '{print "  - " $2 " (" $1 ")"}'
    fi
  else
    echo "- **Pulse log:** not found"
  fi

  # Deep health status
  local health_result
  health_result=$(bash "$CHORUS/platform/scripts/deep-health.sh" 2>&1 | head -1)
  echo "- **Deep health:** ${health_result}"
  echo ""
}

# --- AC3: Signal-to-Noise ---
signal_to_noise() {
  echo "## Signal-to-Noise"
  echo ""

  # Stolen prompts (session health data if available)
  echo "- **Session health alerts (24h):** $(grep -c "session-health" "$HOME/Library/Logs/Gathering/session-health.log" 2>/dev/null || echo "0")"

  # False alerts — deep-health failures that auto-resolved
  echo "- **Hook log size:** $(wc -l < "$PULSE_LOG" 2>/dev/null | tr -d ' ' || echo "0") lines"
  echo ""
}

# --- AC4: Doc Freshness ---
doc_freshness() {
  echo "## Doc Freshness"
  echo ""
  local now stale_threshold
  now=$(date +%s)
  stale_threshold=$((now - 259200)) # 3 days

  for doc in "$DECISIONS_MD" "$PROJECTS_MD"; do
    if [ -f "$doc" ]; then
      local mtime age_h name
      mtime=$(stat -f %m "$doc" 2>/dev/null || echo 0)
      age_h=$(( (now - mtime) / 3600 ))
      name=$(basename "$doc")
      if [ "$mtime" -lt "$stale_threshold" ]; then
        echo "- **${name}:** ${age_h}h old — STALE"
      else
        echo "- **${name}:** ${age_h}h old — fresh"
      fi
    fi
  done
  # Doc-catalog freshness per role
  echo "- **Doc freshness by role (>7d = stale):**"
  local now_ts role_dirs
  now_ts=$(date +%s)
  local stale_7d=$((now_ts - 604800))

  for role in Wren Silas Kade; do
    local stale_count=0 total_count=0 stale_files="" dirs=""
    case "$role" in
      Wren) dirs="$CHORUS/roles/wren $REPO/public/gathering-docs" ;;
      Silas) dirs="$CHORUS/roles/silas" ;;
      Kade) dirs="$CHORUS/roles/kade $REPO/src" ;;
    esac
    for dir in $dirs; do
      [ -d "$dir" ] || continue
      while IFS= read -r f; do
        [ -f "$f" ] || continue
        total_count=$((total_count + 1))
        local fmtime
        fmtime=$(stat -f %m "$f" 2>/dev/null || echo 0)
        if [ "$fmtime" -lt "$stale_7d" ]; then
          stale_count=$((stale_count + 1))
          local age_d=$(( (now_ts - fmtime) / 86400 ))
          stale_files="${stale_files}\n    - $(basename "$f") (${age_d}d)"
        fi
      done < <(find "$dir" -maxdepth 2 -name "*.md" -o -name "*.html" 2>/dev/null | head -50)
    done
    if [ "$stale_count" -gt 0 ]; then
      echo "  - **${role}:** ${stale_count}/${total_count} stale"
      echo -e "$stale_files" | head -5
    else
      echo "  - **${role}:** ${total_count} docs, all fresh"
    fi
  done
  echo ""
}

# --- AC5: Flow Health ---
flow_health() {
  echo "## Flow Health"
  echo ""

  # Parse status sections from board output — extract only lines under matching header
  local board_output
  board_output=$(bash "$CARDS" list 2>/dev/null)

  # Extract WIP section (lines between "WIP (N):" and next status header)
  local wip_section
  wip_section=$(echo "$board_output" | sed -n '/^WIP /,/^[A-Z]/p' | grep -E '^\s+\d+' | grep -iv '\[defect\]')
  local wip_count
  wip_count=$(echo "$wip_section" | grep -cE '^\s+\d+' || true)
  wip_count=${wip_count//[^0-9]/}
  : "${wip_count:=0}"
  echo "- **WIP:** ${wip_count} cards"
  echo "$wip_section" | while read -r line; do
    [ -n "$line" ] && echo "  - $line"
  done

  # Extract Next section
  local next_section
  next_section=$(echo "$board_output" | sed -n '/^Next /,/^[A-Z]/p' | grep -E '^\s+\d+' | grep -iv '\[defect\]')
  local next_count
  next_count=$(echo "$next_section" | grep -cE '^\s+\d+' || true)
  next_count=${next_count//[^0-9]/}
  : "${next_count:=0}"
  if [ "$next_count" -gt 0 ]; then
    echo "- **Next queue:** ${next_count} cards"
    echo "$next_section" | head -5 | while read -r line; do
      [ -n "$line" ] && echo "  - $line"
    done
  else
    echo "- **Next queue:** empty"
  fi
  echo ""
}


# --- AC7: Golfball Detection ---
golfball_detection() {
  echo "## Golfball Detection"
  echo ""
  echo "- **Reactive cards in active domains:**"

  # Find domains in WIP+Next with both new/enhance AND fix/swat cards
  local board_output
  board_output=$(bash "$CARDS" list 2>/dev/null)
  # Extract only WIP + Next sections
  local active_cards
  active_cards=$(echo "$board_output" | sed -n '/^WIP /,/^[A-Z]/p; /^Next /,/^[A-Z]/p' | grep -E '^\s+\d+')
  local domains_with_new
  domains_with_new=$(echo "$active_cards" | grep -E 'type:new|type:enhance' | grep -oE 'domain:\w+' | sort -u)
  for domain in $domains_with_new; do
    local fixes
    fixes=$(echo "$active_cards" | grep "$domain" | grep -E 'type:fix|type:swat' | grep -cE '^\s+\d+' || true)
    fixes=${fixes//[^0-9]/}
    : "${fixes:=0}"
    if [ "$fixes" -gt 0 ]; then
      echo "  - **${domain}:** ${fixes} fix/swat in WIP+Next alongside active development"
    fi
  done
  echo ""
}

# --- Main ---
main() {
  log "Starting daily signal scan"

  {
    echo "# Daily Signal — ${DATE}"
    echo ""
    echo "Generated: $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston"
    echo ""
    codebase_weather
    trust_verification
    signal_to_noise
    doc_freshness
    flow_health
    golfball_detection
    echo "---"
    echo "*Auto-generated by daily-signal-scan.sh (#2088)*"
  } > "$OUTPUT"

  log "Report written to $OUTPUT"

  # Post to Bridge if not dry run
  if [ "$DRY_RUN" = false ]; then
    local summary
    summary=$(head -20 "$OUTPUT" | tr '\n' ' ' | cut -c1-300)
    curl -s -X POST http://localhost:3470/api/message \
      -H 'Content-Type: application/json' \
      -d "{\"from\": \"kade\", \"text\": \"[daily-signal] ${DATE} report ready at ${OUTPUT}\"}" \
      > /dev/null 2>&1 || true
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" daily.signal.completed kade 2>/dev/null &
  fi
}

main
