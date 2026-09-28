#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# @domain: code — the product domain this suite guards (#4334)
load test_helper
# regression-locks.bats
#
# Invariant tests for recurring regressions. Each test asserts a structural
# property that has been "fixed" before and reintroduced. A regression lock
# lives here so the next reintroduction fails at commit, not at user surface.
#
# Current locks:
#   1. Werk version is a version, not a session counter.
#      Generating CLAUDE.md without CLAUDEMD_BUMP=1 must NOT change
#      manifest.json version. (Jeff asked 5× in 3 months.)
#
#   2. All osascript calls go through chorus-inject (#2077).
#      chorus-hooks must not invoke osascript directly.
#      (#4336: locks 2, 2b and 3b RUN the binaries/code with a recording
#      osascript/ssh on PATH instead of grepping their source.)
#
#   3. Docker is retired (#2020, #2119).
#      Live code paths (excluding ADRs, journal, guardrails, knowledge docs)
#      must not contain the literal token `docker`.

CHORUS_ROOT="${CHORUS_ROOT:-${CHORUS_ROOT}}"

# #4336 — a PATH whose osascript / ssh / docker are recorders. A lock that runs
# code under this PATH can see whether the code shelled out, without Terminal,
# a relay, or Docker ever being touched.
recorder_path() {
  R="$BATS_TEST_TMPDIR/rec"
  mkdir -p "$R/bin" "$R/home"
  for tool in osascript ssh docker; do
    printf '#!/bin/bash\nprintf "%%s\\n" "$*" >> "%s/%s.calls"\necho ok\n' "$R" "$tool" > "$R/bin/$tool"
  done
  chmod +x "$R/bin/"*
}
calls() { [ -f "$R/$1.calls" ] && wc -l < "$R/$1.calls" | tr -d ' ' || echo 0; }

# ---------------------------------------------------------------------------
# Lock 1: werk-version does not bump on plain `generate`
# ---------------------------------------------------------------------------

@test "lock: claudemd-gen generate does NOT bump manifest version without CLAUDEMD_BUMP=1" {
  # #3710, two fixes:
  #  KEY  — the counter is `_build`, not `version`; the manifest's own
  #         _versioning_rule names _build as the monotonic integer that bumps on
  #         protocol changes. Reading ['version'] threw KeyError, so this lock
  #         had stopped guarding anything.
  #  SCOPE — it used to run the generator against the REAL manifest and restore
  #         it afterwards. A test that mutates production state and hopes to put
  #         it back is an incident waiting for a mid-run failure; it now works on
  #         its own copy, so canonical is never written at all.
  local work="${BATS_TEST_TMPDIR:-/tmp}/claudemd-lock-$$"
  mkdir -p "$work"
  cp -R "$CHORUS_ROOT/designing/claudemd/." "$work/"

  read_build() { python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['_build'])" "$1"; }
  before=$(read_build "$work/manifest.json")

  python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
    "$work/manifest.json" "$work" generate "" "" >/dev/null 2>&1 || true

  after=$(read_build "$work/manifest.json")
  rm -rf "$work"

  [ "$before" = "$after" ]
}

# ---------------------------------------------------------------------------
# Lock 2: no direct osascript in chorus-hooks (must route via chorus-inject)
# ---------------------------------------------------------------------------

