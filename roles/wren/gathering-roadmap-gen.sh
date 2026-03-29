#!/usr/bin/env bash
# gathering-roadmap-gen.sh — Generate gathering-roadmap.html from live Vikunja board data
#
# Usage:
#   gathering-roadmap-gen.sh              # Generate and write HTML
#   gathering-roadmap-gen.sh --dry-run    # Print stats without writing
#
# Reads Gathering board state from Vikunja API, merges with domain mapping config,
# and outputs a domain-topology roadmap HTML. Static items (shipped capabilities)
# come from the mapping config. Card status (Done/Now/Next/Later) comes from the
# board API. Organized by concentric trust rings.
#
# Requires: curl, python3

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MAPPING="$SCRIPT_DIR/gathering-roadmap-mapping.json"
OUTPUT_LOCAL="$SCRIPT_DIR/gathering-roadmap.html"
OUTPUT_APP="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/gathering-roadmap.html"
DRY_RUN=false

if [ "${1:-}" = "--dry-run" ]; then
  DRY_RUN=true
fi

# --- Load Vikunja token ---
VIKUNJA_URL="${VIKUNJA_URL:-http://localhost:3456}"
VIKUNJA_TOKEN="${VIKUNJA_TOKEN:-}"

if [ -z "$VIKUNJA_TOKEN" ]; then
  for env_file in "/Users/jeffbridwell/CascadeProjects/messages/.env" \
                  "/Users/jeffbridwell/CascadeProjects/messages/scripts/.env"; do
    if [ -f "$env_file" ]; then
      VIKUNJA_TOKEN=$(grep "^VIKUNJA_TOKEN_WREN=" "$env_file" 2>/dev/null | cut -d'=' -f2- || true)
      if [ -z "$VIKUNJA_TOKEN" ]; then
        VIKUNJA_TOKEN=$(grep "^VIKUNJA_TOKEN=" "$env_file" 2>/dev/null | cut -d'=' -f2- || true)
      fi
      [ -n "$VIKUNJA_TOKEN" ] && break
    fi
  done
fi

if [ -z "$VIKUNJA_TOKEN" ]; then
  echo "ERROR: No Vikunja token found. Set VIKUNJA_TOKEN or add to messages/scripts/.env" >&2
  exit 1
fi

# --- Fetch board data ---
echo "Fetching Gathering board (project 2)..."
export GATHERING_DATA=$(curl -sS -H "Authorization: Bearer $VIKUNJA_TOKEN" \
  "$VIKUNJA_URL/api/v1/projects/2/views/8/tasks" 2>/dev/null)

# --- Generate HTML with Python ---
python3 - "$MAPPING" "$OUTPUT_LOCAL" "$OUTPUT_APP" "$DRY_RUN" <<'PYEOF'
import json, sys, os
from datetime import datetime

mapping_file = sys.argv[1]
output_local = sys.argv[2]
output_app = sys.argv[3]
dry_run = sys.argv[4] == "True"

# Read inputs
gathering_json = os.environ.get("GATHERING_DATA", "[]")
mapping = json.load(open(mapping_file))

gathering_buckets = json.loads(gathering_json) if gathering_json else []

# Build card lookup: { "g:78": { title, bucket, owner, ... }, ... }
cards = {}

g_bucket_map = {
    4: "Later", 7: "Next", 5: "Now", 8: "Blocked", 6: "Done", 12: "Next", 13: "Tech Debt"
}

def extract_owner(labels):
    for l in (labels or []):
        t = l.get("title", "")
        if t == "owner:silas": return "Silas"
        if t == "owner:kade": return "Kade"
        if t == "owner:wren": return "Wren"
        if t == "owner:jeff": return "Jeff"
    return None

def process_buckets(buckets, bucket_map, prefix):
    for bucket in buckets:
        bucket_id = bucket.get("id")
        bucket_name = bucket_map.get(bucket_id, bucket.get("title", "Unknown"))
        for task in (bucket.get("tasks") or []):
            idx = task.get("index", task.get("id"))
            key = f"{prefix}:{idx}"
            cards[key] = {
                "title": task.get("title", ""),
                "bucket": bucket_name,
                "owner": extract_owner(task.get("labels")),
                "index": idx,
                "done": task.get("done", False),
            }

process_buckets(gathering_buckets, g_bucket_map, "g")

total_cards = len(cards)
by_bucket = {}
for c in cards.values():
    b = c["bucket"]
    by_bucket[b] = by_bucket.get(b, 0) + 1

print(f"  Gathering: {total_cards} cards")
print(f"  By bucket: {by_bucket}")

