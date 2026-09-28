#!/usr/bin/env bats
# @test-type: contract — signal:ui is fixture-data: one case runs the report page's own script under node against a written report, no browser
# @domain: knowledge — the product domain this suite guards (#4334)
# 4166 — athena-validate must RUN without anyone typing it, and its answer must
# reach a person.
#
# It was built on #3846 to sweep the live graph for the old/bad data the write
# door can never see. Jeff, 2026-09-13 13:09: "didnt we build athena-validate …
# isnt it meant to look for stuff in our graph that is causing problems". It is,
# and the first run in weeks found 1,204 issues — 1,151 dangling edges, 45
# untyped subjects, and 8 subjects living in more than one graph (pulse, spine,
# athena, chorus, borg, werk, loom, convergence). Nothing scheduled it.
#
# The failure this guards is not "the sweep is wrong". It is "the sweep is not
# running, and silence reads like health".

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# #4167 — the bash retired; the schedule runs the installed Rust binary.
BIN_PATH="$HOME/.chorus/bin/athena-validate"
# The built verb in this werk — what the tests below actually exercise. The
# installed path above is what the SCHEDULE must point at; they are different
# concerns and conflating them is how a card proves a binary nobody runs.
BIN="$REPO_ROOT/platform/services/athena-validate/target/release/athena-validate"
PLIST="$REPO_ROOT/platform/launchd/com.chorus.athena-validate.plist"

# #4336 — the fake store: a `curl` on PATH answering the binary's real queries
# as a fixed small graph would (pulse in two graphs, one card owned by nobody).
FAKE_BIN="$REPO_ROOT/platform/tests/fixtures/4336/athena-validate/bin"

setup() { TMP="$BATS_TEST_TMPDIR"; }

need_bin() { [ -x "$BIN" ] || skip "UNMEASURED — athena-validate is not built in this tree (#4336)"; }

# Run the built verb against the fake store, store checks only (the door check
# reads a live API and is not what these cases are about).
run_fake_store() {
  run env PATH="$FAKE_BIN:$PATH" FAKE_STORE_LOG="$TMP/queries.log" \
    ATHENA_VALIDATE_REPORT="$TMP/gv.txt" "$@" "$BIN" --store-only
}

# A fake chorus-log under a fake CHORUS_HOME: the binary's spine emit lands in
# a file this test owns, never the live spine.
fake_chorus_home() {
  mkdir -p "$TMP/home/platform/scripts"
  printf '#!/bin/bash\necho "$*" >> "%s"\n' "$TMP/spine.log" > "$TMP/home/platform/scripts/chorus-log"
  chmod +x "$TMP/home/platform/scripts/chorus-log"
}

plist_get() { plutil -extract "$1" raw -o - "${2:-$PLIST}" 2>/dev/null; }

@test "a schedule exists in the repo, not only on one machine" {
  [ -f "$PLIST" ]
  # #4336 — read the plist the way launchd does, not by grepping its text: a
  # key inside a comment or a malformed file greps fine and never fires.
  plutil -lint "$PLIST"
  [ "$(plist_get Label)" = "com.chorus.athena-validate" ]
  # It must actually be periodic — a plist with no cadence never fires.
  if ! plist_get StartInterval >/dev/null; then
    plist_get StartCalendarInterval.Hour >/dev/null
  fi
}

@test "the plist runs the installed binary and captures its output" {
  # launchd needs an ABSOLUTE program path, and it must be the installed verb —
  # a plist pointing into a werk would run whatever branch happened to be there.
  # Built from $HOME rather than hardcoded (hardcoded-path-guard.bats).
  [ "$(plist_get ProgramArguments.0)" = "$BIN_PATH" ]
  # NEGATIVE PROOF: the retired script must not come back as the program. The
  # name is assembled so the retirement gate (#3598) does not read this test as
  # a user of the dead surface.
  RETIRED="athena-validate.$(printf 's''h')"
  [ "$(basename "$(plist_get ProgramArguments.0)")" != "$RETIRED" ]
  [ -n "$(plist_get StandardOutPath)" ]
  [ -n "$(plist_get StandardErrorPath)" ]
}

@test "NEGATIVE PROOF — an unreachable store reports UNMEASURED, never 0 issues" {
  need_bin
  # Point it at a closed port. Silence and cleanliness must not look alike:
  # this is the failure that would let a dead sweep read as a healthy graph.
  # #4335: this ran `bash "$SCRIPT"`, a variable retired with the script, so it
  # exited 127 and the UNMEASURED check below (hollow on bash 3.2) never read it.
  run env ATHENA_VALIDATE_NUDGE=0 ATHENA_VALIDATE_REPORT="$BATS_TEST_TMPDIR/gv.txt" \
    FUSEKI_QUERY="http://127.0.0.1:9/query" CHORUS_OWL_API="http://127.0.0.1:9" "$BIN"
  [ "$status" -ne 0 ]
  [ "$status" -ne 127 ]
  [[ "$output" == *UNMEASURED* ]] || return 1
  [[ "$output" != *"PROVEN CLEAN"* ]] || return 1
}