# #4336 — was a grep of nudge.rs/process.rs for Command::new("osascript").
# Now the built shim is run on every delivery verb it ever had (inject #2435,
# nudge #2804) with a recording osascript first on PATH: each must be refused
# loudly (non-zero, names the MCP tool) and osascript must never be spawned.
@test "lock: chorus-hook-shim delivery verbs never spawn osascript — delivery routes via chorus-inject" {
  shim="$CHORUS_ROOT/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  [ -x "$shim" ] || skip "UNMEASURED — chorus-hook-shim not built in this checkout (#4336)"
  recorder_path
  for verb in inject nudge; do
    run env -i PATH="$R/bin:/usr/bin:/bin" HOME="$R/home" CHORUS_CONTEXT=test \
      CHORUS_LOG_FILE="$R/spine.log" "$shim" "$verb" wren "hello-4336" < /dev/null
    echo "$verb: status=$status $output"
    [ "$status" -ne 0 ] || return 1
    [[ "$output" == *"chorus_nudge_message"* ]] || return 1
  done
  [ "$(calls osascript)" -eq 0 ]
}

# ---------------------------------------------------------------------------
# Lock 3: docker absent from live code paths (#2020 retirement, #2119 purge)
# ---------------------------------------------------------------------------

# #4336 — was a grep of chorus-inject/src for the seam name and of both crates
# for RUN_LIVE_INJECT / HERMETIC_TEST_MODE. Now the built chorus-inject runs:
#  - with CHORUS_INJECT_DRY_RUN it walks the real path, prints its DRY-RUN
#    line, and never spawns osascript (the seam exists and holds);
#  - the two 2026-04-17 skip-polarity variables change NOTHING: the output is
#    byte-identical with them set, so no skip-gate reads them.
# (The non-dry path is not driven here: it is a live delivery primitive.)
@test "lock: chorus-inject honours the CHORUS_INJECT_DRY_RUN seam and has no skip-polarity gate" {
  inj="$CHORUS_ROOT/platform/services/chorus-inject/target/release/chorus-inject"
  [ -x "$inj" ] || skip "UNMEASURED — chorus-inject not built in this checkout (#4336)"
  recorder_path
  dry() {
    env -i PATH="$R/bin:/usr/bin:/bin" HOME="$R/home" _NUDGE_PULSE_INTERNAL=1 \
      CHORUS_INJECT_DRY_RUN=1 "$@" "$inj" --tty /dev/ttys999 "hello-4336" 2>&1
  }
  base="$(dry)"
  echo "base: $base"
  [[ "$base" == "DRY-RUN inject-by-tty tty=/dev/ttys999 "* ]] || return 1
  [ "$(dry HERMETIC_TEST_MODE=1)" = "$base" ] || return 1
  [ "$(dry RUN_LIVE_INJECT=0)" = "$base" ] || return 1
  [ "$(dry RUN_LIVE_INJECT=1)" = "$base" ] || return 1
  [ "$(calls osascript)" -eq 0 ]
}

@test "lock: no 'docker' in live code paths (ADRs, journal, knowledge docs exempted)" {
  cd "$CHORUS_ROOT"
  # Scan live code paths only: platform/scripts, platform/services, platform/api.
  # Exempt: knowledge docs, ADRs, journal entries, generated TTL, node_modules.
  #
  # #3710 — this lock matched the literal token anywhere and had gone red on
  # three hits, only one of which was docker doing anything:
  #   - doc-catalog.ts     the word inside a regex that CATEGORISES docs as
  #                        "Infrastructure". Classifying the word is not using it.
  #   - *.test.ts fixtures fixture data ABOUT the docker guard (DOCKER_BLOCKED,
  #                        cmd: 'docker rm -f x'). A test proving the guard fires
  #                        must be allowed to name what it blocks.
  #   - index-all-sources-deps.ts  a REAL `docker compose exec` — the Buzz
  #                        postgres read on Bedroom, which #3674 deliberately
  #                        runs on Bedroom's existing Docker Desktop.
  # A lock that is permanently red guards nothing, so the sanctioned exception is
  # named here explicitly. If the Buzz transport changes, delete the line and the
  # lock tightens again — that is the point of listing it rather than widening
  # the pattern.
  # #3904 — comment-stripped: harvest scripts legitimately MENTION docker in
  # prose about observed launchd units. A lock that cannot tell a comment from
  # an invocation cannot tell the two states it exists to separate (the 3785
  # guard's own lesson). Strip #- and //-comments, then match.
  hits=$(for f in $(grep -rnil --include='*.sh' --include='*.rs' --include='*.ts' --include='*.py' \
      '\bdocker\b' platform/scripts platform/services platform/api 2>/dev/null); do
      sed -e 's|//.*||' -e 's|#.*||' "$f" | grep -qi '\bdocker\b' && echo "$f"
    done | \
    grep -v 'node_modules' | \
    grep -v '\.bak$' | \
    grep -v 'regression-locks.bats' | \
    grep -v '\.test\.ts$' | \
    grep -v 'platform/api/src/handlers/doc-catalog\.ts$' | \
    grep -v 'platform/api/src/index-all-sources-deps\.ts$' || true)
  if [ -n "$hits" ]; then
    echo "'docker' reintroduced in live code paths:" >&2
    echo "$hits" >&2
    false
  fi
}

# #4336 — was a count of `docker compose` lines in index-all-sources-deps.ts.
# Now the REAL fetchBuzz runs (tsx, Fuseki stubbed in-process, ssh/docker as
# recorders): one reindex pull must make exactly ONE ssh call carrying exactly
# ONE `docker compose` invocation, and never run docker locally. With
# BUZZ_RELAY_HOST unset the transport must not exist at all.
buzz_probe() {
  local mods="$CHORUS_ROOT/platform/api/node_modules"
  [ -x "$mods/.bin/tsx" ] || mods="${CHORUS_HOME:-}/platform/api/node_modules"
  [ -x "$mods/.bin/tsx" ] || return 99
  env PATH="$R/bin:$PATH" HOME="$R/home" NODE_PATH="$mods" \
    ATHENA_SPARQL=http://127.0.0.1:9/sparql ATHENA_UPDATE=http://127.0.0.1:9/update "$@" \
    "$mods/.bin/tsx" "$CHORUS_ROOT/platform/tests/fixtures/4336/buzz-transport-probe.ts" \
    "$CHORUS_ROOT/platform/api/src/index-all-sources-deps.ts" 2>&1
}

@test "lock: the Buzz docker exception stays a single, named transport (#3674)" {
  recorder_path
  run buzz_probe BUZZ_RELAY_HOST=relay.invalid BUZZ_COMPOSE_DIR=/compose
  [ "$status" -ne 99 ] || skip "UNMEASURED — platform/api node_modules (tsx) not installed (#4336)"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "rows=0" ]
  [ "$(calls ssh)" -eq 1 ]
  [ "$(grep -o 'docker compose' "$R/ssh.calls" | wc -l | tr -d ' ')" -eq 1 ]
  [ "$(calls docker)" -eq 0 ]
}

