#!/usr/bin/env bash
# cite-by-iri-lint.sh — Athena Move 0 cite-by-ID head-start (#2940, cookbook req-4).
#
# Catches paraphrased structural-class restatement in role CLAUDE.md / briefs /
# design docs and suggests the IRI rewrite. Operates on staged files only.
#
# Pattern: structural-class labels appearing as bare words ("loom", "athena",
# "borg", "werk") in narrative prose where the IRI (`chorus:loom`) would
# disambiguate. Move 4 will sweep + harden; this is the head-start lint.
#
# Exit codes: 0 if clean OR no staged files in scope; 1 if paraphrase found
# (advisory — does NOT block commit until Move 4 lands; until then surfaces
# warnings).

set -uo pipefail

TREE_PATH="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/data/athena/tree.json"
if [ ! -f "$TREE_PATH" ]; then
  if [ -f "data/athena/tree.json" ]; then
    TREE_PATH="data/athena/tree.json"
  else
    echo "cite-by-iri-lint: tree.json not found — skipping (Move 0 not yet seeded here)" >&2
    exit 0
  fi
fi

# In-scope files: staged CLAUDE.md, briefs/*.md, designing/docs/*.{md,html}
FILES=$(git diff --cached --name-only --diff-filter=ACM 2>/dev/null \
  | grep -E '(CLAUDE\.md|briefs/.+\.md|designing/docs/.+\.(md|html))$' || true)

if [ -z "$FILES" ]; then
  exit 0
fi

# Python does the whole match — bash-3-portable (no associative arrays).
python3 - "$TREE_PATH" "$FILES" <<'PY'
import json, re, sys

tree_path = sys.argv[1]
files = [f for f in sys.argv[2].splitlines() if f.strip()]

with open(tree_path) as f:
    tree = json.load(f)

# Build label → iri map. Skip labels shorter than 4 chars (too generic to flag).
label_to_iri = {}
for kind in ('products', 'domains', 'services'):
    for n in tree.get(kind, []):
        label = (n.get('label') or '').strip()
        iri = (n.get('iri') or '').strip()
        if label and iri and len(label) >= 4:
            label_to_iri[label] = iri

found = 0
exclude_line_re = re.compile(r'(^\s*```|href=|src=|<code>|<pre>|<!--)')
for fpath in files:
    try:
        lines = open(fpath, encoding='utf-8', errors='replace').read().splitlines()
    except OSError:
        continue
    for n, line in enumerate(lines, 1):
        if exclude_line_re.search(line):
            continue
        for label, iri in label_to_iri.items():
            # Bare-word match: label flanked by non-alphanumeric or string edge.
            pat = re.compile(r'(?:^|[^A-Za-z0-9_-])' + re.escape(label) + r'(?:[^A-Za-z0-9_-]|$)')
            if pat.search(line) and iri not in line:
                found += 1
                print(f"{fpath}:{n}: paraphrase-candidate for '{label}' (IRI: {iri})")
                print(f"  {line.strip()[:120]}")
                break  # one flag per line; don't spam

if found:
    print("")
    print(f"cite-by-iri-lint: {found} paraphrase candidate(s) found (advisory — not blocking until Move 4).")
    print("Rewrite to cite IRI: e.g., 'loom' → '`chorus:loom`' (Loom).")
    sys.exit(1)
sys.exit(0)
PY
