#!/bin/bash
# chorus-ops.sh — Unified operations daemon (merges defect-poller.sh + ops-agent.sh)
#
# Subcommands:
#   errors    — Defect polling: query Loki for error patterns, dedup, auto-card
#   health    — Health agent: pre-fetch system state, claude reasoning, act on findings
#   all       — Run errors first, then health (health self-throttles to every 3rd invocation)
#   status    — Show current state for both subsystems
#   dry-run   — Show what each subsystem would do, don't act
#
# State: ~/.chorus/chorus-ops-state.json (unified)
# Lock:  /tmp/chorus-ops.lock
#
# Usage:
#   chorus-ops.sh errors              # Poll Loki for defects
#   chorus-ops.sh errors --window 1h  # Custom error window
#   chorus-ops.sh health              # Run health agent
#   chorus-ops.sh health --model sonnet  # Override model
#   chorus-ops.sh all                 # Both (health throttled)
#   chorus-ops.sh status              # Show combined state
#   chorus-ops.sh dry-run             # Dry run both subsystems

set -euo pipefail

# --- Common Configuration ---
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BOARD_TS="$SCRIPT_DIR/cards"
CHORUS_LOG="$SCRIPT_DIR/chorus-log"
PROMPT_FILE="$SCRIPT_DIR/ops-agent-prompt.md"
STATE_FILE="$HOME/.chorus/chorus-ops-state.json"
STATE_DIR="$HOME/.chorus"
LOCK_FILE="/tmp/chorus-ops.lock"
LOKI_URL="http://localhost:3102"
ALERTMANAGER_URL="http://localhost:9093"

# --- Defaults ---
WINDOW="5m"
DRY_RUN=false
VERBOSE=false
MODEL="haiku"
BUDGET="0.05"
MAX_CARDS=2
PATTERN_THRESHOLD=3
DEDUP_WINDOW_HOURS=24
STALE_CLOSE_DAYS=7  # Auto-close defect cards when error stops recurring for 7 days (#2285 AC3)
HEALTH_THROTTLE_EVERY=3  # Run health every Nth "all" invocation (~15min at 5min interval)

# Critical keywords — escalate to P1
CRITICAL_PATTERN="panic|fatal|OOM|oom-kill|SIGKILL|crash|segfault|out of memory"

# False positive patterns — skip these
FALSE_POSITIVES=(
    "errorsmith"
    "npm ci failed.*continuing"
    "npm error.*permissions"
    "npm error.*complete log"
    "npm error.*root/Administrator"
    "WARN:.*npm ci"
    "write-scrubber"
    "infra-guardrails"
    "uncommitted files"
    "activity.md has no entries"
    "unhealthy containers.*promtail"
    "chorus-audit"
    "grafana-alerts"
    "INFO Fuseki.*PUT"
    "INFO Fuseki.*GET"
    "C3 memory usage"
    "traffic spike"
    "Deploy time"
    "Build time exceeds"
    "container unhealthy.*transient"
    "SPARQL query.*slow"
    "SPARQL query.*degraded"
    "XA crash recov"
    "command not found"
)

log() { echo "[chorus-ops] $(TZ=America/New_York date '+%H:%M:%S') $*" >&2; }
vlog() { $VERBOSE && log "$*" || true; }

show_help() {
    echo "chorus-ops.sh — Unified operations daemon (merges defect-poller + ops-agent)"
    echo ""
    echo "Usage: chorus-ops.sh {defects|errors|health|all|status|dry-run} [options]"
    echo ""
    echo "Subcommands:"
    echo "  defects|errors      Poll Loki for defects, dedup, auto-card"
    echo "  health              Health agent (claude reasoning)"
    echo "  all                 Both (health self-throttles to every 3rd run)"
    echo "  status              Show current state"
    echo "  dry-run             Dry run both subsystems"
    echo ""
    echo "Options:"
    echo "  --window <5m|1h|1d> Error polling window (default: 5m)"
    echo "  --model <model>     Health agent model (default: haiku)"
    echo "  --verbose           Extra logging"
    echo "  --help              Show this help"
    echo ""
    echo "Examples:"
    echo "  chorus-ops.sh defects              # Poll Loki for defects (5min window)"
    echo "  chorus-ops.sh defects --window 1h  # Custom error window"
    echo "  chorus-ops.sh health               # Run health agent"
    echo "  chorus-ops.sh health --model sonnet # Override model"
    echo "  chorus-ops.sh all                  # Both (health throttled to every 3rd)"
    echo "  chorus-ops.sh status               # Show combined state"
    echo "  chorus-ops.sh dry-run              # Dry run both subsystems"
    echo ""
    echo "State: ~/.chorus/chorus-ops-state.json"
    echo "Lock:  /tmp/chorus-ops.lock"
}

# --- Parse subcommand ---
if [[ $# -lt 1 ]]; then
    show_help
    exit 1
fi

SUBCOMMAND="$1"
shift

# Normalize subcommand aliases
case "$SUBCOMMAND" in
    --help|-h) show_help; exit 0 ;;
    defects) SUBCOMMAND="errors" ;;
esac

# --- Parse remaining args ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --window) WINDOW="$2"; shift 2 ;;
        --model) MODEL="$2"; shift 2 ;;
        --verbose) VERBOSE=true; shift ;;
        --help|-h) show_help; exit 0 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