# --- Helper functions ---

def role_class(owner):
    if not owner: return ""
    return {"Silas": "role-s", "Kade": "role-k", "Wren": "role-w", "Jeff": "role-w"}.get(owner, "")

def role_tag(owner):
    if not owner: return ""
    cls = role_class(owner)
    return f' <span class="role {cls}">{owner}</span>'

def card_tag(key):
    return f'<span class="card-ref card-g">#{key[2:]}</span>'

def get_card_bucket(key):
    c = cards.get(key)
    if not c: return None
    return c["bucket"]

def get_card_owner(key):
    c = cards.get(key)
    if not c: return None
    return c["owner"]

def card_phase(key):
    bucket = get_card_bucket(key)
    if not bucket: return "next"
    return {"Done": "done", "Now": "now", "Next": "next", "Later": "later",
            "Blocked": "now", "Tech Debt": "next"}.get(bucket, "next")

# --- Count items per phase ---
counts = {"done": 0, "now": 0, "next": 0, "later": 0}

def count_phase(phase):
    counts[phase] = counts.get(phase, 0) + 1

domain_count = len(mapping["domains"])

for d in mapping["domains"]:
    for _ in d.get("static", []):
        count_phase("done")
    for key in d.get("cards", {}):
        count_phase(card_phase(key))
    for key in d.get("laterCards", {}):
        count_phase("later")
    for _ in d.get("laterStatic", []):
        count_phase("later")

td = mapping.get("techDebt", {})
for _ in td.get("doneStatic", []):
    count_phase("done")
for key in td.get("cards", {}):
    count_phase(card_phase(key))

total = sum(counts.values())

print(f"  Roadmap totals: {counts}, total={total}")

if dry_run:
    print("Dry run — not writing HTML.")
    sys.exit(0)

# --- Build HTML ---
now = datetime.now().strftime("%Y-%m-%d %H:%M")

