#!/usr/bin/env bash
# board-3654-project.sh — the read-side Vikunja→graph board projection (#3654 AC2/AC3/AC4).
#
# Reads the OPEN board (Now/WIP/Next/Blocked/Later) via the cards CLI and projects it
# into the graph through the governed DAL (chorus-model) — never raw SPARQL:
#   • one chorus:Chunk per chunk label carrying open cards (slug=label, ownedBy=dominant
#     owner of its open cards, roleSequence assigned per role)
#   • one THIN chorus:Card stub per referenced card (id+label only — Vikunja stays SoR,
#     the extension-table boundary; NO status/priority/owner mirror)
#   • one chorus:ChunkMembership per card-in-chunk with rank (deterministic order:
#     status Now<WIP<Next<Blocked<Later, then priority asc, then id asc)
#
# Sequencing policy (Wren, PM lane — re-sequenceable conversationally, that IS the product):
#   • security = roleSequence 1 for its owner + loomSequence 1 (AC4, Jeff's security-first)
#   • other chunks per role: open-card count desc (bigger commitment first), ties by name
#   • loomSequence: ONLY security gets one — the loom axis grows by declaration, not default
#
# Idempotent: DAL writes are DELETE-WHERE+INSERT on deterministic IRIs
# (chunk-<slug>, card-<id>, chunkmembership-<slug>-<id>) — re-run converges.
# Uniqueness (dup rank / roleSequence / loomSequence) is REFUSED at the door (#3681);
# a refusal here is a bug in this script's assignment, and it fail-louds.
set -u

CARDS="${CARDS_CLI:-/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards}"
CM="${CHORUS_MODEL_BIN:-$(command -v chorus-model 2>/dev/null || echo target/release/chorus-model)}"
DRY="${DRY_RUN:-0}"

# Fuseki writes require the write credential (#3630) — same sourcing as
# chorus-model-deploy.sh; exports FUSEKI_ADMIN_USER/PASSWORD for the DAL's curl.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/fuseki-auth.sh"

listing="$("$CARDS" list 2>/dev/null)" || { echo "FATAL: cards list failed" >&2; exit 1; }
[ -n "$listing" ] || { echo "FATAL: empty board listing" >&2; exit 1; }

plan="$(BOARD_LISTING="$listing" python3 - <<'PY'
import sys, os, re, html
from collections import defaultdict

STATUS_W = {"Now":0, "WIP":1, "Next":2, "Blocked":3, "Later":4}
sec, cards, chunks = None, {}, defaultdict(list)
for line in os.environ["BOARD_LISTING"].splitlines():
    m = re.match(r'^(Now|WIP|Next|Blocked|Later|Done|Won.t Do|Harvesting|SWAT) \(', line)
    if m: sec = m.group(1); continue
    if sec not in STATUS_W: continue
    m = re.match(r'^\s+(\d+)\s+(.*?)\s+\[(Wren|Silas|Kade|Jeff)\|(P\d)', line)
    if not m: continue
    cid, title, owner, prio = m.group(1), m.group(2), m.group(3).lower(), int(m.group(4)[1])
    title = html.unescape(title).replace('"', "'").strip()
    cards[cid] = title
    for ch in re.findall(r'chunk:([a-z-]+)', line):
        chunks[ch].append((STATUS_W[sec], prio, int(cid), cid, owner))

# dominant owner per chunk (open cards), ties broken alphabetically for determinism
owner_of = {}
for ch, members in chunks.items():
    tally = defaultdict(int)
    for *_, o in members: tally[o] += 1
    owner_of[ch] = sorted(tally.items(), key=lambda kv: (-kv[1], kv[0]))[0][0]

# roleSequence: security pinned #1 for its owner (AC4); rest by size desc, name asc
by_role = defaultdict(list)
for ch in chunks: by_role[owner_of[ch]].append(ch)
role_seq = {}
for role, chs in by_role.items():
    ordered = sorted(chs, key=lambda c: (0 if c == "security" else 1, -len(chunks[c]), c))
    for i, c in enumerate(ordered, 1): role_seq[c] = i

for ch in sorted(chunks):
    loom = "\tloom=1" if ch == "security" else ""
    print(f"CHUNK\t{ch}\t{owner_of[ch]}\t{role_seq[ch]}{loom}")
emitted = set()
for ch in sorted(chunks):
    for rank, (_, _, _, cid, _) in enumerate(sorted(chunks[ch]), 1):
        if cid not in emitted:
            emitted.add(cid)
            print(f"CARD\t{cid}\t{cards[cid]}")
        print(f"MEMBER\t{ch}\t{cid}\t{rank}")
PY
)" || { echo "FATAL: board parse failed" >&2; exit 1; }

W=0; R=0
run(){  # run <desc> <chorus-model args...> — fail-loud on refusal
  local d="$1"; shift
  if [ "$DRY" = "1" ]; then echo "DRY: $d"; return 0; fi
  local out; out="$("$CM" add "$@" 2>&1)" || { echo "REFUSED: $d → $out" >&2; R=$((R+1)); return 1; }
  W=$((W+1))
}

while IFS=$'\t' read -r kind a b c d; do
  case "$kind" in
    CHUNK)
      args=(--kind chunk --name "$a" --field "label=$a" --field "slug=$a" \
            --field "roleSequence=$c" --edge "ownedBy=role:$b")
      [ "$d" = "loom=1" ] && args+=(--field "loomSequence=1")
      run "chunk $a (owner=$b seq=$c ${d:-})" "${args[@]}" ;;
    CARD)
      run "card $a" --kind card --name "$a" --field "label=$b" ;;
    MEMBER)
      run "membership $a#$c → card $b" --kind chunkmembership --name "$a-$b" \
          --field "rank=$c" --edge "inChunk=chunk:$a" --edge "hasCard=card:$b" ;;
  esac
done <<<"$plan"

echo "board projection: $W writes, $R refusals"
[ "$R" -eq 0 ] || exit 1
