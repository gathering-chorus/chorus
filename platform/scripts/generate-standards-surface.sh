#!/usr/bin/env bash
# generate-standards-surface.sh — Produce live HTML from decisions, hooks, and gate logs
# Card #2266/#2267: Replace hardcoded counts with real data from three sources
#
# Data sources:
#   1. decisions.md — count of DEC-NNN entries
#   2. chorus-hooks/src/hooks/*.rs — count of Rust hook modules
#   3. ~/Library/Logs/Gathering/hooks.log — gate enforcement rates (trailing 7 days)
#
# Pattern: claudemd-gen (read sources, template output, idempotent)
set -euo pipefail

# --- Configuration ---
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

REPO_ROOT="${CHORUS_ROOT}"
DECISIONS_MD="$REPO_ROOT/roles/wren/decisions.md"
HOOKS_DIR="$REPO_ROOT/platform/services/chorus-hooks/src/hooks"
PULSE_LOG="$HOME/Library/Logs/Gathering/hooks.log"
MEMORY_DIR="$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory"
APP_DOCS="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs"

# Allow override for testing
OUTPUT_DIR="${APP_DOCS}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

mkdir -p "$OUTPUT_DIR"

# --- Harvest data ---

# 1. Decision count
DECISION_COUNT=$(grep -c "^## DEC-" "$DECISIONS_MD" 2>/dev/null || echo 0)