CSS = """
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #0a0a0f; color: #e0e0e0; padding: 30px 20px;
    min-width: 1100px;
  }
  header { text-align: center; margin-bottom: 30px; }
  h1 { font-size: 26px; font-weight: 300; letter-spacing: 2px; color: #fff; }
  .subtitle { font-size: 13px; color: #888; letter-spacing: 1px; margin-top: 6px; }
  .legend {
    display: flex; gap: 24px; justify-content: center; margin: 16px 0 24px;
    font-size: 12px; flex-wrap: wrap;
  }
  .legend-item { display: flex; align-items: center; gap: 6px; }
  .legend-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
  .phase-header {
    display: grid; grid-template-columns: 140px 1fr 1fr 1fr 1fr;
    gap: 8px; margin-bottom: 4px; padding: 0 8px;
  }
  .phase-label {
    font-size: 11px; font-weight: 700; text-transform: uppercase;
    letter-spacing: 2px; padding: 10px 14px; text-align: center;
    border-radius: 8px 8px 0 0;
  }
  .phase-done { background: rgba(92,252,124,0.08); color: #5cfc7c; }
  .phase-now { background: rgba(252,170,92,0.14); color: #fcaa5c; border: 1px solid rgba(252,170,92,0.25); }
  .phase-next { background: rgba(74,158,255,0.08); color: #4a9eff; }
  .phase-later { background: rgba(124,92,252,0.08); color: #7c5cfc; }
  .phase-count { font-size: 18px; font-weight: 300; display: block; margin-top: 2px; }
  .domain-row {
    display: grid; grid-template-columns: 140px 1fr 1fr 1fr 1fr;
    gap: 8px; margin-bottom: 6px; padding: 0 8px; min-height: 60px;
  }
  .d-label-cell {
    display: flex; align-items: center; justify-content: flex-end;
    gap: 10px; padding-right: 12px;
  }
  .d-icon {
    width: 36px; height: 36px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: 16px; font-weight: bold; flex-shrink: 0;
    box-shadow: 0 0 12px rgba(0,0,0,0.4);
  }
  .d-name {
    font-size: 12px; font-weight: 600; text-transform: uppercase;
    letter-spacing: 1.5px; text-align: right;
  }
  .d-stat {
    font-size: 9px; color: #666; margin-top: 2px; font-weight: 400;
    text-transform: none; letter-spacing: 0;
  }
  .cell { padding: 10px 12px; border-radius: 6px; min-height: 50px; }
  .cell-done { background: rgba(92,252,124,0.04); border: 1px solid rgba(92,252,124,0.08); }
  .cell-now { background: rgba(252,170,92,0.07); border: 1px solid rgba(252,170,92,0.2); box-shadow: inset 0 0 12px rgba(252,170,92,0.04); }
  .cell-next { background: rgba(74,158,255,0.04); border: 1px solid rgba(74,158,255,0.08); }
  .cell-later { background: rgba(124,92,252,0.04); border: 1px solid rgba(124,92,252,0.08); }
  .item { font-size: 11px; line-height: 1.5; padding: 4px 0; display: flex; align-items: flex-start; gap: 5px; }
  .item-icon { flex-shrink: 0; font-size: 12px; margin-top: 1px; }
  .item-done .item-icon { color: #5cfc7c; }
  .item-now .item-icon { color: #fcaa5c; }
  .item-next .item-icon { color: #4a9eff; }
  .item-later .item-icon { color: #7c5cfc; }
  .item-text { color: #ccc; }
  .item-done .item-text { color: #9efaad; }
  .item-now .item-text { color: #fcd09e; }
  .item-next .item-text { color: #9ec6fa; }
  .item-later .item-text { color: #b8a6fa; }
  .card-ref {
    font-size: 9px; font-weight: 600; padding: 1px 5px; border-radius: 3px;
    white-space: nowrap; margin-left: 3px; vertical-align: middle;
  }
  .card-g { background: rgba(74,158,255,0.15); color: #4a9eff; }
  .role {
    font-size: 8px; font-weight: 700; padding: 1px 4px; border-radius: 2px;
    white-space: nowrap; margin-left: 3px; vertical-align: middle;
    letter-spacing: 0.5px; text-transform: uppercase;
  }
  .role-s { background: rgba(92,138,252,0.2); color: #5c8afc; }
  .role-k { background: rgba(224,92,252,0.2); color: #e05cfc; }
  .role-w { background: rgba(252,170,92,0.2); color: #fcaa5c; }
  .shipped-items { display: none; }
  .shipped-items.expanded { display: block; }
  .shipped-summary {
    font-size: 11px; color: #5cfc7c; cursor: pointer; padding: 4px 0; opacity: 0.7;
  }
  .shipped-summary:hover { opacity: 1; }
  .shipped-summary .expand-icon { transition: transform 0.2s; display: inline-block; }
  .shipped-summary.expanded .expand-icon { transform: rotate(90deg); }
  .ring-divider {
    display: grid; grid-template-columns: 140px 1fr;
    gap: 8px; padding: 10px 8px 4px; margin-top: 10px;
  }
  .ring-divider-label {
    grid-column: 2 / -1; font-size: 10px; font-weight: 700; text-transform: uppercase;
    letter-spacing: 2px; padding: 4px 12px; display: flex; align-items: center; gap: 10px;
    border-bottom: 1px solid rgba(255,255,255,0.06);
  }
  .ring-divider-label .trust-tag {
    font-size: 8px; font-weight: 400; letter-spacing: 1px; padding: 2px 8px;
    border-radius: 10px; margin-left: auto;
  }
  .empty-signal {
    font-size: 10px; color: #444; font-style: italic; padding: 8px 0;
  }
  footer {
    text-align: center; margin-top: 30px; padding: 16px;
    font-size: 11px; color: #555; border-top: 1px solid rgba(255,255,255,0.05);
  }
  .totals {
    display: flex; justify-content: center; gap: 40px;
    margin: 20px 0; padding: 14px 24px;
    background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.06);
    border-radius: 8px; font-size: 13px;
  }
  .total-num { font-size: 22px; font-weight: 300; }
  .total-label { font-size: 10px; text-transform: uppercase; letter-spacing: 1px; color: #888; }
  .gen-badge {
    display: inline-block; font-size: 9px; padding: 2px 8px; border-radius: 10px;
    background: rgba(252,170,92,0.15); color: #fcaa5c; margin-left: 8px;
    letter-spacing: 0.5px;
  }
"""

