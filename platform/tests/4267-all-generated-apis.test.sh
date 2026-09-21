#!/usr/bin/env bash
# @test-type: integration — walks EVERY generated collection and runs the full
# create/read/update/delete cycle against a live variant. Writes rows, so it
# refuses production and refuses to run without an owner identity.
#
# #4267 — Jeff: "done is one page, 53 green lines."
#
# #4237 built a runner for ONE landed manifest. athena-make can emit a manifest
# for every class it serves, and nothing ever asked it to, so 52 of the 53
# generated APIs had never had their write cycle executed even once. This asks
# for all of them and prints one line each.
#
# Why the cycle could never pass before today: the manifest named ten required
# fields and gave no way to tell `label` from `atStep`. A runner filled both with
# a made-up string and the door answered 422 unknown-target. The generator now
# emits `requiredFields` with a kind per field and the target class for edges
# (same change, same card), so this resolves a real subject for every edge from
# the live collection instead of inventing one.

set -uo pipefail

API="${API_BASE:-}"
if [ -z "$API" ]; then
  echo "UNMEASURED: set API_BASE to the variant's athena-make (e.g. http://localhost:3392)." >&2
  exit 2
fi
case "$API" in
  *:3360|*:3360/*)
    echo "UNMEASURED: API_BASE is production athena-make." >&2
    echo "  This suite CREATES, UPDATES and DELETES rows. Point it at a variant." >&2
    exit 2 ;;
esac

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GEN="${CHORUS_ATHENA_MAKE:-$ROOT/platform/services/athena-make/target/release/athena-make}"
[ -x "$GEN" ] || GEN="$(command -v athena-make || true)"
if [ -z "$GEN" ] || [ ! -x "$GEN" ]; then
  echo "UNMEASURED: no athena-make binary to generate manifests from." >&2
  exit 2
fi

TOKEN_BIN="${CHORUS_TOKEN_BIN:-$ROOT/platform/scripts/chorus-identity-token}"
OWNER="${CHORUS_ROLE:-wren}"
TOKEN="$([ -x "$TOKEN_BIN" ] && "$TOKEN_BIN" "$OWNER" 2>/dev/null || true)"
if [ -z "$TOKEN" ]; then
  echo "UNMEASURED: no owner identity token (tried $TOKEN_BIN $OWNER)." >&2
  echo "  Every request would be anonymous; a 401 would read as a refusal we meant." >&2
  exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

CLASSES="$(curl -sf --max-time 20 "$API/" | python3 -c 'import json,sys; print("\n".join(p["kind"] for p in json.load(sys.stdin)["primitives"]))' 2>/dev/null)"
if [ -z "$CLASSES" ]; then
  echo "UNMEASURED: $API/ served no primitive list — nothing to walk." >&2
  exit 2
fi
TOTAL="$(printf '%s\n' "$CLASSES" | wc -l | tr -d ' ')"

# One real subject of a class, for filling an edge. Empty when the collection is
# empty — the case then reports WHY rather than sending a made-up IRI.
resolve_edge() {
  local target="$1" path
  path="$("$GEN" generate-tests --class "$target" 2>/dev/null \
    | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin)
    print(next((r.split(" ",1)[1] for r in d["unit"]["routes"]
                if r.startswith("GET ") and ":name" not in r and not r.endswith("/batch")), ""))
except Exception: print("")' 2>/dev/null)"
  [ -n "$path" ] || return 0
  curl -sf --max-time 15 "$API$path" 2>/dev/null | python3 -c 'import json,sys
try:
    rows=json.load(sys.stdin).get("data") or []
    rows=rows if isinstance(rows,list) else []
    print((rows[0].get("name") or str(rows[0].get("iri","")).rsplit("#",1)[-1]) if rows else "")
except Exception: print("")' 2>/dev/null
}

pass=0; fail=0; unmeasured=0
printf '%-22s %-8s %s\n' CLASS RESULT DETAIL
printf '%-22s %-8s %s\n' "----------------------" "--------" "------"

for CLASS in $CLASSES; do
  M="$WORK/$CLASS.json"
  if ! "$GEN" generate-tests --class "$CLASS" >"$M" 2>/dev/null || [ ! -s "$M" ]; then
    printf '%-22s %-8s %s\n' "$CLASS" UNMEASURED "the generator emitted no manifest"
    unmeasured=$((unmeasured+1)); continue
  fi

  SUBJ="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["quartet"]["throwawaySubject"])' "$M" 2>/dev/null)"
  REQ="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1])).get("requiredFields") or []))' "$M" 2>/dev/null)"
  if [ "$REQ" = "[]" ]; then
    printf '%-22s %-8s %s\n' "$CLASS" UNMEASURED "manifest carries no requiredFields — regenerate with #4267 athena-make"
    unmeasured=$((unmeasured+1)); continue
  fi

  # Fill the body: literals get a marked value, edges get a real subject.
  BODY="$WORK/$CLASS.body.json"; MISSING=""
  : >"$WORK/$CLASS.pairs"
  while IFS=$'\t' read -r FIELD KIND TARGET; do
    [ -n "$FIELD" ] || continue
    if [ "$KIND" = "edge" ]; then
      V="$(resolve_edge "$TARGET")"
      if [ -z "$V" ]; then MISSING="$MISSING $FIELD->$TARGET"; continue; fi
    else
      V="zz-4267-$SUBJ-$FIELD"
    fi
    printf '%s\t%s\n' "$FIELD" "$V" >>"$WORK/$CLASS.pairs"
  done < <(python3 -c 'import json,sys
for f in json.load(open(sys.argv[1])).get("requiredFields") or []:
    print("\t".join([f["field"], f.get("kind","literal"), f.get("targetClass","")]))' "$M")

  if [ -n "$MISSING" ]; then
    printf '%-22s %-8s %s\n' "$CLASS" UNMEASURED "no row to point a required edge at:$MISSING"
    unmeasured=$((unmeasured+1)); continue
  fi

  python3 -c 'import json,sys
b={"name": sys.argv[2]}
for line in open(sys.argv[1]):
    k,_,v=line.rstrip("\n").partition("\t")
    if k: b[k]=v
json.dump(b, open(sys.argv[3],"w"))' "$WORK/$CLASS.pairs" "$SUBJ" "$BODY"

  DETAIL=""; OK=1
  while IFS=$'\t' read -r STEP METHOD PATH_ WANT; do
    [ -n "$STEP" ] || continue
    if [ "$METHOD" = "GET" ] || [ "$METHOD" = "DELETE" ]; then
      GOT="$(curl -s --max-time 25 -o "$WORK/resp" -w '%{http_code}' -X "$METHOD" \
        -H "Authorization: Bearer $TOKEN" "$API$PATH_")"
    else
      GOT="$(curl -s --max-time 25 -o "$WORK/resp" -w '%{http_code}' -X "$METHOD" \
        -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
        --data @"$BODY" "$API$PATH_")"
    fi
    if [ "$GOT" != "$WANT" ]; then
      OK=0
      DETAIL="$STEP $METHOD wanted $WANT got $GOT — $(head -c 120 "$WORK/resp" | tr -d '\n')"
      break
    fi
    # A status says the door answered; a read-back says it KEPT what it was given.
    if [ "$STEP" = "create" ] || [ "$STEP" = "update" ]; then
      RB="$(python3 -c 'import json,sys
m=json.load(open(sys.argv[1]))
print(next((s["path"] for s in m["quartet"]["steps"] if s["step"]=="read"), ""))' "$M")"
      [ -n "$RB" ] || continue
      DIFF="$(curl -sf --max-time 25 -H "Authorization: Bearer $TOKEN" "$API$RB" 2>/dev/null \
        | python3 -c 'import json,sys
sent=json.load(open(sys.argv[1]))
try: env=json.load(sys.stdin)
except Exception: print("read-back was not JSON"); sys.exit(0)
row=env.get("data") or {}
row=(row[0] if isinstance(row,list) and row else row)
if not isinstance(row,dict): print("read-back was not an object"); sys.exit(0)
links=env.get("links") or {}
bad=[]
for k,v in sent.items():
    if k=="name": continue
    have=row.get(k, links.get(k))
    if have is None: bad.append(f"{k} came back MISSING")
    elif str(have).replace("chorus:","")!=str(v).replace("chorus:",""):
        bad.append(f"{k} sent {v!r} got {have!r}")
print("; ".join(bad))' "$BODY")"
      if [ -n "$DIFF" ]; then OK=0; DETAIL="$STEP read-back: $DIFF"; break; fi
    fi
  done < <(python3 -c 'import json,sys
for s in json.load(open(sys.argv[1]))["quartet"]["steps"]:
    print("\t".join([s["step"], s["method"], s["path"], str(s["expectStatus"])]))' "$M")

  # Never leave a row behind, whatever happened above.
  curl -s --max-time 20 -o /dev/null -X DELETE -H "Authorization: Bearer $TOKEN" \
    "$API$(python3 -c 'import json,sys
print(next((s["path"] for s in json.load(open(sys.argv[1]))["quartet"]["steps"] if s["step"]=="delete"), ""))' "$M")" 2>/dev/null || true

  if [ "$OK" = 1 ]; then
    printf '%-22s %-8s %s\n' "$CLASS" PASS "create read update delete, fields survived"
    pass=$((pass+1))
  else
    printf '%-22s %-8s %s\n' "$CLASS" FAIL "$DETAIL"
    fail=$((fail+1))
  fi
done

echo
echo "$pass pass · $fail fail · $unmeasured unmeasured — of $TOTAL generated APIs"
[ "$fail" -eq 0 ] || exit 1
[ "$unmeasured" -eq 0 ] || exit 2
exit 0