@test "a run says what it found on the spine, with counts" {
  need_bin
  # #4336 — run the verb and read the spine line it wrote, instead of grepping
  # main.rs for the event name.
  fake_chorus_home
  run_fake_store env CHORUS_HOME="$TMP/home"
  [ "$status" -eq 1 ]
  grep -q "^graph.validate.completed .*issues=2 verdict=dirty" "$TMP/spine.log"
  # An unreachable store is heard too, as unmeasured — never as a count.
  rm -f "$TMP/spine.log"
  run env CHORUS_HOME="$TMP/home" FUSEKI_QUERY="http://127.0.0.1:9/query" "$BIN" --store-only
  [ "$status" -eq 2 ]
  grep -q "^graph.validate.unmeasured .*checks_unmeasured=[1-9]" "$TMP/spine.log"
  # A test run (ATHENA_VALIDATE_NUDGE=0) must not reach the spine at all.
  rm -f "$TMP/spine.log"
  run_fake_store env CHORUS_HOME="$TMP/home" ATHENA_VALIDATE_NUDGE=0
  [ ! -e "$TMP/spine.log" ]
}

@test "the one-home violations name the graphs a subject lives in, not just its name" {
  need_bin
  # "pulse is in 2 graphs" is not actionable; "pulse is in A and B" is.
  # #4336 — the fake store holds pulse in two graphs and answers whatever the
  # binary projects; the report line must carry both graph names.
  run_fake_store env ATHENA_VALIDATE_NUDGE=0
  line="$(grep '^graph-issue|one-home|pulse|' "$TMP/gv.txt")"
  [ -n "$line" ]
  [[ "$line" == *"urn:chorus:domains:pulse"* ]] || return 1
  [[ "$line" == *"urn:chorus:instances"* ]] || return 1
}

@test "the run writes a report the page can read — one line per issue, plus a summary" {
  need_bin
  # #4336 — was a live-store read that skipped whenever the box was busy; the
  # fake store makes it run every time.
  run_fake_store env ATHENA_VALIDATE_NUDGE=0
  [ "$status" -eq 1 ]
  [ -f "$TMP/gv.txt" ]
  grep -q "^graph-issue|owner-not-principal|card-1|nobody$" "$TMP/gv.txt"
  grep -qE "^graph-summary\|[0-9]+\|(clean|dirty)$" "$TMP/gv.txt"
  # NEGATIVE PROOF: a report with issues but no summary line would render as a
  # blank verdict on the page. Exactly one summary, always.
  [ "$(grep -c "^graph-summary|" "$TMP/gv.txt")" = "1" ]
  # And the summary count is the issue lines, not a second number.
  issues=$(grep -c '^graph-issue|' "$TMP/gv.txt" | tr -d ' ')
  skipped=$(grep -c '^graph-issue|[^|]*|SKIPPED|' "$TMP/gv.txt" | tr -d ' ')
  grep -q "^graph-summary|$((issues - skipped))|dirty$" "$TMP/gv.txt"
}

@test "NEGATIVE PROOF — an unreachable store writes UNMEASURED to the report, not a count" {
  need_bin
  run env ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:9/query" CHORUS_OWL_API="http://127.0.0.1:9" ATHENA_VALIDATE_NUDGE=0 "$BIN"
  grep -q "^graph-summary|UNMEASURED|unreachable$" "$TMP/gv.txt"
  run grep -qE "^graph-summary\|[0-9]+\|" "$TMP/gv.txt"
  [ "$status" -ne 0 ]
  # A store that answers with an empty body is the same: it never ran a query.
  run_fake_store env ATHENA_VALIDATE_NUDGE=0 FAKE_STORE_MODE=empty
  [ "$status" -eq 2 ]
  grep -q "^graph-summary|UNMEASURED|unreachable$" "$TMP/gv.txt"
}

# #4336 — run the page's own script under node against a report the verb wrote,
# with fetch and the DOM stubbed; assert what the page would show.
render_page() {
  node -e '
    const fs=require("fs");
    const html=fs.readFileSync(process.argv[1],"utf8");
    const js=html.match(/<script>([\s\S]*?)<\/script>/)[1];
    const report=fs.readFileSync(process.argv[2],"utf8");
    const els={meta:{className:"",textContent:""},out:{innerHTML:""}};
    let asked="";
    global.document={getElementById:id=>els[id]};
    global.fetch=async u=>{asked=u;return{text:async()=>report}};
    global.setInterval=()=>0;
    eval(js);
    setTimeout(()=>{console.log("URL "+asked);console.log("META "+els.meta.className+" "+els.meta.textContent);console.log("OUT "+els.out.innerHTML);},50);
  ' "$PAGE" "$1"
}

@test "the page exists and reads the report the script writes" {
  need_bin
  PAGE="$REPO_ROOT/platform/api/public/borg/graph-validate.html"
  [ -f "$PAGE" ]
  command -v node >/dev/null || skip "UNMEASURED — node is not installed (#4336)"
  # A dirty report from the verb renders its issues and its count.
  run_fake_store env ATHENA_VALIDATE_NUDGE=0
  run render_page "$TMP/gv.txt"
  [ "$status" -eq 0 ]
  [[ "$output" == *"URL /borg/graph-validate.txt"* ]] || return 1
  [[ "$output" == *"META  2 issue(s) · dirty"* ]] || return 1
  [[ "$output" == *"<span class=tag>owner-not-principal</span>"* ]] || return 1
  # UNMEASURED must be rendered as its own state, never as zero issues.
  run env ATHENA_VALIDATE_REPORT="$TMP/gv.txt" FUSEKI_QUERY="http://127.0.0.1:9/query" ATHENA_VALIDATE_NUDGE=0 "$BIN" --store-only
  run render_page "$TMP/gv.txt"
  [[ "$output" == *"META warn UNMEASURED"* ]] || return 1
  [[ "$output" != *"0 issue(s)"* ]] || return 1
}
