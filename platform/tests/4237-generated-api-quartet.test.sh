#!/usr/bin/env bash
# @test-type: integration — walks the generated manifests against a live API and
# CREATES, UPDATES and DELETES a throwaway row. Refuses production (exit 2,
# UNMEASURED) and refuses to run at all without an owner identity, because a
# refusal case measured anonymously measures nothing.
# #4237 — RUN the generated API test manifests.
#
# athena-make has projected a tests.json per class since #3467. Nothing ever
# executed one. A whole check layer that has never been able to go red, generated
# to a file nobody consumes. This is the consumer.
#
# What it runs, per class, in order: the quartet (create, read, update, delete),
# the refusal set, and the constraint cases. The create and update steps read the
# row back and compare the write-required fields one by one — a status code says
# the door answered, a read-back says it kept what it was given (#4167 found the
# store holding five hasDomain edges while the API returned none; no status
# assertion can see that).
#
# Cleanup is not optional. Every step works on the manifest's throwawaySubject and
# the run DELETEs it at the end. If the delete cannot be confirmed, the test FAILS
# rather than leaving a row behind — refuseIfNoCleanup in the manifest is the
# contract and this honours it.

set -uo pipefail

# Refuse production, same stance as catalog-curation-api.test.sh (#4187): that
# file wrote 90 rows into the live store because its default target WAS prod. A
# generated write suite pointed at prod would do the same thing every run, only
# faster. The membrane (#3615) refuses prod writes from test context through the
# generated API; curl goes around it, so the refusal lives here too.
API_BASE="${API_BASE:-http://localhost:3340}"
if [ "$API_BASE" = "http://localhost:3340" ] && [ "${CHORUS_ALLOW_PROD_TEST:-0}" != "1" ]; then
  echo "UNMEASURED: API_BASE is the production chorus-api." >&2
  echo "  This suite CREATES, UPDATES and DELETES rows. Point it at a werk variant." >&2
  echo "  Not a red — nothing was measured." >&2
  exit 2
fi

# Resolve from this file, never $CHORUS_ROOT: CHORUS_ROOT points at canonical, so
# a werk run would read canonical's manifests and report on code nobody edited
# (the #3701 ratchet-measured-CANONICAL defect).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"   # platform/tests -> repo root

shopt -s nullglob
MANIFESTS=( "$ROOT"/designing/products/*/domains/*/tests.json )
if [ "${#MANIFESTS[@]}" -eq 0 ]; then
  echo "FAIL: no generated tests.json found under designing/products/*/domains/*/" >&2
  echo "  The generator ran and landed nothing, or the land location moved." >&2
  exit 1
fi

pass=0; fail=0; skipped=0
check() {
  local desc="$1" want="$2" got="$3"
  if [ "$want" = "$got" ]; then
    pass=$((pass+1)); echo "  PASS: $desc"
  else
    fail=$((fail+1)); echo "  FAIL: $desc (expected: $want, got: $got)"
  fi
}

jqf() { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(eval(sys.argv[2],{'d':d,'json':json,'next':next}))" "$1" "$2" 2>/dev/null; }

# The manifest's `auth` field is the whole difference between a quartet step and a
# refusal case: POST /domains as owner must be 201, the same POST as nobody must be
# 401. A runner that ignores it sends identical requests for both and the two
# cases become one — every refusal then measures whatever the quartet measured.
# That is the hollow-check shape, so the header is not optional here.
#
# chorus-api reads `req.headers.authorization` (security-envelope.ts:130). Tokens
# come from platform/scripts/chorus-identity-token <role>.
TOKEN_BIN="${CHORUS_TOKEN_BIN:-$ROOT/platform/scripts/chorus-identity-token}"
OWNER_ROLE="${CHORUS_ROLE:-wren}"
OTHER_ROLE="kade"; [ "$OWNER_ROLE" = "kade" ] && OTHER_ROLE="silas"
token_for() {
  [ -x "$TOKEN_BIN" ] || return 1
  "$TOKEN_BIN" "$1" 2>/dev/null
}
OWNER_TOKEN="$(token_for "$OWNER_ROLE" || true)"
OTHER_TOKEN="$(token_for "$OTHER_ROLE" || true)"

# A suite that cannot mint an owner token cannot tell 201 from 401. Report
# UNMEASURED and stop — never run the refusals against an anonymous door and call
# the passes real.
if [ -z "$OWNER_TOKEN" ] && [ "${ALLOW_ANON_QUARTET:-0}" != "1" ]; then
  echo "UNMEASURED: no owner identity token (tried $TOKEN_BIN $OWNER_ROLE)." >&2
  echo "  Without it every request is anonymous and the refusal cases are meaningless." >&2
  exit 2
fi

auth_header() {
  case "$1" in
    owner)       [ -n "$OWNER_TOKEN" ] && printf 'Authorization: Bearer %s' "$OWNER_TOKEN" ;;
    other-owner) [ -n "$OTHER_TOKEN" ] && printf 'Authorization: Bearer %s' "$OTHER_TOKEN" ;;
    *)           : ;;   # none, or unset — send nothing
  esac
}

