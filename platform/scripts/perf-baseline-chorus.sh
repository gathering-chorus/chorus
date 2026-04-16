#!/usr/bin/env bash
# perf-baseline-chorus.sh — Response time baselines for Chorus services
# Card #1914 | Runs nightly alongside gathering-app baselines
#
# Measures: Chorus API, Clearing, Fuseki, Vikunja
# Output: structured lines to stdout + appends to perf-baseline-nightly.log

set -eo pipefail

LOG="$HOME/Library/Logs/Chorus/perf-baseline-nightly.log"
TIMESTAMP=$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S')

measure() {
  local name="$1" url="$2" threshold="$3"
  local start end latency status result

  start=$(date +%s%N 2>/dev/null || gdate +%s%N 2>/dev/null || echo 0)
  status=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$url" 2>/dev/null || echo "000")
  end=$(date +%s%N 2>/dev/null || gdate +%s%N 2>/dev/null || echo 0)

  if [ "$start" != "0" ] && [ "$end" != "0" ]; then
    latency=$(( (end - start) / 1000000 ))
  else
    latency=-1
  fi

  if [ "$status" = "000" ] || [ "$latency" -lt 0 ]; then
    result="DOWN"
  elif [ "$latency" -le "$threshold" ]; then
    result="pass"
  else
    result="FAIL"
  fi

  printf "  %-28s %4dms  %s\n" "$name" "$latency" "$result"
}

run_baselines() {
  echo "--- Chorus service baselines ($TIMESTAMP) ---"
  measure "chorus-api"  "http://localhost:3340/api/chorus/health"  500
  measure "clearing"    "http://localhost:3470/health"             500
  measure "fuseki"      "http://localhost:3030/$/ping"             2000
  measure "vikunja"     "http://localhost:3456/api/v1/info"        1000
}

# Print to stdout
run_baselines

# Append to nightly log
run_baselines >> "$LOG"
