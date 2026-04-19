#!/usr/bin/env bash
# #2207 — Nightly coverage regression signal.
#
# Runs at 02:00 via com.chorus.nightly-coverage.plist. Reads thresholds
# from coverage-floors.yml (Jeff-authored), invokes standard tools directly
# (no wrapper aggregation), posts a Bridge summary with per-service %s.
#
# Exits 0 always — the LaunchAgent must keep scheduling regardless of result.
# Bridge is the signal channel, not the exit code.
#
# Dry-run mode (NIGHTLY_COVERAGE_DRY_RUN=1): reads pre-baked JSON from
# NIGHTLY_COVERAGE_FIXTURES/<project>/coverage/coverage-summary.json (TS)
# and NIGHTLY_COVERAGE_FIXTURES/<crate>/llvm-cov-summary.json (Rust).

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FLOORS_FILE="${NIGHTLY_COVERAGE_FLOORS:-${CHORUS_ROOT}/coverage-floors.yml}"
BRIDGE_NUDGE_URL="${BRIDGE_NUDGE_URL:-http://localhost:3475/api/nudge}"
DRY_RUN="${NIGHTLY_COVERAGE_DRY_RUN:-}"
FIXTURES="${NIGHTLY_COVERAGE_FIXTURES:-}"

LOG_DIR="$HOME/Library/Logs/Chorus"
LOG="$LOG_DIR/nightly-coverage.log"
mkdir -p "$LOG_DIR"

ts() { TZ=America/New_York date '+%Y-%m-%d %H:%M:%S'; }
log() { echo "[$(ts)] $*" | tee -a "$LOG"; }

# Parse coverage-floors.yml without yaml module (pure Python re).
# Outputs: "ts <rel-path> <floor>" or "rust <rel-path> <floor>" per line.
parse_floors() {
  python3 - "$FLOORS_FILE" <<'PYEOF'
import re, sys
text = open(sys.argv[1]).read()
section = None
for line in text.splitlines():
    m_section = re.match(r'^(ts|rust):\s*$', line)
    m_entry   = re.match(r'^\s{2}(\S[^:]+):\s+(\d+)', line)
    if m_section:
        section = m_section.group(1)
    elif m_entry and section:
        print(f"{section} {m_entry.group(1).strip()} {m_entry.group(2)}")
PYEOF
}

# Read statements.pct from a jest coverage-summary.json.
read_jest_pct() {
  local summary_json="$1"
  python3 -c "
import json, sys
d = json.load(open('$summary_json'))
print(d['total']['statements']['pct'])
" 2>/dev/null
}

# Read lines.percent from a cargo llvm-cov --summary-only --json output.
read_llvm_pct() {
  local summary_json="$1"
  python3 -c "
import json, sys
d = json.load(open('$summary_json'))
print(d['data'][0]['totals']['lines']['percent'])
" 2>/dev/null
}

# Run jest coverage for a TS project; return coverage-summary.json path.
run_ts_coverage() {
  local proj_rel="$1"
  local proj_dir="${CHORUS_ROOT}/${proj_rel}"
  [ -d "$proj_dir" ] || { log "  WARN: TS project dir missing: $proj_dir"; return 1; }
  (cd "$proj_dir" && npx jest --coverage --coverageReporters=json-summary \
     --passWithNoTests --silent 2>/dev/null || true)
  echo "${proj_dir}/coverage/coverage-summary.json"
}

# Run cargo llvm-cov for a Rust crate; return JSON path.
run_rust_coverage() {
  local crate_rel="$1"
  local crate_dir="${CHORUS_ROOT}/${crate_rel}"
  [ -d "$crate_dir" ] || { log "  WARN: Rust crate dir missing: $crate_dir"; return 1; }
  local out="${crate_dir}/llvm-cov-summary.json"
  (cd "$crate_dir" && cargo llvm-cov --summary-only --json 2>/dev/null > "$out" || true)
  echo "$out"
}

