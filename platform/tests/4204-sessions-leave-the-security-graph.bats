#!/usr/bin/env bats
# @test-type: integration:security — talks to the live identity API and store on
# purpose: the question is what the RUNNING door allows, which no stub can answer.
#
# #4204. After #4202 landed, wren and kade could not log in:
#
#   POST /v1/identity/sessions -> 403
#   "no Write row for 'wren' on <urn:chorus:domains:security>"
#
# Session rows were landing in the security graph, so logging in required Write
# on the graph that holds Principals, Credentials and Permission rows. The grant
# that would have unblocked them also hands every role the power to rewrite who
# exists. Permission is graph-scoped (agent, accessTo, mode), so the fix is the
# rows' home, not a wider grant.
#
# These tests are worth nothing unless the SECOND half can fail: a run where the
# new grant is wide would pass test 1 and 2 happily.

setup() {
  CHORUS="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}"
  API="${ATHENA_MAKE_URL:-http://localhost:3360}"
  MINT="$CHORUS/platform/scripts/chorus-identity-token"
  [ -x "$MINT" ] || skip "no minter at $MINT"
  curl -s -o /dev/null --max-time 5 "$API/v1/identity/sessions" || skip "identity API not answering"
}

# Token on a 0600 file, never in argv where ps can read it.
hdr_for() {
  local role="$1" f
  f="$(mktemp)"; chmod 600 "$f"
  printf 'Authorization: Bearer %s\n' "$("$MINT" "$role" 2>/dev/null)" > "$f"
  printf '%s' "$f"
}

post_session() {
  local role="$1" h code now exp body
  h="$(hdr_for "$role")"
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"; exp="$(date -u -v+10M +%Y-%m-%dT%H:%M:%SZ)"
  body="$(mktemp)"
  printf '{"name":"%s-4204probe-%s","ownedBy":"principal-%s","tokenId":"t-4204-%s-%s","issuedAt":"%s","expiresAt":"%s","sessionState":"open","hostAccount":"probe"}' \
    "$role" "$$" "$role" "$role" "$$" "$now" "$exp" > "$body"
  code="$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
          -H "@$h" --data-binary "@$body" "$API/v1/identity/sessions")"
  rm -f "$h" "$body"
  # #4332 — remember what this test wrote, so teardown deletes it. Every run
  # used to leave two probe Session rows in production: 48 had built up by
  # 2026-09-26, half of all the Session rows the store held.
  printf '%s %s-4204probe-%s\n' "$role" "$role" "$$" >> "${BATS_FILE_TMPDIR}/4204-created"
  printf '%s' "$code"
}

teardown() {
  local role name h
  [ -f "${BATS_FILE_TMPDIR}/4204-created" ] || return 0
  while read -r role name; do
    [ -n "$name" ] || continue
    h="$(hdr_for "$role")"
    curl -s -o /dev/null --max-time 10 -X DELETE -H "@$h" "$API/v1/identity/sessions/$name" || true
    rm -f "$h"
  done < "${BATS_FILE_TMPDIR}/4204-created"
  : > "${BATS_FILE_TMPDIR}/4204-created"
}

post_principal() {
  local role="$1" h code body
  h="$(hdr_for "$role")"
  body="$(mktemp)"
  printf '{"name":"%s-4204-should-never-exist","webId":"https://example.invalid/x#me","principalKind":"agent","canSignIn":"false"}' "$role" > "$body"
  code="$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
          -H "@$h" --data-binary "@$body" "$API/v1/identity/principals")"
  rm -f "$h" "$body"
  printf '%s' "$code"
}

@test "wren can record a login" {
  code="$(post_session wren)"
  [ "$code" = "201" ]
}

@test "kade can record a login" {
  code="$(post_session kade)"
  [ "$code" = "201" ]
}

@test "NEGATIVE PROOF — the same grant does NOT let wren write a Principal" {
  # The route is /v1/identity/principals, NOT /v1/security/principals. The first
  # version of this test posted to the security path, got 404 "unknown route",
  # and failed — for the wrong reason. A refusal that is really a typo proves
  # nothing about authorization. Principals are ROUTED under identity and still
  # SERVED from urn:chorus:domains:security, which is exactly why the grant can
  # be narrow.
  # The whole point: logging in must not carry the power to say who exists.
  # If this ever returns 201, the grant is wide and the first two tests are
  # passing for a reason nobody wanted.
  code="$(post_principal wren)"
  [ "$code" != "201" ]
  [ "$code" = "403" ]
}

@test "NEGATIVE PROOF — nor kade" {
  code="$(post_principal kade)"
  [ "$code" != "201" ]
  [ "$code" = "403" ]
}

@test "#4332 no probe Session row this run wrote is left behind" {
  post_session wren >/dev/null
  teardown
  left="$(curl -s --max-time 10 "$API/v1/identity/sessions?limit=5000" | tr -d ' \n' | grep -o "wren-4204probe-$$" | head -1)"
  [ -z "$left" ]
}

@test "NEGATIVE PROOF — no Session row is left in the security graph" {
  # The move is only real if the rows stopped landing in the old home.
  served="$(curl -s --max-time 10 "$API/v1/identity/sessions" | tr -d ' \n')"
  case "$served" in
    *'"servedFrom":"urn:chorus:domains:identity"'*) : ;;
    *) printf 'servedFrom is not the identity graph: %s\n' "${served:0:200}" >&2; return 1 ;;
  esac
}
