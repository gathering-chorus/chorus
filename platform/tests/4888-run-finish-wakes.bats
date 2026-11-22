#!/usr/bin/env bats
# @test-type: unit — the generated pipelines' handlers, and run-finished against a caged nudge
# @domain: pipelines
# @card: #4888 · owner: silas
# What Jeff sees: he is never the one who notices a run finished. A run sends one
# wake when it finishes, pass or fail, to the owner and whoever waits on it (the
# navigator); a step in the middle that passes sends nothing. #4887's land at
# 14:35 woke nobody — Jeff found it at 14:37.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PIPES="$ROOT/platform/pipelines"
RF="$ROOT/platform/scripts/run-finished"

# A top-level pipeline is one a role starts (no TRACE param: a child's parent
# owns its finish). #4965 (Silas) — a pipeline that declares a schedule is a sensor,
# not a run a role waits on: it speaks through its readings and alerts
# (qual-monitor-alert-separation), and a */5 wake would be noise. Keyed on the
# schedule, never on names (this also covers the nightly, which has its own notify).
toplevel() {
  for f in "$1"/*.yaml; do
    grep -q '^  - TRACE: ' "$f" && continue
    grep -q '^schedule:' "$f" && continue
    echo "$f"
  done
}
handlers() { sed -n '/^handler_on:/,/^steps:/p' "$1"; }

# prints a problem line per pipeline whose finish wake is wrong
wake_problems() {
  for f in $(toplevel "$1"); do
    h="$(handlers "$f")"
    n="$(printf '%s\n' "$h" | grep -c 'platform/scripts/run-finished')"
    [ "$n" -eq 1 ] || echo "$(basename "$f"): $n run-finished calls"
    printf '%s\n' "$h" | grep -q '^  exit:' || echo "$(basename "$f"): no exit handler"
    printf '%s\n' "$h" | sed -n '/^  failure:/,/^  [a-z]*:/p' | grep -q 'ops-nudge' && echo "$(basename "$f"): failure handler still nudges"
  done
  return 0
}

@test "AC1/AC2: every top-level pipeline wakes once from its exit handler, through run-finished; its failure handler only routes" {
  out="$(wake_problems "$PIPES")"
  [ -z "$out" ] || { echo "$out"; return 1; }
  # the card's five (and the go) by name, so a misread heuristic can never drop one silently
  for p in pair cicd cicd-go cicd-land athena domain-flow; do
    toplevel "$PIPES" | grep -q "/$p.yaml$" || { echo "$p.yaml is not checked as a top-level pipeline"; return 1; }
  done
}

@test "children stay silent: demo and cicd-test have no wake of their own (the parent's exit is the one)" {
  for c in demo cicd-test; do
    h="$(handlers "$PIPES/$c.yaml")"
    if printf '%s\n' "$h" | grep -qE 'run-finished|ops-nudge'; then echo "$c: $h"; return 1; fi
  done
}

@test "real dagu: a failing child fails its parent, and the parent's exit runs run-finished once (the onExit gate is dagu's own name)" {
  cage
  d="$BATS_TEST_TMPDIR/dg"; mkdir -p "$d/home"
  printf 'params:\n  - TRACE: ""\nsteps:\n  - name: boom\n    command: "false"\n' > "$d/child.yaml"
  # dagu hands a handler only the env its workflow declares, as the generated ones do
  printf 'params:\n  - CARD: ""\n  - ROLE: ""\nenv:\n  - CHORUS_HOME: %s\n  - CHORUS_HOOK_SHIM: %s\n  - RUN_FINISHED_WHY: %s\nhandler_on:\n  exit:\n    command: bash %s ${CARD} ${ROLE} ${DAG_NAME} ${DAG_RUN_ID}\nsteps:\n  - name: call-child\n    command: env DAGU_HOME=%s/home dagu start %s/child.yaml -- TRACE=t\n  - name: after\n    depends: [call-child]\n    command: "true"\n' \
    "$H" "$BATS_TEST_TMPDIR/shim" "$BATS_TEST_TMPDIR/why" "$RF" "$d" "$d" > "$d/parent.yaml"
  (cd "$d" && env -u CHORUS_ROLE DAGU_HOME="$d/home" dagu start parent.yaml -- CARD=4888 ROLE=silas >/dev/null 2>&1) || true
  [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { cat "$SENT"; return 1; }
  grep -q '^silas|#4888 parent failed .*failed at' "$SENT" || { cat "$SENT"; return 1; }
}

@test "the pair ends at the land's prove, yet the land's wake still reaches the navigator, then the kept members go" {
  cage; printf '{"card":"4888","driver":"silas","navigator":"kade"}' > "$H/ops/active-pairs/4888.ended"
  run finish succeeded cicd-land
  [ "$(cut -d'|' -f1 "$SENT" | sort | tr '\n' ' ')" = "kade silas " ] || { cat "$SENT"; return 1; }
  [ ! -e "$H/ops/active-pairs/4888.ended" ] || return 1
}

@test "a wake that cannot go out is loud, lands on the spine as run.wake.failed, and never turns a passed run red" {
  cage; printf '#!/bin/sh\nexit 1\n' > "$BATS_TEST_TMPDIR/shim"
  printf '#!/bin/sh\necho "$@" >> %s/spine\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/log"; chmod +x "$BATS_TEST_TMPDIR/log"
  RUN_FINISHED_LOG="$BATS_TEST_TMPDIR/log" run finish succeeded cicd-land
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"not every wake went out"* ]] || { echo "$output"; return 1; }
  grep -q '^run.wake.failed system card=4888 dag=cicd-land to=silas status=succeeded' "$BATS_TEST_TMPDIR/spine" || { cat "$BATS_TEST_TMPDIR/spine"; return 1; }
}

# AC5 — the wake shows in Jeff's public room, loaded headless, with no message from
# him: the newest run-result wake in the room's record must be on the page. The
# #4887 room check does the load (a wake older than the page's rows: unmeasured).
ROOM_URL="${CLEARING_ROOM_URL:-https://lightlifeurbangardens.com/clearing-v2/}"
# AC5 — a run-finished wake shows in Jeff's public room. werk-test cages every
# test's nudges (CHORUS_PULSE_URL → 127.0.0.1:9), so no test sends a live wake
# (Wren 16:07: keep the cage). The proof is the room's own record: the newest
# run-finished wake there (format "#N <pipeline> passed|failed|landed · dagu …")
# must render on the page Jeff loads. One the page has scrolled past reads
# unmeasured. Before the land no real wake exists to check (the card's AC5 sits
# under "After land": the land's own wake proves it).
# #4889 fenced failure wakes (multi-line) + #4970 quiet pass wakes: match both single-line and fenced run-finished wakes.
WAKE_RE_JS='^\[alert → [a-z-]+\] #[0-9]+ [a-z-]+ (?:failed · \/cws [0-9]+\n```\n[\s\S]*· dagu [^ \n]+\n```.*|.*· dagu [^ \n]+ · \/cws [0-9]+)$'
@test "AC5: a real run-finished wake ingested into Clearing v2 requires sign-in on tunneled reads and renders on the room page" {
  cage
  # #4925 — the tree under test's own Clearing v2: a card's werk holds no dist unless the
  # card changed clearing-v2 (werk-build builds changed units only). tsc takes ~2 s.
  (cd "$ROOT/directing/clearing-v2" && npm run build >"$BATS_TEST_TMPDIR/cv2-build.log" 2>&1) \
    || { echo "clearing-v2 build failed:"; tail -5 "$BATS_TEST_TMPDIR/cv2-build.log"; return 1; }
  printf '#!/bin/sh\n[ "$1" = nudge-send ] && printf "%%s" "$4" > %s/raw_wake\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/shim"
  chmod +x "$BATS_TEST_TMPDIR/shim"
  finish failed cicd >/dev/null
  raw="$(cat "$BATS_TEST_TMPDIR/raw_wake")"
  case "$raw" in "[alert →"*) wake_text="$raw" ;; *) wake_text="[alert → silas] $raw" ;; esac

  run env WAKE_TEXT="$wake_text" WAKE_RE="$WAKE_RE_JS" ROOT="$ROOT" node - <<'NODE'
const http = require('http');
const crypto = require('crypto');
const { chromium } = require(process.env.ROOT + '/node_modules/playwright-core');
const { contentHash } = require(process.env.ROOT + '/platform/chorus-sdk/reply/hash.js');
const { createApp } = require(process.env.ROOT + '/directing/clearing-v2/dist/server.js');
const { mintShareSessionCookie } = require(process.env.ROOT + '/directing/clearing-v2/dist/share-session.js');

(async () => {
  const cookieKey = crypto.randomBytes(32);
  const { app, router, stopKeys } = createApp({
    cookieKey,
    knownKeys: new Map(),
    rolesUrl: 'http://127.0.0.1:9/roles',
    cardsUrl: 'http://127.0.0.1:9/cards',
    alertsUrl: 'http://127.0.0.1:9/alerts',
    pulseUrl: 'http://127.0.0.1:9/pulse',
    readingsUrl: 'http://127.0.0.1:9/readings',
    streamUrl: 'http://127.0.0.1:9/streams',
    relayUrl: '',
    signInUrl: 'https://example.invalid/login',
    readOnly: true,
  });
  router.ingest({ id: 'wake-4888', from: 'system', room: 'team', visible: true, text: process.env.WAKE_TEXT, ts: new Date().toISOString() });
  const srv = http.createServer(app);
  await new Promise((r) => srv.listen(0, '127.0.0.1', r));
  const port = srv.address().port;
  const base = `http://127.0.0.1:${port}`;
  let browser;
  try {
    // 1. Negative proof on the gate (#5002): tunneled anonymous read is refused (401), never 200.
    const anon = await fetch(`${base}/api/messages`, { headers: { 'cf-ray': 'test-4888' } });
    if (anon.status !== 401) throw new Error(`expected 401 without sign-in, got ${anon.status}`);

    // 2. Signed-in tunneled read gets 200 and matches WAKE_RE.
    const cookie = `chorus_share_session=${mintShareSessionCookie('https://jeffbridwell.com/chorus#principal-jeff', cookieKey)}`;
    const authed = await fetch(`${base}/api/messages`, { headers: { 'cf-ray': 'test-4888', cookie } });
    if (authed.status !== 200) throw new Error(`expected 200 with sign-in, got ${authed.status}`);
    const rec = await authed.json();
    const re = new RegExp(process.env.WAKE_RE);
    const w = rec.filter((m) => m.from === 'system' && re.test(m.text || '')).pop();
    if (!w) throw new Error(`real run-finished wake did not match WAKE_RE: ${JSON.stringify(process.env.WAKE_TEXT)}`);

    // 3. Headless Chrome loads the room with the signed-in cookie and renders #msg-wake-4888 on #messages-stream.
    browser = await chromium.launch({
      executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      headless: true,
      args: ['--use-mock-keychain', '--password-store=basic'],
    });
    const ctx = await browser.newContext({ extraHTTPHeaders: { 'cf-ray': 'test-4888', cookie } });
    const page = await ctx.newPage();
    const errs = [];
    page.on('pageerror', (e) => errs.push(e.message));
    await page.goto(`${base}/`, { waitUntil: 'load', timeout: 15000 });
    await page.waitForSelector(`#messages-stream #msg-${w.id}[data-from="system"]`, { timeout: 10000 });
    if (errs.length) throw new Error(`pageerror: ${errs.join('; ')}`);
    console.log(JSON.stringify({ ok: true, id: w.id, hash: contentHash(w.text) }));
  } finally {
    if (browser) await browser.close();
    stopKeys();
    await new Promise((r) => srv.close(r));
  }
})().catch((e) => { console.error(e.message); process.exit(1); });
NODE
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC5: the wake pattern matches run-finished's own words (single-line and #4889 fenced), and not an older nudge; old single-line regex fails on #4889 fenced wake (negative proof)" {
  cage
  printf '#!/bin/sh\n[ "$1" = nudge-send ] && printf "%%s" "$4" > %s/raw_wake\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/shim"
  chmod +x "$BATS_TEST_TMPDIR/shim"
  finish failed cicd >/dev/null
  raw="$(cat "$BATS_TEST_TMPDIR/raw_wake")"
  case "$raw" in "[alert →"*) fenced="$raw" ;; *) fenced="[alert → silas] $raw" ;; esac
  old_re='^\[alert → [a-z-]+\] #[0-9]+ [a-z-]+ .*· dagu [^ ]+ · \/cws [0-9]+$'
  node -e '
    const re = new RegExp(process.argv[1]);
    const oldRe = new RegExp(process.argv[2]);
    const fenced = process.argv[3];
    if (oldRe.test(fenced)) process.exit(10);
    if (!re.test(fenced)) process.exit(1);
    if (!re.test("[alert → abby-normal] #999999 clearing-evaluate passed · dagu 034rvgJDfrob5mIxbyTwg3 · /cws 999999")) process.exit(2);
    if (re.test("[alert → silas] werk v2 #4887 cicd failed at test-pipeline: x · dagu y · /cws 4887 · retry")) process.exit(3);
  ' "$WAKE_RE_JS" "$old_re" "$fenced"
}

@test "AC5 negative proof: the room check names a wake the page does not show" {
  d="$BATS_TEST_TMPDIR/room"; mkdir -p "$d"
  printf '<div id="messages-stream"><div class="message" id="msg-old" data-from="system" data-ts="2026-11-04T19:00:00.000Z"></div></div>' > "$d/index.html"
  printf '[{"id":"old","from":"system","text":"x","ts":"2026-11-04T19:00:00.000Z"},{"id":"w1","from":"system","text":"[alert → silas] #4888 cicd-land landed","ts":"2026-11-04T19:05:00.000Z"}]' > "$d/record.json"
  h="$(node -e 'console.log(require(process.argv[1]).contentHash("[alert → silas] #4888 cicd-land landed"))' "$ROOT/platform/chorus-sdk/reply/hash.js")"
  run env WANT_4887="system:$h" ROOM_RECORD_FILE="$d/record.json" node "$ROOT/platform/tests/lib/room-rows-4887.cjs" "$ROOT/node_modules/playwright-core" "file://$d/index.html"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *'"missing":[{"role":"system"'* ]] || { echo "$output"; return 1; }
}

@test "AC4 negative proof: a pipeline with no exit wake, or one that still nudges on failure, is named" {
  d="$BATS_TEST_TMPDIR/pipes"; mkdir -p "$d"
  printf 'handler_on:\n  failure:\n    command: ops-nudge ${ROLE} x\nsteps:\n' > "$d/old.yaml"
  printf 'handler_on:\n  exit:\n    command: "${CHORUS_HOME}/platform/scripts/run-finished ${CARD} ${ROLE} ${DAG_NAME} ${DAG_RUN_ID}"\nsteps:\n' > "$d/good.yaml"
  printf 'params:\n  - TRACE: ""\nhandler_on:\nsteps:\n' > "$d/child.yaml"
  out="$(wake_problems "$d")"
  [[ "$out" == *"old.yaml: 0 run-finished calls"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"old.yaml: failure handler still nudges"* ]] || { echo "$out"; return 1; }
  [[ "$out" != *"good.yaml"* && "$out" != *"child.yaml"* ]] || { echo "$out"; return 1; }
}

# --- run-finished, with chorus-hook-shim and chorus-werk-status caged ---
cage() {
  H="$BATS_TEST_TMPDIR/home"; mkdir -p "$H/ops/active-pairs"
  SENT="$BATS_TEST_TMPDIR/sent"; : > "$SENT"; : > "$SENT.wake"
  # one line per send: a wake's fenced reason spans lines (#4889)
  printf '#!/bin/sh\n[ "$1" = nudge-send ] && printf "%%s|%%s\\n" "$3" "$(printf %%s "$4" | tr "\\n" " ")" >> %s && echo "${CHORUS_NUDGE_WAKE:-none}" >> %s.wake && echo "${OPS_ALERT:-none}" >> %s.alert\n' "$SENT" "$SENT" "$SENT" > "$BATS_TEST_TMPDIR/shim"
  : > "$SENT.alert"
  # #5002 (Silas): the cage never reads the live Alert rows. ops-nudge looks up the
  # notify field of run-finished-pass (#4970: off, so a pass is quiet), which made
  # these cases depend on prod. An unreachable door fails open (ops-nudge's rule), so
  # the wake reaches the caged shim and each case checks run-finished's own words;
  # the row each wake names is recorded in $SENT.alert.
  export ALERTS_DOOR="http://127.0.0.1:9/alerts"
  printf '#!/bin/sh\necho "failed at test-pipeline: werk-test: RED (exit 1)"\n' > "$BATS_TEST_TMPDIR/why"
  chmod +x "$BATS_TEST_TMPDIR/shim" "$BATS_TEST_TMPDIR/why"
}
finish() { # STATUS DAG [STEP]
  env -u CHORUS_ROLE CHORUS_HOME="$H" CHORUS_HOOK_SHIM="$BATS_TEST_TMPDIR/shim" RUN_FINISHED_WHY="$BATS_TEST_TMPDIR/why" \
    DAG_RUN_STATUS="$1" DAG_RUN_STEP_NAME="${3:-onExit}" bash "$RF" 4888 silas "$2" run-1
}

@test "AC1: a failed run wakes the owner once, naming the step and the reason" {
  cage; run finish failed cicd
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { cat "$SENT"; return 1; }
  grep -q '^silas|#4888 cicd failed · /cws 4888 ``` failed at test-pipeline: werk-test: RED' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC1: a paired card's finish wakes the driver and the navigator, each once" {
  cage; printf '{"card":"4888","driver":"silas","navigator":"kade"}' > "$H/ops/active-pairs/4888.json"
  run finish failed pair
  [ "$(cut -d'|' -f1 "$SENT" | sort | tr '\n' ' ')" = "kade silas " ] || { cat "$SENT"; return 1; }
}

@test "AC3: a land that succeeds wakes with 'landed' and names the post-land step" {
  cage; run finish succeeded cicd-land
  [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { cat "$SENT"; return 1; }
  grep -q '^silas|#4888 landed, post-land: ' "$SENT" || { cat "$SENT"; return 1; }
}

@test "a model land's two finishes say two different things: landed, then the model in prod (Kade 16:24)" {
  cage; run finish succeeded cicd-land
  cage2="$(cat "$SENT")"
  cage; run finish succeeded athena-land
  [[ "$cage2" == *"#4888 landed, post-land"* ]] || { echo "$cage2"; return 1; }
  grep -q '^silas|#4888 model deployed to prod (athena-land)' "$SENT" || { cat "$SENT"; return 1; }
  ! grep -q 'landed, post-land' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC1: a passing athena, pair or domain-flow run wakes once with its result" {
  for dag in athena pair domain-flow; do
    cage; run finish succeeded "$dag"
    [ "$(wc -l < "$SENT" | tr -d ' ')" -eq 1 ] || { echo "$dag:"; cat "$SENT"; return 1; }
    grep -q "#4888 $dag passed" "$SENT" || { echo "$dag:"; cat "$SENT"; return 1; }
    # a pass goes out under run-finished-pass, the row that keeps it quiet (#4970)
    [ "$(cat "$SENT.alert")" = "run-finished-pass" ] || { echo "$dag:"; cat "$SENT.alert"; return 1; }
  done
  # and a failure names no row, so it always wakes
  cage; run finish failed athena
  [ "$(cat "$SENT.alert")" = "none" ] || { cat "$SENT.alert"; return 1; }
}

@test "cicd parks at the go: its pass is the demo brief, so run-finished sends nothing for it" {
  cage; run finish succeeded cicd
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"demo brief"* ]] || { echo "$output"; return 1; }
  [ ! -s "$SENT" ] || { cat "$SENT"; return 1; }
}

@test "AC4 negative proof: called from a step in the middle, run-finished sends nothing and says so" {
  cage; run finish succeeded cicd-land skill-werk-merge
  [ "$status" -ne 0 ] || return 1
  [ ! -s "$SENT" ] || { cat "$SENT"; return 1; }
  [[ "$output" == *"not an exit handler"* ]] || { echo "$output"; return 1; }
}

@test "AC2: run-finished sends through ops-nudge and nothing else" {
  out="$(grep -nE 'nudge-send|curl|clearing-nudge|/api/nudge|osascript' "$RF" || true)"
  [ -z "$out" ] || { echo "$out"; return 1; }
  [ "$(grep -c 'S/ops-nudge' "$RF")" -eq 1 ] || { grep -n ops-nudge "$RF"; return 1; }
}

@test "a passed go is silent (its land wakes); a failed go wakes" {
  cage; run finish succeeded cicd-go
  { [ "$status" -eq 0 ] && [ ! -s "$SENT" ]; } || { cat "$SENT"; return 1; }
  cage; run finish failed cicd-go
  grep -q '^silas|#4888 cicd-go failed .*failed at' "$SENT" || { cat "$SENT"; return 1; }
}

@test "AC6: pair red routes back to slice without its own nudge (the exit handler is the one wake)" {
  out="$(sed -n '/^red)/,/^  ;;/p' "$ROOT/platform/scripts/pair" | grep -nE '^\s*(nudge|.*ops-nudge) ' || true)"
  [ -z "$out" ] || { echo "pair red still nudges: $out"; return 1; }
}

@test "#4889: every finish wake is marked CHORUS_NUDGE_WAKE=run-finished (a field pulse never dedupes), pass or fail" {
  cage; run finish failed cicd
  [ "$(cat "$SENT.wake")" = "run-finished" ] || { cat "$SENT.wake"; return 1; }
  cage; run finish succeeded cicd-land
  [ "$(cat "$SENT.wake")" = "run-finished" ] || { cat "$SENT.wake"; return 1; }
}

@test "negative proof (#4965): a scheduled sensor is exempt, an unscheduled pipeline with no exit wake still fails" {
  P="$BATS_TEST_TMPDIR/pipes"; mkdir -p "$P"
  printf 'name: sensor\nschedule: "*/5 * * * *"\nhandler_on:\n  failure:\n    command: x\nsteps:\n' > "$P/sensor.yaml"
  printf 'name: quiet\nhandler_on:\n  failure:\n    command: x\nsteps:\n' > "$P/quiet.yaml"
  out="$(wake_problems "$P")"
  printf '%s\n' "$out" | grep -q '^quiet.yaml: 0 run-finished calls' || { echo "unscheduled pipeline passed: $out"; return 1; }
  printf '%s\n' "$out" | grep -q '^sensor.yaml' && { echo "scheduled sensor was checked: $out"; return 1; }
  return 0
}