# Post a Bridge nudge.
bridge_post() {
  local msg="$1"
  curl -s -X POST "$BRIDGE_NUDGE_URL" \
    -H 'Content-Type: application/json' \
    -d "{\"from\":\"kade\",\"to\":\"team\",\"content\":$(python3 -c "import json,sys; print(json.dumps(sys.argv[1]))" "$msg")}" \
    > /dev/null 2>&1 || true
}

# --- main ---

log "nightly-coverage starting"

if [ ! -f "$FLOORS_FILE" ]; then
  log "ERROR: floors file missing: $FLOORS_FILE"
  bridge_post "NIGHTLY COVERAGE ERROR: coverage-floors.yml not found. Cannot run regression check."
  exit 0
fi

REGRESSIONS=()
PASSES=()

while IFS=' ' read -r lang proj_rel floor; do
  [ -z "$lang" ] && continue

  if [ "$lang" = "ts" ]; then
    if [ -n "$DRY_RUN" ] && [ -n "$FIXTURES" ]; then
      summary="${FIXTURES}/${proj_rel}/coverage/coverage-summary.json"
    else
      summary=$(run_ts_coverage "$proj_rel") || continue
    fi
    [ -f "$summary" ] || { log "  SKIP $proj_rel: coverage-summary.json not found"; continue; }
    pct=$(read_jest_pct "$summary") || { log "  SKIP $proj_rel: could not parse summary"; continue; }

  elif [ "$lang" = "rust" ]; then
    if [ -n "$DRY_RUN" ] && [ -n "$FIXTURES" ]; then
      summary="${FIXTURES}/${proj_rel}/llvm-cov-summary.json"
    else
      summary=$(run_rust_coverage "$proj_rel") || continue
    fi
    [ -f "$summary" ] || { log "  SKIP $proj_rel: llvm-cov-summary.json not found"; continue; }
    pct=$(read_llvm_pct "$summary") || { log "  SKIP $proj_rel: could not parse summary"; continue; }
  else
    continue
  fi

  # Compare using Python bc-style arithmetic (avoids float issues in bash).
  result=$(python3 -c "
pct=float('$pct'); floor=float('$floor')
delta=round(pct-floor,2)
if pct < floor:
    print(f'REGRESSION {pct} {floor} {delta}')
else:
    print(f'PASS {pct} {floor} {delta}')
")
  status=$(echo "$result" | cut -d' ' -f1)
  curr=$(echo "$result" | cut -d' ' -f2)
  flr=$(echo "$result" | cut -d' ' -f3)
  delta=$(echo "$result" | cut -d' ' -f4)

  if [ "$status" = "REGRESSION" ]; then
    log "  REGRESSION: $proj_rel — ${curr}% < floor: ${flr}% (delta: ${delta}%)"
    REGRESSIONS+=("${proj_rel}: ${curr}% (floor: ${flr}%, delta: ${delta}%)")
  else
    log "  PASS: $proj_rel — ${curr}% >= floor: ${flr}%"
    PASSES+=("${proj_rel}: ${curr}%")
  fi

done < <(parse_floors)

# Compose Bridge message.
if [ "${#REGRESSIONS[@]}" -gt 0 ]; then
  msg="NIGHTLY COVERAGE REGRESSION — $(date '+%Y-%m-%d')"$'\n'
  for r in "${REGRESSIONS[@]}"; do
    msg+="  FAIL ${r}"$'\n'
  done
  if [ "${#PASSES[@]}" -gt 0 ]; then
    msg+="  OK: "
    msg+=$(IFS=', '; echo "${PASSES[*]}")
  fi
  log "Posting regression bridge message"
  bridge_post "$msg"
  printf '%s\n' "$msg"
else
  msg="NIGHTLY COVERAGE PASS — $(date '+%Y-%m-%d')"$'\n'
  msg+="  "
  msg+=$(IFS=', '; echo "${PASSES[*]}")
  log "Posting green bridge message"
  bridge_post "$msg"
  printf '%s\n' "$msg"
fi

log "nightly-coverage done"
exit 0
