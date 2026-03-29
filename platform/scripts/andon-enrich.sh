#!/bin/bash
# andon-enrich.sh — slow-path enrichment + Jeff intensity for andon light
# Runs every 30s via LaunchAgent (com.chorus.andon-enrich).
# Part 1: Queries board-ts, workflow.sh, brief dirs, struggling markers, chorus.log
#          Writes per-role JSON to /tmp/claude-team-scan/{role}-state.json
# Part 2: Aggregates prompt timestamps to measure Jeff's pacing + input signals
#          Writes /tmp/claude-team-scan/jeff-state.json
# Consolidation: #1311 — merged jeff-intensity.sh into this script (2026-03-11)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BOARD_TS="$SCRIPT_DIR/board-ts"
WORKFLOW="$SCRIPT_DIR/workflow.sh"
SCAN_DIR="/tmp/claude-team-scan"
TEMPO_LOG="$SCAN_DIR/tempo.log"
CHORUS_LOG="$SCRIPT_DIR/../logs/chorus.log"

mkdir -p "$SCAN_DIR"

# --- 1. Board snapshot (one call, shared across roles) ---
BOARD_OUTPUT=""
if [ -x "$BOARD_TS" ]; then
  BOARD_OUTPUT=$("$BOARD_TS" list 2>/dev/null || true)
fi

# --- Helper: extract first card for a role in a given bucket ---
# Board format: "   430  Title here [Silas|P2]"
# Bucket header: "BucketName (N):"
extract_role_card() {
  local role_cap="$1"  # Wren, Silas, Kade
  local bucket="$2"    # WIP, Blocked, SWAT, etc.

  local in_bucket=0
  while IFS= read -r line; do
    # Bucket header detection
    if echo "$line" | grep -qE '^[A-Z]'; then
      if echo "$line" | grep -qE "^${bucket} "; then
        in_bucket=1
      else
        in_bucket=0
      fi
      continue
    fi
    # Card line with role match
    if [ "$in_bucket" -eq 1 ]; then
      if echo "$line" | grep -qF "[${role_cap}|" || echo "$line" | grep -qF "[${role_cap}]"; then
        local card_id card_title
        card_id=$(echo "$line" | sed -E 's/^[[:space:]]+([0-9]+)[[:space:]]+.*/\1/')
        card_title=$(echo "$line" | sed -E 's/^[[:space:]]+[0-9]+[[:space:]]+(.*)\[.*/\1/' | sed 's/[[:space:]]*$//')
        echo "${card_id}|${card_title}"
        return 0
      fi
    fi
  done <<< "$BOARD_OUTPUT"
  return 1
}

# --- Helper: check if role has any card in a bucket ---
role_in_bucket() {
  local role_cap="$1"
  local bucket="$2"
  extract_role_card "$role_cap" "$bucket" >/dev/null 2>&1
}

# --- Helper: build JSON array from space-separated items ---
json_array() {
  local items="$1"
  if [ -z "$items" ]; then
    echo "[]"
    return
  fi
  local result="["
  local first=1
  for item in $items; do
    if [ "$first" -eq 1 ]; then
      first=0
    else
      result="${result},"
    fi
    result="${result}\"${item}\""
  done
  echo "${result}]"
}

# --- Helper: get JSONL directory for a role ---
jsonl_dir_for() {
  local role="$1"
  case "$role" in
    wren)  echo "$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-product-manager" ;;
    silas) echo "$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-architect" ;;
    kade)  echo "$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-engineer" ;;
    *)     return 1 ;;
  esac
}