# --- Ensure state directory and file ---
mkdir -p "$STATE_DIR"
if [ ! -f "$STATE_FILE" ]; then
    echo '{"version":2,"defects":{},"last_errors_poll":"","health":{"last_run":"","findings":[],"cards_created":0,"last_status":"unknown","last_summary":"","carded_categories":{}},"all_invocation_count":0}' > "$STATE_FILE"
fi

# --- Migrate from v1 state files if they exist ---
migrate_state() {
    python3 << 'PYEOF'
import json, os, sys

STATE_FILE = os.path.expanduser("~/.chorus/chorus-ops-state.json")
OLD_DEFECT = os.path.expanduser("~/.chorus/defect-state.json")
OLD_OPS = os.path.expanduser("~/.chorus/ops-agent-state.json")

try:
    with open(STATE_FILE) as f:
        state = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    state = {"version": 2}

# Already migrated?
if state.get("version", 0) >= 2 and state.get("defects") is not None:
    sys.exit(0)

# Merge defect state
if os.path.exists(OLD_DEFECT):
    try:
        with open(OLD_DEFECT) as f:
            old = json.load(f)
        state["defects"] = old.get("defects", {})
        state["last_errors_poll"] = old.get("last_poll", "")
    except (json.JSONDecodeError, KeyError):
        pass

# Merge ops-agent state
if os.path.exists(OLD_OPS):
    try:
        with open(OLD_OPS) as f:
            old = json.load(f)
        state["health"] = {
            "last_run": old.get("last_run", ""),
            "findings": old.get("findings", []),
            "cards_created": old.get("cards_created", 0),
            "last_status": old.get("last_status", "unknown"),
            "last_summary": old.get("last_summary", ""),
            "carded_categories": old.get("carded_categories", {}),
        }
    except (json.JSONDecodeError, KeyError):
        pass

state["version"] = 2
state.setdefault("all_invocation_count", 0)
state.setdefault("defects", {})
state.setdefault("health", {"last_run":"","findings":[],"cards_created":0,"last_status":"unknown","last_summary":"","carded_categories":{}})

with open(STATE_FILE, "w") as f:
    json.dump(state, f, indent=2)
PYEOF
}

migrate_state

# ============================================================
# STATUS subcommand
# ============================================================
do_status() {
    python3 << 'PYEOF'
import json, sys

STATE_FILE = "$STATE_FILE"
try:
    with open(STATE_FILE) as f:
        state = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    print("No state file found.")
    sys.exit(0)

print("=== chorus-ops status ===")
print()

# Errors subsystem
defects = state.get("defects", {})
print(f"[errors] Last poll: {state.get('last_errors_poll', 'never')}")
print(f"[errors] Tracked defects: {len(defects)}")
if defects:
    for h, d in sorted(defects.items(), key=lambda x: x[1].get('last_seen',''), reverse=True)[:10]:
        tier = d.get('tier', '?')
        count = d.get('count', 0)
        card = d.get('card_id', 'none')
        source = d.get('source', '?')
        pattern = d.get('pattern', '?')[:80]
        print(f"  [{tier:>8}] x{count:<3} card={card:<6} {source}: {pattern}")
print()

# Health subsystem
health = state.get("health", {})
print(f"[health] Last run: {health.get('last_run', 'never')}")
print(f"[health] Status: {health.get('last_status', 'unknown')}")
print(f"[health] Cards created (total): {health.get('cards_created', 0)}")
summary = health.get('last_summary', '')
if summary:
    print(f"[health] Summary: {summary}")
findings = health.get('findings', [])
if findings:
    print(f"[health] Active findings: {len(findings)}")
    for f in findings:
        print(f"  [{f.get('severity','?'):>8}] {f.get('id','?')}: {f.get('title','?')}")
        print(f"           action={f.get('action','?')} repeat={f.get('is_repeat','?')}")
else:
    print("[health] No active findings.")

print()
print(f"[all] Invocation count: {state.get('all_invocation_count', 0)}")
print(f"[all] Health runs every {$HEALTH_THROTTLE_EVERY}th 'all' invocation")
PYEOF
}

# Use envsubst-style replacement for the python heredoc
do_status() {
    local sf="$STATE_FILE"
    local throttle="$HEALTH_THROTTLE_EVERY"
    python3 - "$sf" "$throttle" << 'PYEOF'
import json, sys

STATE_FILE = sys.argv[1]
THROTTLE = sys.argv[2]

try:
    with open(STATE_FILE) as f:
        state = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    print("No state file found.")
    sys.exit(0)

print("=== chorus-ops status ===")
print()

# Errors subsystem
defects = state.get("defects", {})
print(f"[errors] Last poll: {state.get('last_errors_poll', 'never')}")
print(f"[errors] Tracked defects: {len(defects)}")
if defects:
    for h, d in sorted(defects.items(), key=lambda x: x[1].get('last_seen',''), reverse=True)[:10]:
        tier = d.get('tier', '?')
        count = d.get('count', 0)
        card = d.get('card_id', 'none')
        source = d.get('source', '?')
        pattern = d.get('pattern', '?')[:80]
        print(f"  [{tier:>8}] x{count:<3} card={card:<6} {source}: {pattern}")
print()

# Health subsystem
health = state.get("health", {})
print(f"[health] Last run: {health.get('last_run', 'never')}")
print(f"[health] Status: {health.get('last_status', 'unknown')}")
print(f"[health] Cards created (total): {health.get('cards_created', 0)}")
summary = health.get('last_summary', '')
if summary:
    print(f"[health] Summary: {summary}")
findings = health.get('findings', [])
if findings:
    print(f"[health] Active findings: {len(findings)}")
    for f in findings:
        print(f"  [{f.get('severity','?'):>8}] {f.get('id','?')}: {f.get('title','?')}")
        print(f"           action={f.get('action','?')} repeat={f.get('is_repeat','?')}")
else:
    print("[health] No active findings.")

print()
print(f"[all] Invocation count: {state.get('all_invocation_count', 0)}")
print(f"[all] Health runs every {THROTTLE}th 'all' invocation")
PYEOF
}

