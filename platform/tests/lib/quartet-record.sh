#!/bin/bash
# @test-type: unit — sourced helper; its own proof is 4283-quartet-records-each-api.bats
# #4283 — the API quartet records ONE result per generated API per owner, so a
# red names the endpoint. Before this the 18:36 run walked 55 APIs × 3 owners
# and recorded 5 cases.
#
#   quartet_record <api-base> <token> <owner> <class> <PASS|FAIL|NOT-PERM|UNMEASURED> <detail> [of-test-iri]
#
# #4579 — the result is a `quartet.result` event on the spine (Loki), not a
# TestResult row: Jeff 10-17 "deprecate logging in the graph". The api, token
# and of-test arguments stay so callers do not change; they are unused.
# A refused or unreachable emit is printed, never fatal: recording is a side
# channel of the walk, and the walk's own verdict stands. Disabled with
# QUARTET_RECORD=0. QUARTET_LOG names the spine writer (tests stub it).
quartet_record() {
  local owner="$3" class="$4" verdict="$5" detail="$6"
  [ "${QUARTET_RECORD:-1}" = "0" ] && return 0
  local result
  case "$verdict" in PASS) result=pass ;; FAIL) result=fail ;; *) result=unmeasured ;; esac
  local log="${QUARTET_LOG:-${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/platform/scripts/chorus-log}"
  bash "$log" quartet.result system "owner=$owner" "class=$class" "result=$result" \
    "file=platform/tests/4279-api-quartet-prod.bats" "detail=$detail" >/dev/null 2>&1 \
    || echo "    (record: $owner $class $result -> spine refused, not recorded)"
}