# --- Helper: check JSONL mtime as session heartbeat ---
# Returns 0 (active) if most recent JSONL was written to in the last N seconds
jsonl_active() {
  local role="$1"
  local max_age="${2:-120}"
  local dir
  dir=$(jsonl_dir_for "$role") || return 1
  local latest
  latest=$(ls -t "${dir}"/*.jsonl 2>/dev/null | head -1)
  [ -z "$latest" ] && return 1
  local age=$(( $(date +%s) - $(stat -f%m "$latest" 2>/dev/null || echo 0) ))
  [ "$age" -lt "$max_age" ]
}

# --- 2. Per-role enrichment ---
ROLE_NAMES="wren silas kade"
ROLE_CAPS="Wren Silas Kade"
BRIEF_DIRS="product-manager/briefs architect/briefs engineer/briefs"
BASE_DIR="$SCRIPT_DIR/../.."

i=0
for role in $ROLE_NAMES; do
  i=$((i + 1))
  role_cap=$(echo "$ROLE_CAPS" | cut -d' ' -f"$i")
  brief_rel=$(echo "$BRIEF_DIRS" | cut -d' ' -f"$i")
  brief_dir="$BASE_DIR/$brief_rel"
  out_file="$SCAN_DIR/${role}-state.json"
  tmp_file="${out_file}.tmp"

  needs_jeff=""
  macrotask_signals=""

  # --- Needs-Jeff signals ---

  # SWAT card
  if role_in_bucket "$role_cap" "SWAT"; then
    needs_jeff="${needs_jeff} swat_card"
  fi

  # Blocked card
  if role_in_bucket "$role_cap" "Blocked"; then
    needs_jeff="${needs_jeff} blocked_card"
  fi

  # Struggling / deploy failed
  struggling_file="$SCAN_DIR/${role}.struggling"
  if [ -f "$struggling_file" ]; then
    age=$(( $(date +%s) - $(stat -f%m "$struggling_file" 2>/dev/null || echo 0) ))
    if [ "$age" -lt 90 ]; then
      needs_jeff="${needs_jeff} error_burst"
      # Also check for deploy failure in recent chorus.log
      if [ -f "$CHORUS_LOG" ] && tail -10 "$CHORUS_LOG" 2>/dev/null | grep -q "deploy.pipeline.failed"; then
        needs_jeff="${needs_jeff} deploy_failed"
      fi
    fi
  fi

  # --- Macrotask signals ---

  # Brief waiting: .md files newer than session-init marker
  init_marker="/tmp/claude-session-init/${role}.done"
  if [ -d "$brief_dir" ]; then
    brief_count=0
    if [ -f "$init_marker" ]; then
      brief_count=$(find "$brief_dir" -name '*.md' -newer "$init_marker" 2>/dev/null | wc -l | tr -d ' ')
    else
      # No session — check last 24h
      brief_count=$(find "$brief_dir" -name '*.md' -mmin -1440 2>/dev/null | wc -l | tr -d ' ')
    fi
    if [ "$brief_count" -gt 0 ]; then
      macrotask_signals="${macrotask_signals} brief_waiting"
    fi
  fi

  # Workflow step pending
  wf_output=$("$WORKFLOW" pending "$role" 2>/dev/null || true)
  if [ -n "$wf_output" ]; then
    macrotask_signals="${macrotask_signals} workflow_step"
  fi

  # --- WIP card extraction (with stale-card fallback) ---
  card_id=""
  card_title=""
  card_status=""

  wip_line=$(extract_role_card "$role_cap" "WIP" 2>/dev/null || true)
  if [ -n "$wip_line" ]; then
    card_id=$(echo "$wip_line" | cut -d'|' -f1)
    card_title=$(echo "$wip_line" | cut -d'|' -f2-)
    card_status="WIP"
  else
    now_line=$(extract_role_card "$role_cap" "Now" 2>/dev/null || true)
    if [ -n "$now_line" ]; then
      card_id=$(echo "$now_line" | cut -d'|' -f1)
      card_title=$(echo "$now_line" | cut -d'|' -f2-)
      card_status="Now"
    fi
  fi

  # Stale-card fallback: if board-ts returned no card but previous state had one
  # within the last 300s, keep the previous card. Prevents transient board-ts
  # failures (contention, lock timeout) from flipping Active → Waiting.
  if [ -z "$card_id" ] && [ -f "$out_file" ]; then
    prev_updated=$(python3 -c "import json,sys; d=json.load(open('$out_file')); print(d.get('updated',0))" 2>/dev/null || echo 0)
    prev_age=$(( $(date +%s) - prev_updated ))
    if [ "$prev_age" -lt 300 ]; then
      prev_card=$(python3 -c "import json,sys; d=json.load(open('$out_file')); c=d.get('card'); print(f\"{c['id']}\t{c['title']}\t{c['status']}\") if c else sys.exit(1)" 2>/dev/null || true)
      if [ -n "$prev_card" ]; then
        card_id=$(printf '%s' "$prev_card" | cut -d$'\t' -f1)
        card_title=$(printf '%s' "$prev_card" | cut -d$'\t' -f2)
        card_status=$(printf '%s' "$prev_card" | cut -d$'\t' -f3)
      fi
    fi
  fi

  # --- Declared state (authoritative when fresh <120s) ---
  declared_state=""
  declared_detail=""
  declared_gemba=""
  declared_file="$SCAN_DIR/${role}-declared.json"
  if [ -f "$declared_file" ]; then
    declared_ts=$(python3 -c "import json; print(json.load(open('$declared_file')).get('ts',0))" 2>/dev/null || echo 0)
    declared_age=$(( $(date +%s) - declared_ts ))
    if [ "$declared_age" -lt 120 ]; then
      declared_state=$(python3 -c "import json; print(json.load(open('$declared_file')).get('state',''))" 2>/dev/null || true)
      declared_detail=$(python3 -c "import json; print(json.load(open('$declared_file')).get('detail',''))" 2>/dev/null || true)
      declared_gemba=$(python3 -c "import json; print(json.load(open('$declared_file')).get('gemba',''))" 2>/dev/null || true)
    fi
  fi

  # --- Map declared state into signal arrays the Swift display reads ---
  if [ "$declared_state" = "blocked" ]; then
    needs_jeff="${needs_jeff} declared_blocked"
  elif [ "$declared_state" = "waiting" ]; then
    macrotask_signals="${macrotask_signals} declared_waiting"
  fi

  # --- Gemba observation active? ---
  # Priority: declared state "observing" with gemba target > legacy gemba file
  gemba_target=""
  if [ -n "$declared_gemba" ]; then
    gemba_target="$declared_gemba"
  else
    gemba_file="$SCAN_DIR/${role}-gemba.json"
    if [ -f "$gemba_file" ]; then
      age=$(( $(date +%s) - $(stat -f%m "$gemba_file" 2>/dev/null || echo 0) ))
      if [ "$age" -lt 300 ]; then
        gemba_target=$(python3 -c "import json; print(json.load(open('$gemba_file')).get('target',''))" 2>/dev/null || true)
      fi
    fi
  fi

  # --- JSONL heartbeat (active if written to in last 120s) ---
  jsonl_alive="false"
  if jsonl_active "$role" 120; then
    jsonl_alive="true"
  fi

  # --- Turn rate + last break (from prompt timestamps) ---
  turn_rate_min="0"
  last_break_ago_min="0"
  prompt_log="$SCAN_DIR/${role}-prompt-times.log"
  if [ -f "$prompt_log" ]; then
    now_ts=$(date +%s)
    # Get timestamps from last 30 min for turn rate
    recent_ts=$(awk -v cutoff="$((now_ts - 1800))" '$1 >= cutoff' "$prompt_log" 2>/dev/null)
    recent_count=$(echo "$recent_ts" | grep -c '[0-9]' 2>/dev/null) || recent_count=0
    if [ "$recent_count" -ge 2 ]; then
      # Average gap between consecutive prompts
      prev=""
      total_gap=0
      gap_count=0
      while IFS= read -r ts; do
        if [ -n "$prev" ]; then
          gap=$((ts - prev))
          total_gap=$((total_gap + gap))
          gap_count=$((gap_count + 1))
        fi
        prev=$ts
      done <<< "$recent_ts"
      if [ "$gap_count" -gt 0 ]; then
        avg_gap=$((total_gap / gap_count))
        turn_rate_min=$(python3 -c "print(f'{$avg_gap/60:.1f}')" 2>/dev/null || echo "0")
      fi
    fi
    # Find last break (gap > 5 min) — how many minutes ago it ended
    all_ts=$(cat "$prompt_log" 2>/dev/null)
    last_break_end=0
    prev=""
    while IFS= read -r ts; do
      if [ -n "$prev" ]; then
        gap=$((ts - prev))
        if [ "$gap" -ge 300 ]; then
          last_break_end=$ts
        fi
      fi
      prev=$ts
    done <<< "$all_ts"
    if [ "$last_break_end" -gt 0 ]; then
      last_break_ago_min=$(( (now_ts - last_break_end) / 60 ))
    else
      # No break found — use time since first prompt
      first_ts=$(head -1 "$prompt_log" 2>/dev/null | tr -d '[:space:]')
      if [ -n "$first_ts" ] && [ "$first_ts" -gt 0 ] 2>/dev/null; then
        last_break_ago_min=$(( (now_ts - first_ts) / 60 ))
      fi
    fi
  fi

  # --- Build JSON ---
  needs_jeff=$(echo "$needs_jeff" | xargs)  # trim
  macrotask_signals=$(echo "$macrotask_signals" | xargs)

  nj_json=$(json_array "$needs_jeff")
  mt_json=$(json_array "$macrotask_signals")

  card_json="null"
  if [ -n "$card_id" ]; then
    escaped_title=$(echo "$card_title" | sed 's/\\/\\\\/g; s/"/\\"/g')
    card_json="{\"id\":${card_id},\"title\":\"${escaped_title}\",\"status\":\"${card_status}\"}"
  fi

  gemba_json="null"
  if [ -n "$gemba_target" ]; then
    gemba_json="\"${gemba_target}\""
  fi

  declared_json="null"
  if [ -n "$declared_state" ]; then
    esc_detail=$(echo "$declared_detail" | sed 's/\\/\\\\/g; s/"/\\"/g')
    declared_json="{\"state\":\"${declared_state}\",\"detail\":\"${esc_detail}\"}"
  fi

  cat > "$tmp_file" <<ENDJSON
{"role":"${role}","updated":$(date +%s),"needs_jeff":${nj_json},"macrotask":${mt_json},"card":${card_json},"gemba":${gemba_json},"declared":${declared_json},"jsonl_alive":${jsonl_alive},"turn_rate_min":${turn_rate_min},"last_break_ago_min":${last_break_ago_min}}
ENDJSON

  mv "$tmp_file" "$out_file"

  # --- Compute andon state (mirrors Swift andon-light logic) ---
  andon_state="idle"
  if [ -n "$needs_jeff" ]; then
    andon_state="struggling"
  elif [ "$jsonl_alive" = "true" ] && [ -n "$card_id" ]; then
    andon_state="active"
  elif [ -n "$macrotask_signals" ]; then
    andon_state="waiting"
  fi
  # Declared state overrides when fresh
  if [ -n "$declared_state" ]; then
    case "$declared_state" in
      building|observing) andon_state="active" ;;
      blocked) andon_state="struggling" ;;
      waiting) andon_state="waiting" ;;
      idle) andon_state="idle" ;;
    esac
  fi

  # --- Emit role tempo to Loki-scrapable log ---
  TEMPO_LOG="$SCAN_DIR/tempo.log"
  TEMPO_TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
  printf '{"timestamp":"%s","participant":"%s","turn_rate_min":%s,"last_break_ago_min":%s,"jsonl_alive":%s,"card_status":"%s","andon":"%s"}\n' \
    "$TEMPO_TS" "$role" "${turn_rate_min:-0}" "${last_break_ago_min:-0}" "$jsonl_alive" "${card_status:-none}" "$andon_state" \
    >> "$TEMPO_LOG"
done

# =============================================================================
# Part 2: Jeff Intensity Monitor (was jeff-intensity.sh, inlined by #1311)
# =============================================================================

JEFF_OUT_FILE="$SCAN_DIR/jeff-state.json"
JEFF_TMP_FILE="${JEFF_OUT_FILE}.tmp"
NOW=$(date +%s)

# --- 3. Aggregate all prompt timestamps from all roles ---
ALL_TIMES=$(mktemp)
for role in wren silas kade; do
  log="$SCAN_DIR/${role}-prompt-times.log"
  if [ -f "$log" ]; then
    cat "$log" >> "$ALL_TIMES"
  fi
done

# Sort and deduplicate (same second across roles = one Jeff action)
sort -n "$ALL_TIMES" | uniq > "${ALL_TIMES}.sorted"
mv "${ALL_TIMES}.sorted" "$ALL_TIMES"

TOTAL=$(wc -l < "$ALL_TIMES" | tr -d ' ')

if [ "$TOTAL" -lt 2 ]; then
  # Not enough data
  cat > "$JEFF_TMP_FILE" <<ENDJSON
{"role":"jeff","updated":${NOW},"prompts_1h":0,"prompts_3h":0,"longest_break_min":0,"since_last_min":999,"intensity":"inactive","signal":"gray"}
ENDJSON
  mv "$JEFF_TMP_FILE" "$JEFF_OUT_FILE"
  rm -f "$ALL_TIMES"
  exit 0
fi

# --- 4. Compute metrics ---

# Prompts in last hour
HOUR_AGO=$((NOW - 3600))
PROMPTS_1H=$(awk -v cutoff="$HOUR_AGO" '$1 >= cutoff' "$ALL_TIMES" | wc -l | tr -d ' ')

# Prompts in last 3 hours
THREE_AGO=$((NOW - 10800))
PROMPTS_3H=$(awk -v cutoff="$THREE_AGO" '$1 >= cutoff' "$ALL_TIMES" | wc -l | tr -d ' ')

# Minutes since last prompt
LAST_TS=$(tail -1 "$ALL_TIMES")
SINCE_LAST_SEC=$((NOW - LAST_TS))
SINCE_LAST_MIN=$((SINCE_LAST_SEC / 60))

# --- 5. Break analysis (last 3 hours) ---
LONGEST_BREAK_SEC=0
PREV=""
while IFS= read -r ts; do
  if [ -n "$PREV" ] && [ "$ts" -ge "$THREE_AGO" ]; then
    gap=$((ts - PREV))
    if [ "$gap" -gt "$LONGEST_BREAK_SEC" ]; then
      LONGEST_BREAK_SEC=$gap
    fi
  fi
  PREV=$ts
done < "$ALL_TIMES"

if [ "$SINCE_LAST_SEC" -gt "$LONGEST_BREAK_SEC" ]; then
  LONGEST_BREAK_SEC=$SINCE_LAST_SEC
fi

LONGEST_BREAK_MIN=$((LONGEST_BREAK_SEC / 60))

# Count breaks > 15 min in last 3 hours
BREAK_COUNT=0
PREV=""
while IFS= read -r ts; do
  if [ -n "$PREV" ] && [ "$ts" -ge "$THREE_AGO" ]; then
    gap=$((ts - PREV))
    if [ "$gap" -ge 900 ]; then
      BREAK_COUNT=$((BREAK_COUNT + 1))
    fi
  fi
  PREV=$ts
done < "$ALL_TIMES"

# --- 6. Intensity classification ---
INTENSITY="green"
SIGNAL="green"

if [ "$SINCE_LAST_MIN" -ge 30 ]; then
  INTENSITY="away"
  SIGNAL="gray"
fi

# --- 7. Read input monitor data (keyboard + mouse + scroll) ---
INPUT_FILE="$SCAN_DIR/jeff-input.json"
KEYS_PER_MIN=0
CLICKS_PER_MIN=0
SCROLLS_PER_MIN=0
MOUSE_ACTIVE="false"
INPUT_ACTIVE="false"
if [ -f "$INPUT_FILE" ]; then
  input_updated=$(python3 -c "import json; print(json.load(open('$INPUT_FILE')).get('updated',0))" 2>/dev/null || echo 0)
  input_age=$((NOW - input_updated))
  if [ "$input_age" -lt 60 ]; then
    KEYS_PER_MIN=$(python3 -c "import json; print(json.load(open('$INPUT_FILE')).get('keys_per_min',0))" 2>/dev/null || echo 0)
    CLICKS_PER_MIN=$(python3 -c "import json; print(json.load(open('$INPUT_FILE')).get('clicks_per_min',0))" 2>/dev/null || echo 0)
    SCROLLS_PER_MIN=$(python3 -c "import json; print(json.load(open('$INPUT_FILE')).get('scrolls_per_min',0))" 2>/dev/null || echo 0)
    MOUSE_ACTIVE=$(python3 -c "import json; print(str(json.load(open('$INPUT_FILE')).get('mouse_active',False)).lower())" 2>/dev/null || echo "false")
    INPUT_ACTIVE="true"
  fi
fi

# --- 7a. Read posture data (from posture-capture.sh via LLaVA) ---
POSTURE="unknown"
TENSION="unknown"
MOOD="unknown"
ENERGY="unknown"
POSTURE_FRESH="false"
TODAY_DIR="/tmp/posture-timelapse/$(date '+%Y-%m-%d')"
SCORES_FILE="$TODAY_DIR/scores.jsonl"
if [ -f "$SCORES_FILE" ]; then
  scores_age=$(( NOW - $(stat -f%m "$SCORES_FILE" 2>/dev/null || echo 0) ))
  if [ "$scores_age" -lt 600 ]; then
    POSTURE=$(python3 -c "import json; print(json.loads(open('$SCORES_FILE').readlines()[-1]).get('posture','unknown'))" 2>/dev/null || echo "unknown")
    TENSION=$(python3 -c "import json; print(json.loads(open('$SCORES_FILE').readlines()[-1]).get('tension','unknown'))" 2>/dev/null || echo "unknown")
    MOOD=$(python3 -c "import json; print(json.loads(open('$SCORES_FILE').readlines()[-1]).get('mood','unknown'))" 2>/dev/null || echo "unknown")
    ENERGY=$(python3 -c "import json; print(json.loads(open('$SCORES_FILE').readlines()[-1]).get('energy','unknown'))" 2>/dev/null || echo "unknown")
    POSTURE_FRESH="true"
  fi
fi

# --- 7b. Prompt sentiment (classify Jeff's recent messages) ---
PROMPT_TYPE="none"
PROMPT_SENTIMENT="neutral"
PROMPT_AVG_LEN=0
PROMPT_SENTIMENT_FILE="$SCAN_DIR/jeff-prompt-sentiment.json"

python3 << 'PYSENTIMENT' > "${PROMPT_SENTIMENT_FILE}.tmp" 2>/dev/null || true
import json, glob, os, time

scan_dir = "/tmp/claude-team-scan"
now = int(time.time())
messages = []

for role in ["wren", "silas", "kade"]:
    log = f"{scan_dir}/{role}-prompt-times.log"
    if not os.path.exists(log):
        continue
    with open(log) as f:
        for line in f:
            ts = line.strip()
            if ts and ts.isdigit():
                age = now - int(ts)
                if age < 3600:
                    messages.append(int(ts))

if len(messages) < 2:
    print(json.dumps({"type": "none", "sentiment": "neutral", "avg_len": 0, "pace": "slow", "count_10m": 0}))
else:
    ten_min_ago = now - 600
    recent = [m for m in messages if m >= ten_min_ago]
    count_10m = len(recent)

    if count_10m >= 10:
        pace = "rapid"
    elif count_10m >= 5:
        pace = "steady"
    elif count_10m >= 1:
        pace = "slow"
    else:
        pace = "idle"

    messages.sort()
    gaps = [messages[i+1] - messages[i] for i in range(len(messages)-1) if messages[i] >= ten_min_ago]
    avg_gap = sum(gaps) / len(gaps) if gaps else 999

    if avg_gap < 15 and count_10m > 5:
        ptype = "re-prompt"
        sentiment = "negative"
    elif count_10m >= 8:
        ptype = "direction"
        sentiment = "neutral"
    elif pace == "slow":
        ptype = "ideation"
        sentiment = "positive"
    else:
        ptype = "direction"
        sentiment = "neutral"

    print(json.dumps({"type": ptype, "sentiment": sentiment, "avg_len": 0, "pace": pace, "count_10m": count_10m}))
PYSENTIMENT

if [ -f "${PROMPT_SENTIMENT_FILE}.tmp" ]; then
  mv "${PROMPT_SENTIMENT_FILE}.tmp" "$PROMPT_SENTIMENT_FILE"
  PROMPT_TYPE=$(python3 -c "import json; print(json.load(open('$PROMPT_SENTIMENT_FILE')).get('type','none'))" 2>/dev/null || echo "none")
  PROMPT_SENTIMENT=$(python3 -c "import json; print(json.load(open('$PROMPT_SENTIMENT_FILE')).get('sentiment','neutral'))" 2>/dev/null || echo "neutral")
fi

# --- 7c. Idle + break tracking (from actual input, not prompts) ---
IDLE_STATE_FILE="$SCAN_DIR/jeff-idle-state.json"
IDLE_DURATION_MIN=0
IDLE_START_TS=0
LAST_BREAK_START=0
LAST_BREAK_END=0
LAST_BREAK_DURATION_MIN=0
BREAK_THRESHOLD=120

if [ -f "$IDLE_STATE_FILE" ]; then
  IDLE_START_TS=$(python3 -c "import json; print(json.load(open('$IDLE_STATE_FILE')).get('idle_start_ts',0))" 2>/dev/null || echo 0)
  LAST_BREAK_START=$(python3 -c "import json; print(json.load(open('$IDLE_STATE_FILE')).get('last_break_start',0))" 2>/dev/null || echo 0)
  LAST_BREAK_END=$(python3 -c "import json; print(json.load(open('$IDLE_STATE_FILE')).get('last_break_end',0))" 2>/dev/null || echo 0)
  LAST_BREAK_DURATION_MIN=$(python3 -c "import json; print(json.load(open('$IDLE_STATE_FILE')).get('last_break_duration_min',0))" 2>/dev/null || echo 0)
fi

if [ "$INPUT_ACTIVE" = "true" ]; then
  keys_check=${KEYS_PER_MIN%.*}
  keys_check=${keys_check:-0}
  clicks_check=${CLICKS_PER_MIN%.*}
  clicks_check=${clicks_check:-0}

  # Prompt activity is a presence signal — if Jeff sent a prompt recently, he's not idle
  prompt_present=0
  if [ "$SINCE_LAST_MIN" -lt 5 ] && [ "$PROMPTS_1H" -gt 0 ]; then
    prompt_present=1
  fi

  if [ "$keys_check" -gt 0 ] || [ "$clicks_check" -gt 0 ] || [ "$prompt_present" -eq 1 ]; then
    if [ "$IDLE_START_TS" -gt 0 ]; then
      idle_was=$((NOW - IDLE_START_TS))
      if [ "$idle_was" -ge "$BREAK_THRESHOLD" ]; then
        LAST_BREAK_START=$IDLE_START_TS
        LAST_BREAK_END=$NOW
        LAST_BREAK_DURATION_MIN=$((idle_was / 60))
      fi
      IDLE_START_TS=0
    fi
    IDLE_DURATION_MIN=0
  else
    if [ "$IDLE_START_TS" -eq 0 ]; then
      IDLE_START_TS=$NOW
    fi
    IDLE_DURATION_MIN=$(( (NOW - IDLE_START_TS) / 60 ))
  fi
else
  IDLE_DURATION_MIN=0
fi

LAST_BREAK_TIME=""
if [ "$LAST_BREAK_START" -gt 0 ]; then
  LAST_BREAK_TIME=$(TZ=America/New_York date -r "$LAST_BREAK_START" '+%H:%M' 2>/dev/null || echo "")
fi

cat > "${IDLE_STATE_FILE}.tmp" <<IDLEJSON
{"idle_start_ts":${IDLE_START_TS},"last_break_start":${LAST_BREAK_START},"last_break_end":${LAST_BREAK_END},"last_break_duration_min":${LAST_BREAK_DURATION_MIN}}
IDLEJSON
mv "${IDLE_STATE_FILE}.tmp" "$IDLE_STATE_FILE"

if [ "$IDLE_DURATION_MIN" -ge 5 ] && [ "$INPUT_ACTIVE" = "true" ]; then
  INTENSITY="away"
  SIGNAL="gray"
fi

# --- 8. Behavioral mode (composite of all signals) ---
BEHAVIOR="unknown"
if [ "$INTENSITY" = "away" ]; then
  BEHAVIOR="away"
elif [ "$INPUT_ACTIVE" = "true" ]; then
  keys_bh=${KEYS_PER_MIN%.*}; keys_bh=${keys_bh:-0}
  clicks_bh=${CLICKS_PER_MIN%.*}; clicks_bh=${clicks_bh:-0}
  scrolls_bh=${SCROLLS_PER_MIN%.*}; scrolls_bh=${scrolls_bh:-0}

  if [ "$keys_bh" -gt 100 ] && [ "$clicks_bh" -gt 5 ]; then
    BEHAVIOR="coding"
  elif [ "$keys_bh" -gt 50 ] && [ "$clicks_bh" -le 5 ]; then
    BEHAVIOR="directing"
  elif [ "$keys_bh" -le 10 ] && { [ "$clicks_bh" -gt 5 ] || [ "$scrolls_bh" -gt 5 ]; }; then
    BEHAVIOR="reviewing"
  elif [ "$keys_bh" -le 10 ] && [ "$clicks_bh" -le 5 ] && [ "$MOUSE_ACTIVE" = "true" ]; then
    BEHAVIOR="thinking"
  elif [ "$keys_bh" -le 10 ] && [ "$MOUSE_ACTIVE" = "false" ]; then
    BEHAVIOR="paused"
  else
    BEHAVIOR="active"
  fi
fi

# --- 9. Rate x Sentiment composite ---
COMPOSITE="green"
INTENSITY_LABEL="steady"
HIGH_RATE=0
if [ "$PROMPTS_1H" -gt 15 ]; then
  HIGH_RATE=1
fi

if [ "$INTENSITY" = "away" ]; then
  COMPOSITE="gray"
  INTENSITY="away"
  INTENSITY_LABEL="away"
elif [ "$HIGH_RATE" -eq 1 ] && [ "$PROMPT_SENTIMENT" = "negative" ]; then
  COMPOSITE="red"
  INTENSITY="red"
  INTENSITY_LABEL="strain"
elif [ "$HIGH_RATE" -eq 0 ] && [ "$PROMPT_SENTIMENT" = "negative" ]; then
  COMPOSITE="yellow"
  INTENSITY="yellow"
  INTENSITY_LABEL="stuck"
elif [ "$HIGH_RATE" -eq 1 ] && { [ "$PROMPT_SENTIMENT" = "positive" ] || [ "$PROMPT_SENTIMENT" = "neutral" ]; }; then
  COMPOSITE="green"
  INTENSITY="green"
  INTENSITY_LABEL="flow"
elif [ "$HIGH_RATE" -eq 0 ]; then
  COMPOSITE="green"
  INTENSITY="green"
  INTENSITY_LABEL="reflective"
fi

# Posture tension escalation
if [ "$TENSION" = "high" ] && [ "$POSTURE_FRESH" = "true" ]; then
  if [ "$COMPOSITE" = "green" ]; then
    COMPOSITE="yellow"
    INTENSITY="yellow"
  elif [ "$COMPOSITE" = "yellow" ]; then
    COMPOSITE="red"
    INTENSITY="red"
  fi
fi

SIGNAL="$COMPOSITE"

# --- 10. Write Jeff state ---
cat > "$JEFF_TMP_FILE" <<ENDJSON
{"role":"jeff","updated":${NOW},"prompts_1h":${PROMPTS_1H},"prompts_3h":${PROMPTS_3H},"longest_break_min":${LONGEST_BREAK_MIN},"since_last_min":${SINCE_LAST_MIN},"break_count_3h":${BREAK_COUNT},"keys_per_min":${KEYS_PER_MIN},"clicks_per_min":${CLICKS_PER_MIN},"scrolls_per_min":${SCROLLS_PER_MIN},"mouse_active":${MOUSE_ACTIVE},"input_monitor":${INPUT_ACTIVE},"idle_duration_min":${IDLE_DURATION_MIN},"last_break_time":"${LAST_BREAK_TIME}","last_break_duration_min":${LAST_BREAK_DURATION_MIN},"posture":"${POSTURE}","tension":"${TENSION}","mood":"${MOOD}","energy":"${ENERGY}","posture_fresh":${POSTURE_FRESH},"prompt_type":"${PROMPT_TYPE}","prompt_sentiment":"${PROMPT_SENTIMENT}","behavior":"${BEHAVIOR}","composite":"${COMPOSITE}","intensity":"${INTENSITY}","signal":"${SIGNAL}"}
ENDJSON

mv "$JEFF_TMP_FILE" "$JEFF_OUT_FILE"
rm -f "$ALL_TIMES"

# --- 11. Jeff intensity history log ---
HISTORY_LOG="$SCAN_DIR/jeff-intensity-history.tsv"
if [ ! -f "$HISTORY_LOG" ]; then
  printf "timestamp\tprompts_1h\tprompts_3h\tsince_last_min\tlongest_break_min\tbreak_count_3h\tkeys_per_min\tclicks_per_min\tidle_min\tintensity\n" > "$HISTORY_LOG"
fi
printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
  "$NOW" "$PROMPTS_1H" "$PROMPTS_3H" "$SINCE_LAST_MIN" "$LONGEST_BREAK_MIN" "$BREAK_COUNT" "$KEYS_PER_MIN" "$CLICKS_PER_MIN" "$IDLE_DURATION_MIN" "$INTENSITY" \
  >> "$HISTORY_LOG"

# Rotate: keep last 7 days (~20k lines at 30s intervals)
MAX_LINES=20160
LINE_COUNT=$(wc -l < "$HISTORY_LOG" | tr -d ' ')
if [ "$LINE_COUNT" -gt "$MAX_LINES" ]; then
  TRIM=$((LINE_COUNT - MAX_LINES))
  tail -n +"$((TRIM + 1))" "$HISTORY_LOG" > "${HISTORY_LOG}.tmp"
  printf "timestamp\tprompts_1h\tprompts_3h\tsince_last_min\tlongest_break_min\tbreak_count_3h\tkeys_per_min\tclicks_per_min\tidle_min\tintensity\n" > "${HISTORY_LOG}.new"
  tail -n +2 "${HISTORY_LOG}.tmp" >> "${HISTORY_LOG}.new"
  mv "${HISTORY_LOG}.new" "$HISTORY_LOG"
  rm -f "${HISTORY_LOG}.tmp"
fi

# --- 12. Jeff tempo to Loki-scrapable log ---
JEFF_TEMPO_TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
printf '{"timestamp":"%s","participant":"jeff","prompts_1h":%d,"sentiment":"%s","behavior":"%s","composite":"%s","keys_per_min":%s,"idle_min":%d,"label":"%s"}\n' \
  "$JEFF_TEMPO_TS" "$PROMPTS_1H" "$PROMPT_SENTIMENT" "$BEHAVIOR" "$COMPOSITE" "$KEYS_PER_MIN" "$IDLE_DURATION_MIN" "$INTENSITY_LABEL" \
  >> "$TEMPO_LOG"

# Rotate tempo.log at 10K lines
TEMPO_LINES=$(wc -l < "$TEMPO_LOG" 2>/dev/null | tr -d ' ')
if [ "${TEMPO_LINES:-0}" -gt 10000 ]; then
  tail -7500 "$TEMPO_LOG" > "${TEMPO_LOG}.tmp" && mv "${TEMPO_LOG}.tmp" "$TEMPO_LOG"
fi