status_of() {
  local method="$1" path="$2" body="${3:-}" auth="${4:-none}"
  local hdr; hdr="$(auth_header "$auth")"
  local args=( -s -o /dev/null -w '%{http_code}' -X "$method" )
  [ -n "$hdr" ] && args+=( -H "$hdr" )
  if [ -n "$body" ]; then
    args+=( -H 'Content-Type: application/json' --data "$body" )
  fi
  curl "${args[@]}" "$API_BASE$path"
}

# Same request, but keep the response body.
body_of() {
  local method="$1" path="$2" body="${3:-}" auth="${4:-none}"
  local hdr; hdr="$(auth_header "$auth")"
  local args=( -s -X "$method" )
  [ -n "$hdr" ] && args+=( -H "$hdr" )
  if [ -n "$body" ]; then
    args+=( -H 'Content-Type: application/json' --data "$body" )
  fi
  curl "${args[@]}" "$API_BASE$path"
}

# THE READ-BACK. This is the half a status code cannot do.
#
# The door answering 201 says it accepted the request. It does not say it kept
# what it was given. #4167 found the store holding five hasDomain edges while the
# API returned none, and PUT is documented to drop properties the shape does not
# declare — both invisible to every status assertion we have ever written.
#
# So: send a known value for each compareField, read the row back, and compare
# field by field. A field that comes back missing or different is a FAIL naming
# the field, the value sent and the value returned.
compare_read_back() {
  local label="$1" get_path="$2" sent_json="$3" fields_json="$4"
  local got; got="$(body_of GET "$get_path" "" owner)"
  local report
  report=$(SENT="$sent_json" GOT="$got" FIELDS="$fields_json" python3 - <<'PY2'
import json, os, sys
try:
    sent = json.loads(os.environ["SENT"])
    fields = json.loads(os.environ["FIELDS"])
except Exception as e:
    print(f"UNREADABLE|could not parse what we sent: {e}"); sys.exit(0)
raw = os.environ.get("GOT", "")
try:
    got = json.loads(raw)
except Exception:
    print(f"UNREADABLE|the read-back was not JSON: {raw[:120]!r}"); sys.exit(0)
row = got.get("data", got)
if isinstance(row, list):
    row = row[0] if row else {}
if not isinstance(row, dict):
    print(f"UNREADABLE|the read-back was not an object: {type(row).__name__}"); sys.exit(0)
bad = []
for f in fields:
    want, have = sent.get(f), row.get(f)
    if want is None:
        continue          # we sent nothing for it; nothing to compare
    if have is None:
        bad.append(f"{f}: sent {want!r}, came back MISSING")
    elif str(have) != str(want):
        bad.append(f"{f}: sent {want!r}, came back {have!r}")
print("OK|" if not bad else "DIFF|" + " ; ".join(bad))
PY2
)
  case "$report" in
    OK\|*)         check "$label read-back: every compared field survived" "same" "same" ;;
    UNREADABLE\|*) fail=$((fail+1)); echo "  FAIL: $label read-back UNREADABLE — ${report#*|}" ;;
    *)             fail=$((fail+1)); echo "  FAIL: $label read-back lost or changed fields — ${report#*|}" ;;
  esac
}