html = []
html.append(f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Gathering Roadmap &mdash; Domain Topology</title>
<style>{CSS}</style>
</head>
<body>

<header>
  <h1>GATHERING ROADMAP</h1>
  <div class="subtitle">Domain Topology &mdash; What&rsquo;s Built, What&rsquo;s Next, Where the Gaps Are <span class="gen-badge">LIVE from board API &mdash; {now}</span></div>
</header>

<div class="totals">
  <div style="text-align:center"><div class="total-num" style="color:#5cfc7c">{counts['done']}</div><div class="total-label">Shipped</div></div>
  <div style="text-align:center"><div class="total-num" style="color:#fcaa5c">{counts['now']}</div><div class="total-label">Now</div></div>
  <div style="text-align:center"><div class="total-num" style="color:#4a9eff">{counts['next']}</div><div class="total-label">Next</div></div>
  <div style="text-align:center"><div class="total-num" style="color:#7c5cfc">{counts['later']}</div><div class="total-label">Later</div></div>
  <div style="text-align:center;border-left:1px solid rgba(255,255,255,0.1);padding-left:40px;">
    <div class="total-num" style="color:#fff">{total}</div><div class="total-label">Total</div>
  </div>
  <div style="text-align:center;border-left:1px solid rgba(255,255,255,0.1);padding-left:40px;">
    <div class="total-num" style="color:#fc5c5c">{domain_count}</div><div class="total-label">Domains</div>
  </div>
</div>

<div class="legend">
  <div class="legend-item"><div class="legend-dot" style="background:#5cfc7c"></div> Shipped</div>
  <div class="legend-item"><div class="legend-dot" style="background:#fcaa5c"></div> Now</div>
  <div class="legend-item"><div class="legend-dot" style="background:#4a9eff"></div> Next</div>
  <div class="legend-item"><div class="legend-dot" style="background:#7c5cfc"></div> Later</div>
  <div class="legend-item"><span class="card-ref card-g">#N</span> Board card</div>
  <div class="legend-item"><span class="role role-s">Silas</span><span class="role role-k">Kade</span><span class="role role-w">Wren</span> Owner</div>
</div>

<div class="phase-header">
  <div></div>
  <div class="phase-label phase-done">Shipped <span class="phase-count">{counts['done']}</span></div>
  <div class="phase-label phase-now">Now <span class="phase-count">{counts['now']}</span></div>
  <div class="phase-label phase-next">Next <span class="phase-count">{counts['next']}</span></div>
  <div class="phase-label phase-later">Later <span class="phase-count">{counts['later']}</span></div>
</div>
""")

# Build ring lookup for dividers
rings = {r["id"]: r for r in mapping["rings"]}
current_ring = None

for d in mapping["domains"]:
    ring_id = d["ring"]

    # Ring divider if new ring
    if ring_id != current_ring:
        current_ring = ring_id
        ring = rings.get(ring_id, {})
        ring_color = ring.get("color", "#888")
        ring_label = ring.get("label", ring_id)
        ring_icon = ring.get("icon", "")
        trust = ring.get("trust")
        trust_bg = ring.get("trustBg")
        trust_color = ring.get("trustColor")

        trust_html = ""
        if trust:
            trust_html = f'<span class="trust-tag" style="background:{trust_bg}; color:{trust_color};">{trust}</span>'

        html.append(f"""
<div class="ring-divider">
  <div></div>
  <div class="ring-divider-label" style="color:{ring_color};">{ring_icon} {ring_label}
    {trust_html}
  </div>
</div>
""")

    # Domain row
    icon = d.get("icon", "?")
    name = d["name"]
    color = d["color"]
    color_dark = d["colorDark"]
    stat = d.get("stat", "")

    # Split name for display
    name_parts = name.replace("&", "&amp;").split(",")
    name_html = "<br>".join(p.strip() for p in name_parts) if "," in name else name.replace("&", "&amp;")

    # --- Done column ---
    static_items = d.get("static", [])
    done_cards = []
    for key, title in d.get("cards", {}).items():
        if card_phase(key) == "done":
            done_cards.append((key, title))

    done_count = len(static_items) + len(done_cards)
    done_html = ""
    if done_count > 0:
        done_html += f'<div class="shipped-summary" onclick="toggleShipped(this)"><span class="expand-icon">&#9656;</span> {done_count} items shipped</div>\n<div class="shipped-items">\n'
        for s in static_items:
            done_html += f'<div class="item item-done"><span class="item-icon">&#9673;</span><span class="item-text">{s}</span></div>\n'
        for key, title in done_cards:
            owner = get_card_owner(key)
            done_html += f'<div class="item item-done"><span class="item-icon">&#9673;</span><span class="item-text">{title} {card_tag(key)}{role_tag(owner)}</span></div>\n'
        done_html += '</div>\n'

    # --- Now column ---
    now_items = []
    for key, title in d.get("cards", {}).items():
        if card_phase(key) == "now":
            owner = get_card_owner(key)
            now_items.append(f'<div class="item item-now"><span class="item-icon">&#9684;</span><span class="item-text">{title} {card_tag(key)}{role_tag(owner)}</span></div>')
    now_html = "\n".join(now_items)

    # --- Next column ---
    next_items = []
    for key, title in d.get("cards", {}).items():
        if card_phase(key) == "next":
            owner = get_card_owner(key)
            next_items.append(f'<div class="item item-next"><span class="item-icon">&#9671;</span><span class="item-text">{title} {card_tag(key)}{role_tag(owner)}</span></div>')
    next_html = "\n".join(next_items)

    # Check for empty Next+Later to show gap signal
    later_items = []
    for key, title in d.get("cards", {}).items():
        if card_phase(key) == "later":
            owner = get_card_owner(key)
            later_items.append(f'<div class="item item-later"><span class="item-icon">&#9675;</span><span class="item-text">{title} {card_tag(key)}{role_tag(owner)}</span></div>')
    for key, title in d.get("laterCards", {}).items():
        owner = get_card_owner(key)
        later_items.append(f'<div class="item item-later"><span class="item-icon">&#9675;</span><span class="item-text">{title} {card_tag(key)}{role_tag(owner)}</span></div>')
    for s in d.get("laterStatic", []):
        later_items.append(f'<div class="item item-later"><span class="item-icon">&#9675;</span><span class="item-text">{s}</span></div>')
    later_html = "\n".join(later_items)

    # Show gap signal for domains with no next AND no later
    if not next_items and not later_items and not now_items:
        next_html = '<div class="empty-signal">No cards queued</div>'
        later_html = '<div class="empty-signal">No cards planned</div>'

    html.append(f"""<div class="domain-row">
  <div class="d-label-cell">
    <div class="d-name">{name_html}<div class="d-stat">{stat}</div></div>
    <div class="d-icon" style="background:linear-gradient(135deg,{color},{color_dark});">{icon}</div>
  </div>
  <div class="cell cell-done">{done_html}</div>
  <div class="cell cell-now">{now_html}</div>
  <div class="cell cell-next">{next_html}</div>
  <div class="cell cell-later">{later_html}</div>
</div>
""")

# --- Tech Debt row ---
td = mapping.get("techDebt", {})
td_done_html = ""
td_done_items = td.get("doneStatic", [])
if td_done_items:
    td_done_html = f'<div class="shipped-summary" onclick="toggleShipped(this)"><span class="expand-icon">&#9656;</span> Resolved items</div>\n<div class="shipped-items">\n'
    for s in td_done_items:
        td_done_html += f'<div class="item item-done"><span class="item-icon">&#9673;</span><span class="item-text">{s}</span></div>\n'
    td_done_html += '</div>\n'

td_now_html = ""
td_next_items = []
for key, title in td.get("cards", {}).items():
    p = card_phase(key)
    owner = get_card_owner(key)
    tag = f'{card_tag(key)}{role_tag(owner)}'
    if p == "done":
        td_done_html += f'<div class="item item-done"><span class="item-icon">&#9673;</span><span class="item-text">{title} {tag}</span></div>\n'
    elif p == "now":
        td_now_html += f'<div class="item item-now"><span class="item-icon">&#9684;</span><span class="item-text">{title} {tag}</span></div>\n'
    else:
        td_next_items.append(f'<div class="item item-next"><span class="item-icon">&#9671;</span><span class="item-text">{title} {tag}</span></div>')

html.append(f"""
<div class="ring-divider">
  <div></div>
  <div class="ring-divider-label" style="color:#fc5c5c;">&#9888; Tech Debt &mdash; Cross-cutting</div>
</div>

<div class="domain-row">
  <div class="d-label-cell">
    <div class="d-name">Debt<br>Backlog<div class="d-stat">{len(td.get('cards', {}))} open items</div></div>
    <div class="d-icon" style="background:linear-gradient(135deg,#fc5c5c,#cc3333);">!</div>
  </div>
  <div class="cell cell-done">{td_done_html}</div>
  <div class="cell cell-now">{td_now_html}</div>
  <div class="cell cell-next">{chr(10).join(td_next_items)}</div>
  <div class="cell cell-later"></div>
</div>
""")

html.append(f"""
<footer>
  Gathering Roadmap &mdash; generated {now} from live board data &nbsp;|&nbsp;
  {domain_count} domains &nbsp;|&nbsp; {total_cards} board cards &nbsp;|&nbsp;
  Organized by concentric trust model (Core &rarr; Collections &rarr; Public)
</footer>

<script>
function toggleShipped(el) {{
  el.classList.toggle('expanded');
  var items = el.nextElementSibling;
  items.classList.toggle('expanded');
}}
</script>

</body>
</html>
""")

output = "\n".join(html)

for path in [output_local, output_app]:
    with open(path, 'w') as f:
        f.write(output)
    print(f"  Written to {path}")

PYEOF

if [ "$DRY_RUN" = false ]; then
  echo ""
  echo "Done. Gathering roadmap generated from live board data."
fi
