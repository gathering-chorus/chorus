#!/usr/bin/env bats
# @test-type: unit — node only, no browser, no network, no credentials.
# @domain: identity — the login probe walks Jeff's whole path, sign-out included (#5026 AC2) · card #5021 (folded from 5026)
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

# Code review 11-21 on the first cut: only the default page was signed out, a test that had
# signed out was sent to logout again, and a failed logout turned a passing test red.
# Two fake pages on one fake door; $1 picks the scenario. Prints what the door saw.
end_of_walk() {
  FLOW_USER=probe FLOW_PASS=x node -e '
    const auth = require(process.argv[1]); const mode = process.argv[2];
    let open = 0; const visits = []; const warned = [];
    const mk = (name) => {
      let url = "https://h/login";
      const el = { first: () => el, waitFor: async () => {}, fill: async () => {},
                   click: async () => { open += 1; url = "https://h/chorus"; } };
      return { name, waitForLoadState: async () => {}, locator: () => el, url: () => url, waitForURL: async () => {},
               isClosed: () => mode === "closed" && name === "second",
               goto: async (u) => {
                 if (mode === "broken") throw new Error("door down");
                 visits.push(name); if (u.endsWith("/chorus/_auth/logout")) open -= 1; } };
    };
    (async () => {
      const first = mk("first"), second = mk("second");
      await auth.signIn(first); await auth.signIn(second);
      if (mode === "already") { await first.goto("https://h/chorus/_auth/logout"); auth.markSignedOut(first); visits.length = 0; }
      const done = await auth.signOutAll("https://h", (m) => warned.push(m));
      const again = await auth.signOutAll("https://h", (m) => warned.push(m));
      console.log(`open=${open} done=${done} again=${again} visits=${visits.join(",")} warned=${warned.length}`);
    })().catch((e) => { console.log(`THREW ${e.message}`); process.exit(1); });
  ' "$AUTH" "${1:-}"
}

@test "5026 AC2: the walk's end signs out every page it signed in, not only the default one" {
  run end_of_walk
  [ "$status" -eq 0 ]
  [ "$output" = "open=0 done=2 again=0 visits=first,second warned=0" ]
}

@test "5026 AC2: a page the test signed out itself is not sent to logout again" {
  run end_of_walk already
  [ "$status" -eq 0 ]
  [ "$output" = "open=0 done=1 again=0 visits=second warned=0" ]
}

# NEGATIVE PROOF: the sign-out really failing is said, never thrown (a passing test stays
# passing) and never counted as done
@test "5026 AC2 NEGATIVE PROOF: a logout that fails is warned about, not thrown and not counted" {
  run end_of_walk broken
  [ "$status" -eq 0 ]
  [ "$output" = "open=2 done=0 again=0 visits= warned=2" ]
}

@test "5026 AC2: a signed-in page closed before the walk ended is said, and the other is still signed out" {
  run end_of_walk closed
  [ "$status" -eq 0 ]
  [ "$output" = "open=1 done=1 again=0 visits=first warned=1" ]
}

# The sign-out scenario proves the visible control: it must assert it, with no bare-URL fallback.
@test "5026 AC2: the sign-out flow asserts the visible Sign out control and has no URL fallback" {
  f="$ROOT/proving/flows/login-journey.spec.cjs"
  blk="$(awk "/test\('sign out ends the session everywhere on the host'/,/^  }\);/" "$f")"
  [ -n "$blk" ]
  printf '%s\n' "$blk" | grep -qF "toBeVisible()" || return 1
  printf '%s\n' "$blk" | grep -qF "_auth/welcome" || return 1
  ! printf '%s\n' "$blk" | grep -qF "logoutUrl(" || return 1
  ! printf '%s\n' "$blk" | grep -qF "test.skip(" || return 1
}

# #4915 — playwright.config.cjs loads the flow creds through _auth.cjs's one loader: a creds
# file sets FLOW_USER for the runner; a value already in the env wins; no file sets nothing.
@test "#4915 playwright.config.cjs loads flow creds through the shared loader" {
  f="$BATS_TEST_TMPDIR/creds.env"; printf 'FLOW_USER=probe-4915\n' > "$f"
  run env -u FLOW_USER FLOW_CREDS_FILE="$f" node -e 'require(process.argv[1]); console.log(process.env.FLOW_USER || "unset")' "$ROOT/playwright.config.cjs"
  [ "$output" = "probe-4915" ] || { echo "config did not load the creds file: $output"; return 1; }
  run env FLOW_USER=already FLOW_CREDS_FILE="$f" node -e 'require(process.argv[1]); console.log(process.env.FLOW_USER)' "$ROOT/playwright.config.cjs"
  [ "$output" = "already" ] || { echo "the env value did not win: $output"; return 1; }
  # NEGATIVE: no creds file, nothing is set
  run env -u FLOW_USER FLOW_CREDS_FILE="$BATS_TEST_TMPDIR/none.env" node -e 'require(process.argv[1]); console.log(process.env.FLOW_USER || "unset")' "$ROOT/playwright.config.cjs"
  [ "$output" = "unset" ] || { echo "set with no creds file: $output"; return 1; }
}