for M in "${MANIFESTS[@]}"; do
  CLASS=$(jqf "$M" "d['class']")
  [ -n "$CLASS" ] || { echo "  SKIP: unreadable manifest $M"; skipped=$((skipped+1)); continue; }
  echo "--- $CLASS  ($(basename "$(dirname "$M")"))"

  SUBJECT=$(jqf "$M" "d['quartet']['throwawaySubject']")
  REFUSE=$(jqf "$M" "d['quartet']['refuseIfNoCleanup']")
  if [ "$REFUSE" != "True" ]; then
    echo "  FAIL: $CLASS manifest does not set refuseIfNoCleanup — a write suite must clean up"
    fail=$((fail+1)); continue
  fi

  # The quartet, in order.
  STEPS=$(jqf "$M" "len(d['quartet']['steps'])")
  for i in $(seq 0 $((STEPS-1))); do
    STEP=$(jqf "$M" "d['quartet']['steps'][$i]['step']")
    METHOD=$(jqf "$M" "d['quartet']['steps'][$i]['method']")
    PATH_=$(jqf "$M" "d['quartet']['steps'][$i]['path']")
    WANT=$(jqf "$M" "d['quartet']['steps'][$i]['expectStatus']")
    AUTH=$(jqf "$M" "d['quartet']['steps'][$i].get('auth','none')")
    FIELDS=$(jqf "$M" "json.dumps((d['quartet']['steps'][$i].get('readBack') or {}).get('compareFields', []))")
    SENT=""
    if [ "$FIELDS" != "[]" ] && [ -n "$FIELDS" ]; then
      # A known value per compared field, so the read-back has something to be
      # wrong about. The subject name is fixed by the manifest.
      SENT=$(FIELDS="$FIELDS" SUBJ="$SUBJECT" STEP="$STEP" python3 -c "
import json, os
fields = json.loads(os.environ['FIELDS'])
subj, step = os.environ['SUBJ'], os.environ['STEP']
print(json.dumps({'name': subj, **{f: f'{subj}-{step}-{f}' for f in fields}}))")
    fi
    GOT=$(status_of "$METHOD" "$PATH_" "$SENT" "$AUTH")
    check "$CLASS quartet $STEP: $METHOD $PATH_ (as $AUTH)" "$WANT" "$GOT"
    # Read it back only if the write was accepted — comparing fields on a row the
    # door refused would report a second failure for the same cause.
    if [ -n "$SENT" ] && [ "$WANT" = "$GOT" ]; then
      READ_PATH=$(jqf "$M" "next((x['path'] for x in d['quartet']['steps'] if x['step']=='read'), '')")
      [ -n "$READ_PATH" ] && compare_read_back "$CLASS $STEP" "$READ_PATH" "$SENT" "$FIELDS"
    fi
  done

  # The refusal set — every one of these asserts the door says NO.
  SECS=$(jqf "$M" "len(d['security'])")
  for i in $(seq 0 $((SECS-1))); do
    ID=$(jqf "$M" "d['security'][$i]['id']")
    METHOD=$(jqf "$M" "d['security'][$i]['method']")
    PATH_=$(jqf "$M" "d['security'][$i]['path']")
    WANT=$(jqf "$M" "d['security'][$i]['expectStatus']")
    BODY=$(jqf "$M" "d['security'][$i].get('body','')")
    case "$PATH_" in *:name*) PATH_="${PATH_/:name/$SUBJECT}";; esac
    AUTH=$(jqf "$M" "d['security'][$i].get('auth','none')")
    GOT=$(status_of "$METHOD" "$PATH_" "$BODY" "$AUTH")
    check "$CLASS refusal $ID (as $AUTH)" "$WANT" "$GOT"
  done

  # Cleanup, always, even if the steps above failed.
  DEL=$(jqf "$M" "next((s['path'] for s in d['quartet']['steps'] if s['step']=='delete'), '')")
  if [ -n "$DEL" ]; then
    CODE=$(status_of DELETE "$DEL" "" owner)
    case "$CODE" in
      204|404) echo "  cleanup: $SUBJECT gone ($CODE)";;
      *) echo "  FAIL: cleanup could not confirm $SUBJECT was removed (got $CODE)"; fail=$((fail+1));;
    esac
  fi
done

echo
echo "Result: $pass passed, $fail failed, $skipped skipped"
[ "$fail" -eq 0 ]