# ============================================================
# ERRORS subcommand (defect-poller logic)
# ============================================================
do_errors() {
    local dry_run_flag="$1"

    # Convert window to seconds
    local now_epoch seconds start_epoch
    now_epoch=$(date -u +%s)
    case "$WINDOW" in
        *m) seconds=$((${WINDOW%m} * 60)) ;;
        *h) seconds=$((${WINDOW%h} * 3600)) ;;
        *d) seconds=$((${WINDOW%d} * 86400)) ;;
        *)  seconds=300 ;;
    esac
    start_epoch=$((now_epoch - seconds))

    # Check Loki
    if ! curl -s --max-time 3 "${LOKI_URL}/ready" >/dev/null 2>&1; then
        log "WARN: Loki unreachable at ${LOKI_URL}"
        return 0
    fi

    # Loki queries
    local query_structured='{container_name=~".+"} | json | level="error"'
    local query_unstructured='{container_name=~".+"} |~ "(?i)\\bpanic\\b|\\bfatal\\b|\\bOOM\\b|\\bSIGKILL\\b|\\bout of memory\\b|\\bcrash\\b|\\bsegfault\\b"'
    local query_chorus='{job="chorus-operations", level="error"}'

    # Fetch logs to temp dir
    local tmpdir
    tmpdir=$(mktemp -d)

    fetch_loki() {
        local query="$1"
        curl -s --max-time 10 "${LOKI_URL}/loki/api/v1/query_range" \
            --data-urlencode "query=${query}" \
            --data-urlencode "limit=100" \
            --data-urlencode "start=${start_epoch}" \
            --data-urlencode "end=${now_epoch}" 2>/dev/null || echo '{"data":{"result":[]}}'
    }

    fetch_loki "$query_structured" > "$tmpdir/structured.json"
    fetch_loki "$query_unstructured" > "$tmpdir/unstructured.json"
    fetch_loki "$query_chorus" > "$tmpdir/chorus.json"

    # Join false positives
    local fp_joined=""
    for fp in "${FALSE_POSITIVES[@]}"; do
        if [ -n "$fp_joined" ]; then
            fp_joined="${fp_joined}|${fp}"
        else
            fp_joined="$fp"
        fi
    done

    # Process errors (python)
    export DEFECT_TMPDIR="$tmpdir"
    export DEDUP_WINDOW_HOURS PATTERN_THRESHOLD CRITICAL_PATTERN STALE_CLOSE_DAYS
    export DRY_RUN="$dry_run_flag"
    export BOARD_TS CHORUS_LOG
    export FALSE_POSITIVES="$fp_joined"
    export CHORUS_OPS_STATE_FILE="$STATE_FILE"

    python3 << 'PYEOF'
import json, hashlib, os, sys, re, subprocess
from datetime import datetime, timezone, timedelta

STATE_FILE = os.environ["CHORUS_OPS_STATE_FILE"]
DEDUP_HOURS = int(os.environ.get("DEDUP_WINDOW_HOURS", "24"))
PATTERN_THRESHOLD = int(os.environ.get("PATTERN_THRESHOLD", "3"))
DRY_RUN = os.environ.get("DRY_RUN", "false") == "true"
CRITICAL_RE = os.environ.get("CRITICAL_PATTERN", "")
BOARD_TS = os.environ.get("BOARD_TS", "")
CHORUS_LOG = os.environ.get("CHORUS_LOG", "")
FALSE_POSITIVES_STR = os.environ.get("FALSE_POSITIVES", "")

critical_re = re.compile(CRITICAL_RE, re.IGNORECASE) if CRITICAL_RE else None
false_positive_re = [re.compile(fp, re.IGNORECASE) for fp in FALSE_POSITIVES_STR.split("|") if fp]

def is_false_positive(line):
    for fp in false_positive_re:
        if fp.search(line):
            return True
    try:
        parsed = json.loads(line)
        app = parsed.get("appName", "")
        msg = parsed.get("message", "")
        combined = f"{app} {msg}"
        for fp in false_positive_re:
            if fp.search(combined):
                return True
    except (json.JSONDecodeError, AttributeError):
        pass
    return False

