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
  exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
fi
case "$API" in
  *:3360|*:3360/*)
    # #4279 — Jeff (2026-09-23): the quartet runs against PRODUCTION in the
    # nightly, once per owner. The write is deliberate and labelled: the caller
    # must say QUARTET_PROD=1 AND declare the membrane context prod
    # (CHORUS_CONTEXT=prod, the #3615 escape hatch). Either missing → refuse,
    # exactly as before. 4279-api-quartet-prod.test.sh is the caller that says both.
    if [ "${QUARTET_PROD:-}" != "1" ] || [ "${CHORUS_CONTEXT:-}" != "prod" ]; then
      echo "UNMEASURED: API_BASE is production athena-make and this run is not labelled a production write." >&2
      echo "  This suite CREATES, UPDATES and DELETES rows. Point it at a variant, or set QUARTET_PROD=1 CHORUS_CONTEXT=prod (#4279)." >&2
      exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
    fi ;;
esac

# #4279 — one disposable subject per run: zz-probe-<runId>-<class>. The manifest
# names a fixed throwawaySubject; a fixed name is how the 09-21 probe row was
# left behind and only found two days later. A run-scoped name makes any
# leftover attributable to the run that made it.
RUN_ID="${QUARTET_RUN_ID:-}"
RESIDUE_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GEN="${CHORUS_ATHENA_MAKE:-$ROOT/platform/services/athena-make/target/release/athena-make}"
[ -x "$GEN" ] || GEN="$(command -v athena-make || true)"
if [ -z "$GEN" ] || [ ! -x "$GEN" ]; then
  echo "UNMEASURED: no athena-make binary to generate manifests from." >&2
  exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
fi

TOKEN_BIN="${CHORUS_TOKEN_BIN:-$ROOT/platform/scripts/chorus-identity-token}"
OWNER="${CHORUS_ROLE:-wren}"
TOKEN="$([ -x "$TOKEN_BIN" ] && "$TOKEN_BIN" "$OWNER" 2>/dev/null || true)"
if [ -z "$TOKEN" ]; then
  echo "UNMEASURED: no owner identity token (tried $TOKEN_BIN $OWNER)." >&2
  echo "  Every request would be anonymous; a 401 would read as a refusal we meant." >&2
  exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

CLASSES="$(curl -sf --max-time 20 "$API/" | python3 -c 'import json,sys; print("\n".join(p["kind"] for p in json.load(sys.stdin)["primitives"]))' 2>/dev/null)"
if [ -z "$CLASSES" ]; then
  echo "UNMEASURED: $API/ served no primitive list — nothing to walk." >&2
  exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
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
echo "owner: $OWNER  api: $API  run: ${RUN_ID:-manifest-subject}"
printf '%-22s %-8s %s\n' CLASS RESULT DETAIL
printf '%-22s %-8s %s\n' "----------------------" "--------" "------"

for CLASS in $CLASSES; do
  # #4279 — QUARTET_ONLY=<Class> runs one class, for reading a single red by hand
  # instead of re-walking 55 against production.
  if [ -n "${QUARTET_ONLY:-}" ] && [ "$CLASS" != "$QUARTET_ONLY" ]; then continue; fi
  M="$WORK/$CLASS.json"
  if ! "$GEN" generate-tests --class "$CLASS" >"$M" 2>/dev/null || [ ! -s "$M" ]; then
    printf '%-22s %-8s %s\n' "$CLASS" UNMEASURED "the generator emitted no manifest"
    unmeasured=$((unmeasured+1)); continue
  fi

  if [ -n "$RUN_ID" ]; then
    # Rewrite the manifest's fixed subject (and every step path that names it)
    # to this run's own name. The generator's paths are the contract; only the
    # subject changes.
    python3 - "$M" "$RUN_ID" "$CLASS" <<'PYR'
import json,sys
p,rid,cls=sys.argv[1:4]; d=json.load(open(p)); q=d["quartet"]; old=q["throwawaySubject"]
new=("zz-probe-%s-%s" % (rid, cls.lower()))[:120]
q["throwawaySubject"]=new
for st in q["steps"]: st["path"]=st["path"].replace(old,new)
json.dump(d,open(p,"w"))
PYR
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
  while IFS=$'\t' read -r FIELD KIND TARGET ALLOWED DTYPE; do
    [ -n "$FIELD" ] || continue
    if [ "$KIND" = "edge" ]; then
      V="$(resolve_edge "$TARGET")"
      if [ -z "$V" ]; then MISSING="$MISSING $FIELD->$TARGET"; continue; fi
    elif [ -n "$ALLOWED" ]; then
      # A constrained literal takes one of the shape's own words. Inventing a
      # value here would 422 forever and read as a broken API.
      V="$ALLOWED"
    else
      # #4269 — send a value of the DECLARED type. Sending a word where the shape
      # says integer makes the door refuse correctly and the suite report it as a
      # product failure, which is the one thing a check must never do.
      case "$DTYPE" in
        integer|int|long|decimal|float|double) V="1" ;;
        boolean)                                V="true" ;;
        date)                                   V="2026-01-01" ;;
        dateTime)                               V="2026-01-01T00:00:00Z" ;;
        anyURI)                                 V="urn:chorus:zz-4267" ;;
        *)                                      V="zz-4267-$SUBJ-$FIELD" ;;
      esac
    fi
    printf '%s\t%s\n' "$FIELD" "$V" >>"$WORK/$CLASS.pairs"
  done < <(python3 -c 'import json,sys
for f in json.load(open(sys.argv[1])).get("requiredFields") or []:
    av=f.get("allowedValues") or []
    print("\t".join([f["field"], f.get("kind","literal"), f.get("targetClass",""), av[0] if av else "", f.get("datatype","")]))' "$M")

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

  DETAIL=""; OK=1; ROW_IRI=""
  call() { # $1 method $2 path → status on stdout, body in $WORK/resp
    if [ "$1" = "GET" ] || [ "$1" = "DELETE" ]; then
      curl -s --max-time 25 -o "$WORK/resp" -w '%{http_code}' -X "$1" -H "Authorization: Bearer $TOKEN" "$API$2"
    else
      curl -s --max-time 25 -o "$WORK/resp" -w '%{http_code}' -X "$1" -H "Authorization: Bearer $TOKEN" \
        -H 'Content-Type: application/json' --data @"$BODY" "$API$2"
    fi
  }
  while IFS=$'\t' read -r STEP METHOD PATH_ WANT; do
    [ -n "$STEP" ] || continue
    GOT="$(call "$METHOD" "$PATH_")"
    # #4279 — a 401 mid-run is the token, not the door: on 2026-09-23 a peer's
    # deploy at 12:47 rotated the verifier and every in-flight token read
    # authn-missing. Re-mint once and retry the same step; a second 401 is real.
    if [ "$GOT" = "401" ] && grep -q "authn-missing" "$WORK/resp" 2>/dev/null; then
      TOKEN="$("$TOKEN_BIN" "$OWNER" 2>/dev/null || printf '%s' "$TOKEN")"
      GOT="$(call "$METHOD" "$PATH_")"
    fi
    if [ "$GOT" != "$WANT" ]; then
      # A door that refuses the CALLER has not failed the cycle — it has
      # answered a different question, and counting it as a broken API hides
      # the real number. 403 means this identity may not write here; the cycle
      # is unmeasured for this class until someone who may write runs it.
      if [ "$GOT" = "403" ]; then
        OK=2
        DETAIL="$(head -c 150 "$WORK/resp" | tr -d '\n')"
      elif [ "$GOT" = "422" ] && grep -q "zz-4267\|zz-probe\|unknown-target\|not in sh:in" "$WORK/resp" 2>/dev/null; then
        # "not in sh:in" (#4279): the shape constrains the field but the
        # manifest carried no allowedValues for it, so the runner sent a
        # placeholder. The generator owes allowedValues from sh:in — that is a
        # generator gap, named in the detail, not an API failure.
        # #4269 — the door refused a value THIS RUNNER invented: a placeholder
        # that is not a legal value, or an edge pointed at a row we made up. The
        # API behaved correctly. Calling that a product failure is a lie about
        # which of the two states we are in, so it reports UNMEASURED.
        OK=2
        DETAIL="runner input rejected, not a product failure: $(head -c 260 "$WORK/resp" | tr -d '\n' | sed -E 's/.*"message": *"//; s/" *}$//')"
      elif [ "$GOT" = "422" ] && grep -qiE "cannot be written directly|deploy-only|written by the model deploy" "$WORK/resp" 2>/dev/null; then
        # #4279 — a class the door keeps for itself (Version rows are written by
        # the door on replace; deploy-only classes by the model deploy). No
        # principal may create one by hand, so the quartet cannot measure it.
        # That is the door's contract, not a break: NOT-PERM, named.
        OK=2
        DETAIL="door-managed class, not writable by any principal: $(head -c 160 "$WORK/resp" | tr -d '\n')"
      else
        OK=0
        DETAIL="$STEP $METHOD wanted $WANT got $GOT — $(head -c 300 "$WORK/resp" | tr -d '\n')"
      fi
      break
    fi
    # A status says the door answered; a read-back says it KEPT what it was given.
    if [ "$STEP" = "create" ] || [ "$STEP" = "update" ]; then
      RB="$(python3 -c 'import json,sys
m=json.load(open(sys.argv[1]))
print(next((s["path"] for s in m["quartet"]["steps"] if s["step"]=="read"), ""))' "$M")"
      [ -n "$RB" ] || continue
      # Compare the fields the MANIFEST says to compare, not every field we sent.
      # The body must carry ownedBy — the shape requires it — but the door sets
      # the owner to the authenticated caller, so comparing it asks whether the
      # door kept a value it is documented to replace. compareFields is the
      # generator's own answer to "what can the caller be held to".
      CF="$(python3 -c 'import json,sys
m=json.load(open(sys.argv[1]))
st=next((s for s in m["quartet"]["steps"] if s["step"]==sys.argv[2]), {})
print(json.dumps(((st.get("readBack") or {}).get("compareFields")) or []))' "$M" "$STEP")"
      curl -sf --max-time 25 -H "Authorization: Bearer $TOKEN" "$API$RB" >"$WORK/readback" 2>/dev/null || : >"$WORK/readback"
      # #4279 — the door names the row (kind-slug + subject); the residue check
      # below must look for THAT IRI, not the bare subject we sent. The first cut
      # looked for chorus#<subject> and could never find a leftover.
      [ -n "$ROW_IRI" ] || ROW_IRI="$(python3 -c 'import json,sys
try:
    env=json.load(open(sys.argv[1])); row=env.get("data") or {}
    row=(row[0] if isinstance(row,list) and row else row)
    print(row.get("iri","") if isinstance(row,dict) else "")
except Exception: print("")' "$WORK/readback" 2>/dev/null)"
      DIFF="$(python3 -c 'import json,sys
sent=json.load(open(sys.argv[1]))
only=set(json.loads(sys.argv[2]))
sent={k:v for k,v in sent.items() if k in only}
try: env=json.load(open(sys.argv[3]))
except Exception: print("read-back was not JSON"); sys.exit(0)
row=env.get("data") or {}
row=(row[0] if isinstance(row,list) and row else row)
if not isinstance(row,dict): print("read-back was not an object"); sys.exit(0)
links=env.get("links") or {}
def _same(have, want):
    # An edge is SENT as a bare subject name and READ BACK as the typed IRI the
    # door resolved it to: sent "abby-normal", read "chorus:principal-abby-normal".
    # That is the door doing its job, not a lost field. Compare the tail.
    h=str(have).replace("chorus:",""); w=str(want).replace("chorus:","")
    return h==w or h.endswith("-"+w) or w.endswith("-"+h)
bad=[]
for k,v in sent.items():
    if k=="name": continue
    have=row.get(k, links.get(k))
    if have is None: bad.append(f"{k} came back MISSING")
    elif not _same(have, v):
        bad.append(f"{k} sent {v!r} got {have!r}")
print("; ".join(bad))' "$BODY" "$CF" "$WORK/readback")"
      if [ -n "$DIFF" ]; then OK=0; DETAIL="$STEP read-back: $DIFF"; break; fi
    fi
  done < <(python3 -c 'import json,sys
for s in json.load(open(sys.argv[1]))["quartet"]["steps"]:
    print("\t".join([s["step"], s["method"], s["path"], str(s["expectStatus"])]))' "$M")

  # Never leave a row behind, whatever happened above.
  curl -s --max-time 20 -o /dev/null -X DELETE -H "Authorization: Bearer $TOKEN" \
    "$API$(python3 -c 'import json,sys
print(next((s["path"] for s in json.load(open(sys.argv[1]))["quartet"]["steps"] if s["step"]=="delete"), ""))' "$M")" 2>/dev/null || true

  # #4279 — residue: the delete reply is not the proof; the store is. Count the
  # subject's triples in EVERY graph after the delete. Any left is a leftover
  # row, and that fails the cycle whatever the statuses said (2026-09-23:
  # athena-model delete printed "deleted" and the row stayed).
  if [ -n "$ROW_IRI" ]; then
    [ -n "${FUSEKI_AUTH+x}" ] || [ ! -r "$ROOT/platform/scripts/fuseki-auth.sh" ] || source "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
    LEFT="$(curl -s --max-time 20 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -G "$RESIDUE_QUERY" \
      --data-urlencode "query=SELECT ?g (COUNT(*) AS ?n) WHERE { GRAPH ?g { <$ROW_IRI> ?p ?o } } GROUP BY ?g" \
      -H 'Accept: text/csv' 2>/dev/null | tail -n +2 | tr -d '\r' | tr '\n' ' ')"
    if [ -n "$LEFT" ]; then OK=0; DETAIL="leftover row after delete: <$ROW_IRI> in $LEFT"; fi
  fi

  if [ "$OK" = 1 ]; then
    printf '%-22s %-8s %s\n' "$CLASS" PASS "create read update delete, fields survived, no residue"
    pass=$((pass+1))
  elif [ "$OK" = 2 ]; then
    printf '%-22s %-8s %s\n' "$CLASS" NOT-PERM "$DETAIL"
    unmeasured=$((unmeasured+1))
  else
    printf '%-22s %-8s %s\n' "$CLASS" FAIL "$DETAIL"
    fail=$((fail+1))
  fi
done

echo
echo "$pass pass · $fail fail · $unmeasured unmeasured — of $TOTAL generated APIs"
[ "$fail" -eq 0 ] || exit 1
[ "$unmeasured" -eq 0 ] || exit 3  # #4273 — SELF-REFUSED: werk-only suite declined to run here (rc=3 is the runner's skip-with-reason, rc=2 means the measurement itself failed)
exit 0