# 2. Hook module count (exclude mod.rs)
HOOK_COUNT=$(ls "$HOOKS_DIR"/*.rs 2>/dev/null | grep -v mod.rs | wc -l | tr -d ' ')

# 3. Feedback and story counts from memory files
FEEDBACK_COUNT=$(ls "$MEMORY_DIR"/feedback_*.md 2>/dev/null | wc -l | tr -d ' ')
STORY_COUNT=$(ls "$MEMORY_DIR"/story_*.md 2>/dev/null | wc -l | tr -d ' ')

# 4. Gate enforcement rates from hooks metrics API (#2277)
# Replaces direct awk parsing of pulse log — API has 60s cache and structured JSON
METRICS_JSON=$(curl -sf --max-time 5 "http://localhost:3340/api/chorus/hooks/metrics" 2>/dev/null || echo "")

if [ -n "$METRICS_JSON" ]; then
  eval "$(echo "$METRICS_JSON" | python3 -c "
import json, sys
d = json.load(sys.stdin)
modules = d.get('modules', {})
allow = sum(m['allow'] for m in modules.values())
deny = sum(m['deny'] for m in modules.values())
warn = sum(m['warn'] for m in modules.values())
enforced = d.get('enforcedModules', 0)
active = d.get('totalModules', 0)
print(f'ALLOW_COUNT={allow}')
print(f'BLOCK_COUNT=0')
print(f'DENY_COUNT={deny}')
print(f'TOTAL_DECISIONS={d.get(\"totalDecisions\", 0)}')
print(f'ENFORCED_MODULES={enforced}')
all_active = active
partial = all_active - enforced
print(f'PARTIAL_MODULES={partial}')
# Per-module deny counts for tooltips
denies = [(m, v['deny']) for m, v in modules.items() if v['deny'] > 0]
denies.sort(key=lambda x: -x[1])
print(f'MODULE_DENIES={chr(44).join(f\"{m}:{c}\" for m, c in denies)}')
" 2>/dev/null)"

  # Doc-only = total hook modules minus active modules from API
  ALL_ACTIVE_MODULES=$(echo "$METRICS_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin).get('totalModules',0))" 2>/dev/null || echo 0)
  DOC_ONLY_MODULES=$(( HOOK_COUNT - ALL_ACTIVE_MODULES ))
  [ "$DOC_ONLY_MODULES" -lt 0 ] && DOC_ONLY_MODULES=0

  # Percentages
  if [ "$HOOK_COUNT" -gt 0 ]; then
    PCT_ENFORCED=$(( ENFORCED_MODULES * 100 / HOOK_COUNT ))
    PCT_PARTIAL=$(( PARTIAL_MODULES * 100 / HOOK_COUNT ))
    PCT_DOC=$(( 100 - PCT_ENFORCED - PCT_PARTIAL ))
  else
    PCT_ENFORCED=0; PCT_PARTIAL=0; PCT_DOC=100
  fi
else
  # Fallback: API unreachable
  ALLOW_COUNT=0; BLOCK_COUNT=0; DENY_COUNT=0; TOTAL_DECISIONS=0
  ENFORCED_MODULES=0; PARTIAL_MODULES=0; DOC_ONLY_MODULES="$HOOK_COUNT"
  PCT_ENFORCED=0; PCT_PARTIAL=0; PCT_DOC=100
  MODULE_DENIES=""
  echo "Warning: hooks metrics API unreachable, using zero values" >&2
fi

GENERATED_DATE=$(TZ=America/New_York date '+%Y-%m-%d %H:%M')

# --- Generate chorus-standards.html ---
# Strategy: read the existing curated HTML, replace the data-driven sections
# (header counts, instrumentation bar, and per-standard deny count tooltips).

EXISTING_STANDARDS="$APP_DOCS/chorus-standards.html"
if [ -f "$EXISTING_STANDARDS" ]; then
  python3 - "$EXISTING_STANDARDS" "$OUTPUT_DIR/chorus-standards.html" \
    "$DECISION_COUNT" "$FEEDBACK_COUNT" "$STORY_COUNT" \
    "$PCT_ENFORCED" "$PCT_PARTIAL" "$PCT_DOC" \
    "$GENERATED_DATE" "$MODULE_DENIES" <<'PYEOF'
import sys, re

src, dst = sys.argv[1], sys.argv[2]
dec, fb, st = sys.argv[3], sys.argv[4], sys.argv[5]
pct_e, pct_p, pct_d = sys.argv[6], sys.argv[7], sys.argv[8]
gen_date = sys.argv[9]
module_denies_raw = sys.argv[10] if len(sys.argv) > 10 else ""

# Parse module deny counts: "tdd_gate:588,memory_gate:765,..."
module_denies = {}
if module_denies_raw:
    for pair in module_denies_raw.split(","):
        if ":" in pair:
            mod, count = pair.split(":", 1)
            module_denies[mod.strip()] = int(count.strip())

# Map hook modules to keywords found in standard names
hook_to_keywords = {
    "tdd_gate": ["TDD Discipline"],
    "demo_gate": ["Proving gate"],
    "demo_preflight": ["Demo quality"],
    "memory_gate": ["Memory-and-research"],
    "memory_first": ["Chorus first for search"],
    "log_first_gate": ["Log-first"],
    "accept_gate": ["No self-service"],
    "search_hierarchy": ["Search hierarchy"],
    "infra_guardrails": ["Manual kill guard", "Git in team repo", "Terraform apply/destroy"],
    "sensitive_paths": ["No secrets"],
    "write_scrubber": ["credentials"],
    "pair_gate": ["Pair enforcement"],
    "session_init_gate": ["Session close-out"],
    "csc_guard": ["CSC"],
}

with open(src) as f:
    html = f.read()

# Replace date line
html = re.sub(
    r'(<div class="date">).*?(</div>)',
    rf'\g<1>{gen_date} — Generated by generate-standards-surface.sh. '
    rf'Sources: {dec} decisions, {fb} feedback memories, {st} stories.\g<2>',
    html, count=1
)

# Replace summary card numbers
replacements = [
    (r'<div class="number">\d+</div>(<div class="label">Decisions</div>)',
     rf'<div class="number">{dec}</div>\1'),
    (r'<div class="number">\d+</div>(<div class="label">Feedback Rules</div>)',
     rf'<div class="number">{fb}</div>\1'),
    (r'<div class="number">\d+</div>(<div class="label">Stories</div>)',
     rf'<div class="number">{st}</div>\1'),
]
for pattern, repl in replacements:
    html = re.sub(pattern, repl, html, count=1)

# Replace instrumentation bar widths
html = re.sub(r'(bar-instrumented" style="width: )\d+%', rf'\g<1>{pct_e}%', html)
html = re.sub(r'(bar-partial" style="width: )\d+%', rf'\g<1>{pct_p}%', html)
html = re.sub(r'(bar-documented" style="width: )\d+%', rf'\g<1>{pct_d}%', html)

# Replace legend percentages
html = re.sub(r'Gate-enforced \(\d+%\)', f'Gate-enforced ({pct_e}%)', html)
html = re.sub(r'Partially instrumented \(\d+%\)', f'Partially instrumented ({pct_p}%)', html)
html = re.sub(r'Documented only \(\d+%\)', f'Documented only ({pct_d}%)', html)

# Add deny count tooltips to GATE-tagged standards (#2267 AC 4)
for hook_mod, deny_count in module_denies.items():
    if hook_mod in hook_to_keywords:
        for keyword in hook_to_keywords[hook_mod]:
            escaped = re.escape(keyword)
            html = re.sub(
                rf'(<div class="standard)(">)(.*?{escaped}.*?tag-gate)',
                rf'\1" title="{hook_mod}: {deny_count} denies (7d)\2\3',
                html, count=1, flags=re.DOTALL
            )

with open(dst, 'w') as f:
    f.write(html)
PYEOF
else
  echo "Warning: $EXISTING_STANDARDS not found, skipping standards HTML" >&2
fi

# --- Generate chorus-hook-architecture.html ---
# Replace the header counts, gap status, and per-module deny stats

EXISTING_HOOKS="$APP_DOCS/chorus-hook-architecture.html"
if [ -f "$EXISTING_HOOKS" ]; then
  python3 - "$EXISTING_HOOKS" "$OUTPUT_DIR/chorus-hook-architecture.html" \
    "$HOOK_COUNT" "$GENERATED_DATE" \
    "$ENFORCED_MODULES" "$BLOCK_COUNT" "$DENY_COUNT" "$MODULE_DENIES" <<'PYEOF'
import sys, re

src, dst = sys.argv[1], sys.argv[2]
hook_count = sys.argv[3]
gen_date = sys.argv[4]
enforced = sys.argv[5]
blocks = sys.argv[6]
denies = sys.argv[7]
module_denies_raw = sys.argv[8] if len(sys.argv) > 8 else ""

# Parse per-module deny counts
module_denies = {}
if module_denies_raw:
    for pair in module_denies_raw.split(","):
        if ":" in pair:
            mod, count = pair.split(":", 1)
            module_denies[mod.strip()] = int(count.strip())

with open(src) as f:
    html = f.read()

# Update hook count in intro paragraph
html = re.sub(r'\d+ hook modules', f'{hook_count} hook modules', html, count=1)

# Update generation date
html = re.sub(
    r'generated \d{4}-\d{2}-\d{2}',
    f'generated {gen_date[:10]}',
    html, count=1
)

# Update the update date with enforcement stats
html = re.sub(
    r'updated \d{4}-\d{2}-\d{2}[^<]*',
    f'updated {gen_date[:10]} by generate-standards-surface.sh. '
    f'{enforced} modules actively enforcing ({blocks} blocks, {denies} denies trailing 7d).',
    html, count=1
)

# Add per-module deny counts to hook inventory table rows (#2267 AC 4)
for mod, count in module_denies.items():
    # Find the table row for this hook module and add deny count
    html = re.sub(
        rf'(<tr><td>{re.escape(mod)}</td>)',
        rf'\1',
        html
    )
    # Add title attribute to the row's first cell
    html = re.sub(
        rf'<td>({re.escape(mod)})</td>',
        rf'<td title="{count} denies (7d)">\1</td>',
        html, count=1
    )

with open(dst, 'w') as f:
    f.write(html)
PYEOF
else
  echo "Warning: $EXISTING_HOOKS not found, skipping hooks HTML" >&2
fi

# Summary to stderr (stdout is data)
echo "Generated: $OUTPUT_DIR" >&2
echo "  Decisions: $DECISION_COUNT | Feedback: $FEEDBACK_COUNT | Stories: $STORY_COUNT" >&2
echo "  Hook modules: $HOOK_COUNT | Enforced: $ENFORCED_MODULES | Partial: $PARTIAL_MODULES | Doc-only: $DOC_ONLY_MODULES" >&2
echo "  Instrumentation: ${PCT_ENFORCED}% gate / ${PCT_PARTIAL}% partial / ${PCT_DOC}% doc" >&2
echo "  Pulse log (7d): $ALLOW_COUNT allow, $BLOCK_COUNT block, $DENY_COUNT deny" >&2
if [ -n "$MODULE_DENIES" ]; then
  echo "  Per-module: $MODULE_DENIES" >&2
fi
