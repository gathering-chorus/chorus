#!/bin/bash
# @test-type: unit — sourced helper; its own proof is 4283-quartet-records-each-api.bats
# #4283 — the API quartet records ONE result per generated API per owner, so a
# red on /nightly names the endpoint and the store holds the walk. Before this
# the 18:36 run walked 55 APIs × 3 owners and recorded 5 cases.
#
#   quartet_record <api-base> <token> <owner> <class> <PASS|FAIL|NOT-PERM|UNMEASURED> <detail> [of-test-iri]
#
# Posts a TestResult row through the API's own collection. A refused or
# unreachable post is printed, never fatal: recording is a side channel of the
# walk, and the walk's own verdict stands. Disabled with QUARTET_RECORD=0.
quartet_record() {
  local api="$1" token="$2" owner="$3" class="$4" verdict="$5" detail="$6" of_test="${7:-}"
  [ "${QUARTET_RECORD:-1}" = "0" ] && return 0
  local result
  case "$verdict" in PASS) result=pass ;; FAIL) result=fail ;; *) result=unmeasured ;; esac
  local slug; slug="$(printf '%s' "$class" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9\n' '-' | tr -s '-')"
  local ts; ts="$(python3 -c 'import datetime;print(datetime.datetime.now().astimezone().isoformat(timespec="seconds"))')"
  local epoch; epoch="$(date +%s)"
  local name="testresult-4279-${owner}-${slug}-${epoch}"
  # the API takes an edge target as its LOCAL name, never the full IRI (422 otherwise)
  of_test="${of_test##*#}"
  local body
  body="$(python3 -c '
import json,sys
name,fp,tn,res,ts,of=sys.argv[1:7]
d={"name":name,"filePath":fp,"testName":tn,"result":res,"runTs":ts}
if of: d["ofTest"]=of
print(json.dumps(d))' "$name" "platform/tests/4279-api-quartet-prod.bats" "$owner: $class quartet" "$result" "$ts" "$of_test")"
  local code
  code="$(curl -s --max-time 20 -o "${QUARTET_RECORD_OUT:-/dev/null}" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    --data-binary "$body" "$api/v1/tests/results" 2>/dev/null || echo 000)"
  case "$code" in 2*) ;; *) echo "    (record: $owner $class $result -> HTTP $code, not stored)" ;; esac
}
