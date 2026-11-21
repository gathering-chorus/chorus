#!/usr/bin/env bats
# @test-type: unit — node only, no browser, no network, no credentials.
# @domain: identity — the login probe walks Jeff's whole path, sign-out included (#5026 AC2)
#
# Since #5015 each probe sign-in opens a Session row at the door. The flows signed
# in and never signed out (the sign-out scenario started from an empty jar and
# skipped), so by 11-21 50 probe rows sat open and failed OpenSessionActsAsARole.
# _auth.cjs now remembers a page it signed in, and its `test` signs that page out
# when the test ends. A fake door below counts open sessions: a sign-in opens one,
# a visit to /chorus/_auth/logout closes it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
AUTH="$ROOT/proving/flows/_auth.cjs"

# Walk one fake sign-in; $1 = "signout" to end the walk the way the fixture does.
# Prints the door's open-session count and the URLs visited.
walk() {
  FLOW_USER=probe FLOW_PASS=x node -e '
    const auth = require(process.argv[1]);
    let open = 0; const visits = []; let url = "https://h/login";
    const el = { first: () => el, waitFor: async () => {}, fill: async () => {},
                 click: async () => { open += 1; url = "https://h/chorus"; } };
    const page = {
      waitForLoadState: async () => {}, locator: () => el, url: () => url,
      waitForURL: async () => {},
      goto: async (u) => { visits.push(u); if (u.endsWith("/chorus/_auth/logout")) open -= 1; },
    };
    (async () => {
      await auth.signIn(page);
      if (process.argv[2] === "signout") await auth.signOutIfSignedIn(page, "https://h/");
      console.log(`open=${open} visits=${visits.join(",")}`);
    })();
  ' "$AUTH" "${1:-}"
}

@test "5026 AC2: a walk that signs in ends with its session closed" {
  run walk signout
  [ "$status" -eq 0 ]
  [ "$output" = "open=0 visits=https://h/chorus/_auth/logout" ]
}

@test "5026 AC2 NEGATIVE PROOF: a walk that skips sign-out leaves an open session" {
  run walk
  [ "$status" -eq 0 ]
  [ "$output" = "open=1 visits=" ]
}

@test "5026 AC2: a page that never signed in is not signed out" {
  run node -e '
    const auth = require(process.argv[1]); const visits = [];
    auth.signOutIfSignedIn({ goto: async (u) => visits.push(u) }, "https://h")
      .then((did) => console.log(`did=${did} visits=${visits.length}`));
  ' "$AUTH"
  [ "$output" = "did=false visits=0" ]
}

# Every spec that signs in must take `test` from _auth.cjs, whose fixture signs it out.
# Prints each offending file; exits 1 if any.
gate() {
  local dir="$1" bad=0 f
  for f in "$dir"/*.spec.cjs; do
    grep -qE '\b(signIn|gotoSignedIn)\(' "$f" || continue
    if grep -qE "const \{[^}]*\btest\b[^}]*\} = require\('@playwright/test'\)" "$f" \
       || ! grep -qE "const \{[^}]*\btest\b[^}]*\} = require\('\./_auth\.cjs'\)" "$f"; then
      echo "signs in without the sign-out test: ${f##*/}"; bad=1
    fi
  done
  return $bad
}

@test "5026 AC2: every flow that signs in uses the test that signs it out" {
  run gate "$ROOT/proving/flows"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "5026 AC2 NEGATIVE PROOF: a flow that signs in with playwright's own test is caught" {
  d="$BATS_TEST_TMPDIR/flows"; mkdir -p "$d"
  cat > "$d/bad.spec.cjs" <<'SPEC'
const { test, expect } = require('@playwright/test');
const { gotoSignedIn } = require('./_auth.cjs');
test('x', async ({ page }) => { await gotoSignedIn(page, '/chorus'); });
SPEC
  run gate "$d"
  [ "$status" -eq 1 ]
  [ "$output" = "signs in without the sign-out test: bad.spec.cjs" ]
}
