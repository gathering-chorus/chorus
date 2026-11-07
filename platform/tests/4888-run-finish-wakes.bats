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
# owns its finish). The nightly keeps its own exit (werk-test notify).
toplevel() {
  for f in "$1"/*.yaml; do
    grep -q '^  - TRACE: ' "$f" && continue
    case "$(basename "$f")" in nightly.yaml) continue ;; esac
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
WAKE_RE='^\[alert → [a-z-]+\] #[0-9]+ [a-z-]+ .*· dagu [^ ]+ · /cws [0-9]+$'
@test "AC5: the newest run-finished wake in the room's record renders on the public page" {
  rec="$(curl -sf --max-time 10 "${ROOM_URL}api/messages")" || { echo "room record unreadable"; return 1; }
  want="$(printf '%s' "$rec" | node -e '
    const {contentHash}=require(process.argv[1]); let s=""; process.stdin.on("data",d=>s+=d).on("end",()=>{
    const re=new RegExp(process.argv[2]); const w=JSON.parse(s).filter(m=>m.from==="system"&&re.test(m.text||"")).pop();
    if(w) console.log("system:"+contentHash(w.text)); });' "$ROOT/platform/chorus-sdk/reply/hash.js" "$WAKE_RE")"
  [ -n "$want" ] || skip "unmeasured: no run-finished wake in the room's record"
  run env WANT_4887="$want" node "$ROOT/platform/tests/lib/room-rows-4887.cjs" "$ROOT/node_modules/playwright-core" "$ROOM_URL"
  echo "$output"
  [ "$status" -ne 3 ] || skip "unmeasured: $output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC5: the wake pattern matches run-finished's own words, and not an older nudge" {
  re="$WAKE_RE"
  [[ "[alert → abby-normal] #999999 clearing-evaluate passed · dagu 034rvgJDfrob5mIxbyTwg3 · /cws 999999" =~ $re ]] || return 1
  [[ ! "[alert → silas] werk v2 #4887 cicd failed at test-pipeline: x · dagu y · /cws 4887 · retry" =~ $re ]] || return 1
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
  printf '#!/bin/sh\n[ "$1" = nudge-send ] && printf "%%s|%%s\\n" "$3" "$(printf %%s "$4" | tr "\\n" " ")" >> %s && echo "${CHORUS_NUDGE_WAKE:-none}" >> %s.wake\n' "$SENT" "$SENT" > "$BATS_TEST_TMPDIR/shim"
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
  done
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