@test "lock: with no relay configured the Buzz docker transport does not run (#3674)" {
  recorder_path
  run buzz_probe BUZZ_RELAY_HOST=
  [ "$status" -ne 99 ] || skip "UNMEASURED — platform/api node_modules (tsx) not installed (#4336)"
  echo "$output"
  [ "$output" = "NO-FETCHBUZZ" ]
  [ "$(calls ssh)" -eq 0 ]
  [ "$(calls docker)" -eq 0 ]
}

@test "#3904 NEGATIVE PROOF: the comment-stripped docker lock still catches a REAL invocation" {
  # Same discipline as recovery-path-ungated-3785: prove the strip did not
  # defang the lock — a file whose comment mentions docker but whose CODE
  # invokes it must be seen.
  FIXTURE="$BATS_TEST_TMPDIR/uses-docker.sh"
  printf '#!/usr/bin/env bash\n# docker is mentioned here in prose\ndocker compose up -d\n' > "$FIXTURE"
  run bash -c "sed -e 's|//.*||' -e 's|#.*||' '$FIXTURE' | grep -qi '\\bdocker\\b'"
  [ "$status" -eq 0 ]
  FIXTURE2="$BATS_TEST_TMPDIR/mentions-docker.sh"
  printf '#!/usr/bin/env bash\n# docker mentioned only in prose\necho hello\n' > "$FIXTURE2"
  run bash -c "sed -e 's|//.*||' -e 's|#.*||' '$FIXTURE2' | grep -qi '\\bdocker\\b'"
  [ "$status" -ne 0 ]
}
