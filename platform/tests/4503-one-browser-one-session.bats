#!/usr/bin/env bats
# @test-type: unit:security — loads the door's real code with stub I/O; no network, no store, no running guard
# @domain: identity
# @card: #4503
#
# Jeff, 2026-10-12 10:49: five open sessions for one person, "no this is a bandaid".
# One browser holds one session: signing in again from it replaces the session
# instead of adding a second. The check counts the rows left open.
#
# Covers: platform/scripts/chorus-share-guard.py
# Covers: platform/tests/fixtures/one-browser-4503.py
# Also checks the access line: a UTC time and the user agent (the 2026-10-12 loop had neither).

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  FIX="$ROOT/platform/tests/fixtures/one-browser-4503.py"
  GUARD="$ROOT/platform/scripts/chorus-share-guard.py"
}

@test "two sign-ins from one browser leave one open session; a second browser is its own" {
  run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 0 ]
  if printf '%s\n' "$output" | grep -q '^FAIL'; then return 1; fi
}

@test "the sign-in route uses the replace, so the check covers what a browser hits" {
  grep -q 'sid = sign_in_session(self._session_claims(), webid, tok)' "$GUARD"
}

@test "NEGATIVE PROOF: a door that opens a session without closing this browser's old one fails" {
  work="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$work" && cp -R "$ROOT/platform/scripts/lib" "$work/lib"
  sed '/if replaced_sid(old, webid):/{N;d;}' "$GUARD" > "$work/chorus-share-guard.py"
  # the edit must have happened, or this proof proves nothing
  if grep -q 'if replaced_sid(old, webid):' "$work/chorus-share-guard.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work/chorus-share-guard.py"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q '^FAIL two sign-ins from one browser leave one open session'
}

@test "every branch of a sign-in names its step in the trace: started, returned, signed-in, failed, abandoned, logout" {
  for step in login.started login.returned login.signed-in login.abandoned logout; do
    grep -q "login_trace(\"$step\"" "$GUARD" || { echo "no trace for $step"; return 1; }
  done
  for why in unknown-or-expired-state token-exchange no-webid; do
    grep -q "login_trace(\"login.failed\", state, reason=\"$why\"" "$GUARD" || { echo "no failed trace for $why"; return 1; }
  done
}