def normalize_pattern(line):
    """Strip timestamps, UUIDs, hex, numbers, paths to get stable hash."""
    s = line
    s = re.sub(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[.\d]*Z?', '<TS>', s)
    s = re.sub(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', '<UUID>', s)
    s = re.sub(r'[0-9a-f]{16,}', '<HEX>', s)
    s = re.sub(r'0x[0-9a-f]+', '<ADDR>', s)
    s = re.sub(r'(?:/[\w.-]+){2,}(?:\.[\w]+)?', '<PATH>', s)
    s = re.sub(r'https?://[^\s,"]+', '<URL>', s)
    s = re.sub(r'goroutine \d+', 'goroutine <N>', s)
    s = re.sub(r'stack=".+"', 'stack="<STACK>"', s)
    s = re.sub(r'message [0-9a-f-]+', 'message <ID>', s)
    s = re.sub(r'handler_poisoned=\S+', 'handler_poisoned=<H>', s)
    s = re.sub(r'topic_poisoned=\S+', 'topic_poisoned=<T>', s)
    s = re.sub(r'subscriber_poisoned=\S+', 'subscriber_poisoned=<S>', s)
    s = re.sub(r'reason_poisoned=.*$', 'reason_poisoned=<REASON>', s)
    s = re.sub(r':\d{2,5}\b', ':<PORT>', s)
    s = re.sub(r'\s+', ' ', s).strip()
    return s

def hash_pattern(source, pattern):
    return hashlib.sha256(f"{source}:{pattern}".encode()).hexdigest()[:16]

def classify_tier(line):
    if critical_re and critical_re.search(line):
        return "critical"
    return "warning"

# Load unified state
with open(STATE_FILE) as f:
    state = json.load(f)
defects = state.get("defects", {})

# Expire old entries
cutoff = (datetime.now(timezone.utc) - timedelta(hours=DEDUP_HOURS)).isoformat()
expired = [h for h, d in defects.items() if d.get("last_seen", "") < cutoff]
for h in expired:
    del defects[h]

# Parse Loki results
now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
new_defects = []
updated_defects = []

TMPDIR = os.environ.get("DEFECT_TMPDIR", "/tmp")
for filename, env_var in [("structured.json", "STRUCTURED"), ("unstructured.json", "UNSTRUCTURED"), ("chorus.json", "CHORUS")]:
    filepath = os.path.join(TMPDIR, filename)
    try:
        with open(filepath) as f:
            data = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        continue

    for stream in data.get("data", {}).get("result", []):
        labels = stream.get("stream", {})
        container = labels.get("container_name", labels.get("appName", "unknown"))

        stream_app = labels.get("appName", "")
        stream_fp = is_false_positive(stream_app)

        for ts_ns, line in stream.get("values", []):
            if stream_fp or is_false_positive(line):
                continue

            msg = line
            try:
                parsed = json.loads(line)
                msg = parsed.get("message", parsed.get("msg", line))
                log_level = parsed.get("level", "").lower()
                if log_level not in ("error", "fatal", "panic", "err", "crit"):
                    continue
            except (json.JSONDecodeError, AttributeError):
                if env_var == "STRUCTURED":
                    continue
                if env_var == "CHORUS":
                    msg = line
                else:
                    level_match = re.search(r'level=(\w+)', line)
                    if level_match:
                        log_level = level_match.group(1).lower()
                        if log_level not in ("error", "fatal", "panic", "err", "crit"):
                            continue
                        msg = line

            pattern = normalize_pattern(msg)
            h = hash_pattern(container, pattern)
            tier = classify_tier(line)

            if h in defects:
                defects[h]["count"] += 1
                defects[h]["last_seen"] = now_iso
                if defects[h].get("tier") != "critical" and tier == "critical":
                    defects[h]["tier"] = "critical"
                updated_defects.append(defects[h])
            else:
                defects[h] = {
                    "hash": h,
                    "source": container,
                    "pattern": pattern[:200],
                    "sample": msg[:500],
                    "tier": tier,
                    "count": 1,
                    "first_seen": now_iso,
                    "last_seen": now_iso,
                    "card_id": None,
                }
                new_defects.append(defects[h])

# Decide what to card
actions = {}

# Severity filter (#2285 AC2): Warnings wait for pattern threshold before carding.
# Only critical errors card on first occurrence. Single warnings are tracked in state
# but don't create cards — stops every ERROR log line from becoming a card.
seen_new = set()
for d in new_defects:
    h = d["hash"]
    if h in seen_new:
        continue
    seen_new.add(h)
    if d["tier"] == "critical":
        actions[h] = {"action": "card", "defect": d, "priority": "P1", "reason": "new critical"}
    # Warnings: wait for pattern threshold before carding — no more single-occurrence cards

for d in updated_defects:
    h = d["hash"]
    if h in actions:
        continue
    if d.get("card_id") is None and d["count"] >= PATTERN_THRESHOLD:
        actions[h] = {"action": "card", "defect": d, "priority": "P2", "reason": f"pattern threshold ({d['count']}x)"}
    elif d.get("card_id") and d["count"] % 10 == 0:
        actions[h] = {"action": "comment", "defect": d, "reason": f"recurring ({d['count']}x)"}

actions = list(actions.values())

# Execute actions
for act in actions:
    d = act["defect"]
    h = d["hash"]
    title_prefix = "DEFECT" if act.get("priority") == "P1" else "defect"
    title = f"[{title_prefix}] {d['source']}: {d['pattern'][:60]}"

    if act["action"] == "card":
        if DRY_RUN:
            print(f"DRY-RUN: would card [{act.get('priority','P2')}] {title}")
            continue

        owner = "Silas"
        if any(app in d["source"] for app in ("personal-site-app", "gathering-app", "wordpress")):
            owner = "Kade"

        try:
            result = subprocess.run(
                [BOARD_TS, "add", title,
                 "--owner", owner,
                 "--priority", act.get("priority", "P2"),
                 "--status", "ops",
                 "--domain", "infrastructure",
                 "--description", f"Auto-detected by chorus-ops (errors).\n\nPattern: {d['pattern'][:200]}\nSample: {d['sample'][:300]}\nFirst seen: {d['first_seen']}\nCount: {d['count']}\nHash: {h}"],
                capture_output=True, text=True, timeout=15
            )
            output = result.stdout.strip()
            card_match = re.search(r'#(\d+)', output)
            if card_match:
                d["card_id"] = card_match.group(1)
                print(f"CARDED: #{d['card_id']} [{act.get('priority','P2')}] {d['source']}: {d['pattern'][:60]}")
            else:
                print(f"CARDED (no ID parsed): {output}")
        except Exception as e:
            print(f"ERROR: Failed to card: {e}", file=sys.stderr)

        try:
            subprocess.run(
                [CHORUS_LOG, "ops.defect.detected", "system",
                 f"source={d['source']}", f"tier={d['tier']}", f"hash={h}",
                 f"card_id={d.get('card_id', 'none')}",
                 f"pattern={d['pattern'][:80]}"],
                capture_output=True, timeout=5
            )
        except Exception:
            pass

    elif act["action"] == "comment" and d.get("card_id"):
        if DRY_RUN:
            print(f"DRY-RUN: would comment on #{d['card_id']} ({d['count']}x)")
            continue
        try:
            subprocess.run(
                [BOARD_TS, "comment", str(d["card_id"]),
                 f"Defect recurring: {d['count']}x since {d['first_seen']}. Latest: {d['last_seen']}"],
                capture_output=True, text=True, timeout=15
            )
            print(f"COMMENT: #{d['card_id']} ({d['count']}x)")
        except Exception as e:
            print(f"ERROR: Failed to comment: {e}", file=sys.stderr)

# Auto-close stale defect cards (#2285 AC3)
# If a tracked defect has a card but hasn't recurred in STALE_CLOSE_DAYS, close the card.
STALE_CLOSE_DAYS_VAL = int(os.environ.get("STALE_CLOSE_DAYS", "7"))
stale_cutoff = (datetime.now(timezone.utc) - timedelta(days=STALE_CLOSE_DAYS_VAL)).isoformat()
stale_closed = 0
for h, d in list(defects.items()):
    card_id = d.get("card_id")
    last_seen = d.get("last_seen", "")
    if card_id and last_seen and last_seen < stale_cutoff:
        if not DRY_RUN:
            try:
                subprocess.run(
                    [BOARD_TS, "done", str(card_id)],
                    capture_output=True, text=True, timeout=15
                )
                print(f"AUTO-CLOSED: #{card_id} (no recurrence in {STALE_CLOSE_DAYS_VAL}d)")
                stale_closed += 1
                del defects[h]
            except Exception as e:
                print(f"ERROR: Failed to auto-close #{card_id}: {e}", file=sys.stderr)
        else:
            print(f"DRY-RUN: would auto-close #{card_id} (stale {STALE_CLOSE_DAYS_VAL}d)")

# Save state (errors subsystem only)
state["defects"] = defects
state["last_errors_poll"] = now_iso
with open(STATE_FILE, "w") as f:
    json.dump(state, f, indent=2)

# Summary
total = len(new_defects) + len(updated_defects)
carded = sum(1 for a in actions if a["action"] == "card" and not DRY_RUN)
parts = [f"{total} errors", f"{len(new_defects)} new patterns", f"{carded} carded"]
if stale_closed > 0:
    parts.append(f"{stale_closed} auto-closed")
if total > 0 or carded > 0 or stale_closed > 0:
    print(f"[errors] Poll: {', '.join(parts)}")
else:
    print("[errors] Poll: clean")
PYEOF

    rm -rf "$tmpdir"
}

# ============================================================
# HEALTH subcommand (ops-agent logic)
# ============================================================
do_health() {
    local dry_run_flag="$1"

    vlog "Phase 1: Pre-fetching system state"

    local tmp_dir
    tmp_dir=$(mktemp -d)

    # Parallel pre-fetch
    curl -s --max-time 5 "${ALERTMANAGER_URL}/api/v2/alerts?active=true" \
        > "$tmp_dir/alerts.json" 2>/dev/null &

    curl -s --max-time 10 "${LOKI_URL}/loki/api/v1/query" \
        --data-urlencode 'query=sum by (container_name) (count_over_time({container_name=~".+"} | json | level="error" [30m]))' \
        > "$tmp_dir/loki_errors.json" 2>/dev/null &

    curl -s --max-time 10 "${LOKI_URL}/loki/api/v1/query" \
        --data-urlencode 'query=sum by (container_name) (count_over_time({container_name=~".+"} |~ "(?i)(sync|fuseki)" | json | level="error" [30m]))' \
        > "$tmp_dir/loki_sync.json" 2>/dev/null &

    df -h / > "$tmp_dir/disk.txt" 2>/dev/null &

    "$BOARD_TS" list > "$tmp_dir/board.txt" 2>/dev/null &

    wait
    vlog "Phase 1: Pre-fetch complete"

    # Assemble context JSON
    export OPS_TMP_DIR="$tmp_dir"
    export CHORUS_OPS_STATE_FILE="$STATE_FILE"

    local CONTEXT_JSON
    CONTEXT_JSON=$(python3 << 'PYEOF'
import json, os, re, sys
from datetime import datetime, timezone

TMP = os.environ["OPS_TMP_DIR"]
STATE_FILE = os.environ["CHORUS_OPS_STATE_FILE"]

context = {
    "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "containers": {},
    "alerts": {},
    "errors": {},
    "disk": {},
    "board": {},
    "previous_findings": []
}

EXPECTED_CONTAINERS = {
    "jeff-bridwell-personal-site-app",
    "jeff-bridwell-personal-site-fuseki",
    "jeff-bridwell-personal-site-navidrome",
    "jeff-bridwell-personal-site-css",
    "jeff-bridwell-personal-site-webvowl",
    "prometheus",
    "alertmanager",
    "grafana",
    "loki",
    "promtail",
    "blackbox-exporter",
    "mysqld-exporter",
    "node-exporter",
    "vikunja",
    "wordpress-mysql",
    "wordpress-blog",
    "wordpress-mailhog",
}

# Containers
containers = {"total": 0, "running": 0, "unhealthy": [], "stopped": [], "missing": []}
running_names = set()
try:
    with open(os.path.join(TMP, "containers.jsonl")) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            c = json.loads(line)
            containers["total"] += 1
            name = c.get("Names", "unknown")
            state = c.get("State", "unknown").lower()
            status = c.get("Status", "").lower()
            running_names.add(name)
            if state == "running":
                containers["running"] += 1
                if "unhealthy" in status:
                    containers["unhealthy"].append(name)
            else:
                containers["stopped"].append(name)
except (FileNotFoundError, json.JSONDecodeError):
    pass
containers["missing"] = sorted(EXPECTED_CONTAINERS - running_names)
context["containers"] = containers

# Alerts
alerts = {"firing": []}
try:
    with open(os.path.join(TMP, "alerts.json")) as f:
        data = json.load(f)
    if isinstance(data, list):
        for a in data:
            labels = a.get("labels", {})
            annotations = a.get("annotations", {})
            alerts["firing"].append({
                "alertname": labels.get("alertname", "unknown"),
                "severity": labels.get("severity", "unknown"),
                "summary": annotations.get("summary", annotations.get("description", ""))[:200]
            })
except (FileNotFoundError, json.JSONDecodeError):
    pass
context["alerts"] = alerts

# Loki errors
errors = {"total_30m": 0, "by_container": {}}
try:
    with open(os.path.join(TMP, "loki_errors.json")) as f:
        data = json.load(f)
    for result in data.get("data", {}).get("result", []):
        container = result.get("metric", {}).get("container_name", "unknown")
        value = result.get("value", [0, "0"])
        count = int(float(value[1])) if len(value) > 1 else 0
        errors["by_container"][container] = count
        errors["total_30m"] += count
except (FileNotFoundError, json.JSONDecodeError, ValueError):
    pass
context["errors"] = errors

# Sync storm
sync_storm = {"detected": False, "container": None, "count": 0}
try:
    with open(os.path.join(TMP, "loki_sync.json")) as f:
        data = json.load(f)
    for result in data.get("data", {}).get("result", []):
        container = result.get("metric", {}).get("container_name", "unknown")
        value = result.get("value", [0, "0"])
        count = int(float(value[1])) if len(value) > 1 else 0
        if count > 10:
            sync_storm = {"detected": True, "container": container, "count": count}
            break
except (FileNotFoundError, json.JSONDecodeError, ValueError):
    pass
context["errors"]["sync_storm"] = sync_storm

# Disk
disk = {"usage_pct": 0, "available_gb": 0}
try:
    with open(os.path.join(TMP, "disk.txt")) as f:
        lines = f.readlines()
    if len(lines) > 1:
        parts = lines[1].split()
        if len(parts) >= 5:
            pct_str = parts[4].replace('%', '')
            disk["usage_pct"] = int(pct_str)
            avail = parts[3]
            m = re.match(r'([\d.]+)([KMGTP]i?)', avail)
            if m:
                val = float(m.group(1))
                unit = m.group(2).upper().rstrip('I')
                multipliers = {'K': 0.001, 'M': 0.001, 'G': 1.0, 'T': 1000.0, 'P': 1000000.0}
                disk["available_gb"] = round(val * multipliers.get(unit, 1.0), 1)
except (FileNotFoundError, ValueError, IndexError):
    pass
context["disk"] = disk

# Board
board = {"summary": ""}
try:
    with open(os.path.join(TMP, "board.txt")) as f:
        board["summary"] = f.read()[:2000]
except FileNotFoundError:
    pass
context["board"] = board

# Previous findings from unified state
try:
    with open(STATE_FILE) as f:
        ustate = json.load(f)
    context["previous_findings"] = ustate.get("health", {}).get("findings", [])
except (FileNotFoundError, json.JSONDecodeError):
    context["previous_findings"] = []

print(json.dumps(context))
PYEOF
    )

    vlog "Context assembled (${#CONTEXT_JSON} bytes)"

    # Dry run: show context and exit
    if [ "$dry_run_flag" = "true" ]; then
        echo "[health] Dry run — context JSON:"
        echo "$CONTEXT_JSON" | python3 -m json.tool
        rm -rf "$tmp_dir"
        return 0
    fi

    # Phase 2: Claude reasoning
    vlog "Phase 2: Calling claude -p (model=$MODEL, budget=$BUDGET)"

    if [ ! -f "$PROMPT_FILE" ]; then
        log "ERROR: System prompt not found at $PROMPT_FILE"
        rm -rf "$tmp_dir"
        return 1
    fi
    local SYSTEM_PROMPT
    SYSTEM_PROMPT=$(cat "$PROMPT_FILE")

    local JSON_SCHEMA='{"type":"object","properties":{"status":{"type":"string"},"findings":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"severity":{"type":"string"},"category":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"action":{"type":"string"},"is_repeat":{"type":"boolean"}},"required":["id","severity","category","title","description","action","is_repeat"]}},"summary":{"type":"string"}},"required":["status","findings","summary"]}'

    echo "$CONTEXT_JSON" > "$tmp_dir/context.json"

    cat "$tmp_dir/context.json" | env -u CLAUDECODE claude -p \
        --model "$MODEL" \
        --permission-mode dontAsk \
        --no-session-persistence \
        --max-budget-usd "$BUDGET" \
        --output-format json \
        --json-schema "$JSON_SCHEMA" \
        --disallowedTools "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task" \
        --system-prompt "$SYSTEM_PROMPT" \
        > "$tmp_dir/claude_response.json" 2>/dev/null || {
        log "ERROR: claude -p failed (exit code $?)"
        "$CHORUS_LOG" ops.agent.completed system status=error model="$MODEL" >/dev/null 2>/dev/null || true
        rm -rf "$tmp_dir"
        return 1
    }

    local RESPONSE_SIZE
    RESPONSE_SIZE=$(wc -c < "$tmp_dir/claude_response.json" | tr -d ' ')
    vlog "Phase 2: Claude response received (${RESPONSE_SIZE} bytes)"

    if [ "$RESPONSE_SIZE" -lt 10 ]; then
        log "ERROR: Claude response too small (${RESPONSE_SIZE} bytes)"
        "$CHORUS_LOG" ops.agent.completed system status=error model="$MODEL" error=empty_response >/dev/null 2>/dev/null || true
        rm -rf "$tmp_dir"
        return 1
    fi

    cp "$tmp_dir/claude_response.json" /tmp/chorus-ops-last-health-response.json 2>/dev/null || true

    # Phase 3: Act on findings
    vlog "Phase 3: Processing findings"

    export OPS_TMP_DIR="$tmp_dir"
    export OPS_BOARD_TS="$BOARD_TS"
    export OPS_CHORUS_LOG="$CHORUS_LOG"
    export OPS_MAX_CARDS="$MAX_CARDS"
    export OPS_MODEL="$MODEL"
    export CHORUS_OPS_STATE_FILE="$STATE_FILE"

    python3 << 'PYEOF'
import json, os, subprocess, sys, re
from datetime import datetime, timezone

RESPONSE_FILE = os.path.join(os.environ["OPS_TMP_DIR"], "claude_response.json")
BOARD_TS = os.environ["OPS_BOARD_TS"]
CHORUS_LOG = os.environ["OPS_CHORUS_LOG"]
STATE_FILE = os.environ["CHORUS_OPS_STATE_FILE"]
MAX_CARDS = int(os.environ.get("OPS_MAX_CARDS", "2"))
MODEL = os.environ.get("OPS_MODEL", "haiku")

# Parse claude JSON envelope
try:
    with open(RESPONSE_FILE) as f:
        envelope = json.load(f)
    if "structured_output" in envelope and envelope["structured_output"]:
        response = envelope["structured_output"]
    else:
        result_text = envelope.get("result", "")
        if isinstance(result_text, str):
            cleaned = result_text.strip()
            if cleaned.startswith("```"):
                cleaned = re.sub(r'^```\w*\n?', '', cleaned)
                cleaned = re.sub(r'\n?```\s*$', '', cleaned)
            response = json.loads(cleaned)
        elif isinstance(result_text, dict):
            response = result_text
        else:
            response = envelope
except (json.JSONDecodeError, TypeError, FileNotFoundError) as e:
    print(f"ERROR: Failed to parse claude response: {e}", file=sys.stderr)
    try:
        subprocess.run(
            [CHORUS_LOG, "ops.agent.completed", "system",
             "status=error", f"model={MODEL}", "error=json_parse"],
            capture_output=True, timeout=5
        )
    except Exception:
        pass
    sys.exit(1)

status = response.get("status", "ok")
findings = response.get("findings", [])
summary = response.get("summary", "No summary")

# Load state for cooldown
try:
    with open(STATE_FILE) as _sf:
        _full_state = json.load(_sf)
except (FileNotFoundError, json.JSONDecodeError):
    _full_state = {"version": 2, "health": {}}

health_state = _full_state.get("health", {})
carded_categories = health_state.get("carded_categories", {})
COOLDOWN_HOURS = 24

def is_on_cooldown(category):
    if category not in carded_categories:
        return False
    try:
        last = datetime.fromisoformat(carded_categories[category].replace("Z", "+00:00"))
        now = datetime.now(timezone.utc)
        return (now - last).total_seconds() < COOLDOWN_HOURS * 3600
    except (ValueError, TypeError):
        return False

# Execute actions
now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
cards_created = 0
for f in findings:
    fid = f.get("id", "unknown")
    action = f.get("action", "ignore")
    is_repeat = f.get("is_repeat", False)
    severity = f.get("severity", "info")
    title = f.get("title", "Unknown finding")
    desc = f.get("description", "")
    category = f.get("category", "unknown")

    if action == "card" and not is_repeat and cards_created < MAX_CARDS and not is_on_cooldown(category):
        priority = "P1" if severity == "critical" else "P2"
        card_desc = (
            f"Auto-detected by chorus-ops (health).\n\n"
            f"{desc}\n\n"
            f"Finding ID: {fid}\n"
            f"Severity: {severity}\n"
            f"Category: {category}"
        )
        try:
            result = subprocess.run(
                [BOARD_TS, "add", f"[ops-health] {title}",
                 "--owner", "Silas",
                 "--priority", priority,
                 "--status", "ops",
                 "--domain", "infrastructure",
                 "--description", card_desc],
                capture_output=True, text=True, timeout=15
            )
            card_match = re.search(r'#(\d+)', result.stdout)
            card_id = card_match.group(1) if card_match else "?"
            cards_created += 1
            carded_categories[category] = now_iso
            print(f"CARD: #{card_id} [{priority}] {title}")
        except Exception as e:
            print(f"ERROR: card creation failed: {e}", file=sys.stderr)

    elif action == "card" and is_on_cooldown(category):
        print(f"COOLDOWN: [{severity}] {category}: {title} (carded within {COOLDOWN_HOURS}h)")

    elif action == "log" or (action == "card" and is_repeat):
        print(f"LOG: [{severity}] {fid}: {title}")

    elif action == "ignore":
        pass

# Log to chorus
try:
    subprocess.run(
        [CHORUS_LOG, "ops.agent.completed", "system",
         f"status={status}",
         f"findings={len(findings)}",
         f"cards={cards_created}",
         f"model={MODEL}",
         f"summary={summary[:100]}"],
        capture_output=True, timeout=5
    )
except Exception:
    pass

# Save health state into unified state file
_full_state["health"] = {
    "last_run": now_iso,
    "findings": findings,
    "cards_created": health_state.get("cards_created", 0) + cards_created,
    "last_status": status,
    "last_summary": summary,
    "carded_categories": carded_categories,
}

with open(STATE_FILE, "w") as f_out:
    json.dump(_full_state, f_out, indent=2)

print(f"[health] Run complete: status={status} findings={len(findings)} cards={cards_created}")
if summary:
    print(f"[health] Summary: {summary}")
PYEOF

    rm -rf "$tmp_dir"
}

# ============================================================
# LOCK (shared across subcommands)
# ============================================================
acquire_lock() {
    if [ -f "$LOCK_FILE" ]; then
        local existing_pid
        existing_pid=$(cat "$LOCK_FILE" 2>/dev/null || echo "")
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            log "Already running (PID $existing_pid), skipping"
            exit 0
        fi
        rm -f "$LOCK_FILE"
    fi
    echo $$ > "$LOCK_FILE"
    trap "rm -f $LOCK_FILE" EXIT
}

# ============================================================
# MAIN — dispatch subcommand
# ============================================================
case "$SUBCOMMAND" in
    status)
        do_status
        ;;
    dry-run)
        echo "=== errors (dry-run) ==="
        do_errors "true" || true
        echo ""
        echo "=== health (dry-run) ==="
        do_health "true" || true
        ;;
    errors)
        acquire_lock
        do_errors "$DRY_RUN"
        ;;
    health)
        acquire_lock
        do_health "$DRY_RUN"
        ;;
    all)
        acquire_lock
        # Always run errors
        do_errors "false" || true

        # Health self-throttles: only every Nth invocation
        # Read and increment counter atomically
        INVOCATION=$(python3 -c "
import json
sf = '$STATE_FILE'
with open(sf) as f:
    s = json.load(f)
n = s.get('all_invocation_count', 0) + 1
s['all_invocation_count'] = n
with open(sf, 'w') as f:
    json.dump(s, f, indent=2)
print(n)
")
        if (( INVOCATION % HEALTH_THROTTLE_EVERY == 0 )); then
            vlog "Health check triggered (invocation #$INVOCATION)"
            do_health "false" || true
        else
            vlog "Health check skipped (invocation #$INVOCATION, runs every ${HEALTH_THROTTLE_EVERY}th)"
        fi
        ;;
    *)
        echo "Unknown subcommand: $SUBCOMMAND"
        echo "Usage: chorus-ops.sh {errors|health|all|status|dry-run} [options]"
        exit 1
        ;;
esac
