#!/usr/bin/env bash
# @domain: policies
# #4496 — put the policy rows from designing/data/policy-instances.ttl into the store,
# through the door. Rows live in the store and a land never replays them, so this
# runs once after the land that mounts /v1/policies/policies (PolicyShape).
# Default is a dry run that prints what it would send. --apply sends it.
#   new row      → POST  /v1/policies/policies        (label, comment, enforcedBy)
#   existing row → PATCH /v1/policies/policies/<name> (enforcedBy)
# Needs CHORUS_IDENTITY_TOKEN in the environment; the door refuses unsigned writes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BASE="${ATHENA_MAKE_URL:-http://localhost:3360}"
APPLY=0; [ "${1:-}" = "--apply" ] && APPLY=1
if [ "$APPLY" = 1 ] && [ -z "${CHORUS_IDENTITY_TOKEN:-}" ]; then
  echo "no CHORUS_IDENTITY_TOKEN in the environment; the door refuses unsigned writes" >&2; exit 2
fi
# A 404 on a row means "no such row" only once the route itself is served; before the
# land the whole route is unknown and every row would read as new.
route=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/v1/policies/policies")
if [ "$route" != 200 ]; then
  echo "the door does not serve /v1/policies/policies yet (answered $route); land #4496 first" >&2; exit 1
fi
riot --output=ntriples "$ROOT/designing/data/policy-instances.ttl" | python3 -c '
import json, sys, re
NS = "https://jeffbridwell.com/chorus#"
rows = {}
for line in sys.stdin:
    m = re.match(r"<([^>]+)> <([^>]+)> (.+) \.$", line.strip())
    if not m: continue
    s, p, o = m.groups()
    if not s.startswith(NS + "policy-"): continue
    r = rows.setdefault(s[len(NS) + len("policy-"):], {"enforcedBy": []})
    if p.endswith("#label"): r["label"] = json.loads(o)
    elif p.endswith("#comment"): r["comment"] = json.loads(o)
    elif p == NS + "enforcedBy": r["enforcedBy"].append(o.strip("<>")[len(NS) + len("code-file-"):])
for name, r in sorted(rows.items()):
    print(json.dumps({"name": name, **r}))
' | while IFS= read -r row; do
  name=$(printf '%s' "$row" | python3 -c 'import json,sys; print(json.load(sys.stdin)["name"])')
  got=$(curl -s -w '\n%{http_code}' "$BASE/v1/policies/policies/$name"); code=${got##*$'\n'}
  if [ "$code" = 200 ] && ! printf '%s' "${got%$'\n'*}" | grep -q '"ownedBy" *: *"principal-'; then
    # The 15 rows from before #4496 have no owner, and the door keeps an unowned row
    # closed to every caller (#4277). Name its owner once, then the door opens it.
    if [ "$APPLY" = 0 ]; then echo "would link $name ownedBy principal-wren"
    else athena-model link --kind policy --name "$name" --edge ownedBy=principal:wren; fi
  fi
  if [ "$code" = 200 ]; then
    body=$(printf '%s' "$row" | python3 -c 'import json,sys; r=json.load(sys.stdin); print(json.dumps({"enforcedBy": r["enforcedBy"]}))')
    [ "$body" = '{"enforcedBy": []}' ] && { echo "keep   $name (no hook)"; continue; }
    method=PATCH; url="$BASE/v1/policies/policies/$name"
  elif [ "$code" = 404 ]; then
    body="$row"; method=POST; url="$BASE/v1/policies/policies"
  else
    echo "FAIL   $name: the door answered $code on read" >&2; exit 1
  fi
  if [ "$APPLY" = 0 ]; then echo "would $method $name $body"; continue; fi
  out=$(curl -s -m 30 -X "$method" -H 'Content-Type: application/json' \
        -H "Authorization: Bearer $CHORUS_IDENTITY_TOKEN" --data-binary "$body" -w '\n%{http_code}' "$url")
  status=${out##*$'\n'}
  case "$status" in 2??) echo "$method $status $name" ;; *) echo "FAIL   $method $status $name: ${out%$'\n'*}" >&2; exit 1 ;; esac
done
