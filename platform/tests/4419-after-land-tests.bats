#!/usr/bin/env bats
# @test-type: unit — stubs for launchctl, werk-test and the nudge; no live service
# @domain: tests
# @card: 4419 · owner: kade
# What Jeff sees: a land runs every test in the domains it touched, in the
# background, and a red reaches the card's owner in minutes, not at 03:00.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

setup() {
  T="$(mktemp -d)"
  export AFTER_LAND_QUEUE="$T/after-land/queue"
  export AFTER_LAND_STATE_CMD="cat $T/state"
  export AFTER_LAND_KICKSTART_CMD="touch $T/kicked"
  : > "$T/state"
  export NIGHTLY_LOCKDIR="$T/nightly.lock.d" AFTER_LAND_WAIT_TICK=1
  R="$T/repo"; git init -q "$R"
  echo one > "$R/f"; git -C "$R" add f; git -C "$R" -c user.email=t@t -c user.name=t commit -qm one
  C1="$(git -C "$R" rev-parse HEAD)"
  echo two > "$R/f"; git -C "$R" -c user.email=t@t -c user.name=t commit -qam two
  C2="$(git -C "$R" rev-parse HEAD)"
}
teardown() { rm -rf "$T"; }

@test "a land queues its card, role and commit, and kickstarts the runner" {
  run env CARD_ID=4353 ROLE=wren LANDED_COMMIT=6a1b7841b bash "$REPO/platform/scripts/after-land-detached.sh"
  [ "$status" -eq 0 ] || return 1
  [ -f "$T/kicked" ] || return 1
  f="$(ls "$AFTER_LAND_QUEUE"/*-4353.env)"
  grep -qx 'CARD=4353' "$f" || return 1
  grep -qx 'ROLE=wren' "$f" || return 1
  grep -qx 'COMMIT=6a1b7841b' "$f" || return 1
}

@test "a land while the runner is going queues and does not restart it" {
  printf '\tstate = running\n\tpid = 42\n' > "$T/state"
  run env CARD_ID=4417 ROLE=wren LANDED_COMMIT=470751b82 bash "$REPO/platform/scripts/after-land-detached.sh"
  [ "$status" -eq 0 ] || return 1
  [ ! -f "$T/kicked" ] || return 1
  ls "$AFTER_LAND_QUEUE"/*-4417.env >/dev/null || return 1
}

stub_werk_test() { # exit code
  cat > "$T/werk-test" <<STUB
#!/bin/bash
echo "domain-select: 3 changed file(s) touch domain(s) [domains] → 2 registered test file(s)"
echo "args=\$* replay=\$WERK_TEST_REPLAY after=\$WERK_TEST_AFTER_LAND file=\$(cat \$WERK_TEST_TREE/f)" >> "$T/werk-test.args"
[ "$1" -eq 0 ] || echo "   jest:platform/api … FAIL"
exit $1
STUB
  chmod +x "$T/werk-test"
  printf '#!/bin/bash\necho "$*" >> %s/nudges\n' "$T" > "$T/nudge"; chmod +x "$T/nudge"
}

@test "a red after-land run nudges the card's owner and names the failing suite" {
  stub_werk_test 1
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=4353\nROLE=wren\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/1-4353.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [ "$status" -eq 0 ] || return 1
  # the LANDED commit is what ran (file=one), not canonical's later HEAD (two)
  grep -q "args=4353 wren replay=$C1 after=1 file=one" "$T/werk-test.args" || return 1
  grep -q '^wren after-land #4353: red' "$T/nudges" || return 1
  grep -q 'jest:platform/api … FAIL' "$T/nudges" || return 1
  ls "$T/after-land/done/1-4353-rc1.env" >/dev/null || return 1
  [ -z "$(ls "$AFTER_LAND_QUEUE")" ] || return 1
}

@test "NEGATIVE PROOF — a green after-land run nudges nobody" {
  stub_werk_test 0
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=4417\nROLE=wren\nCOMMIT=%s\n' "$C2" > "$AFTER_LAND_QUEUE/1-4417.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [ "$status" -eq 0 ] || return 1
  [ ! -f "$T/nudges" ] || return 1
  [[ "$output" == *"#4417 green"* ]] || return 1
  ls "$T/after-land/done/1-4417-rc0.env" >/dev/null || return 1
}

@test "two queued lands run oldest first" {
  stub_werk_test 0
  mkdir -p "$AFTER_LAND_QUEUE"
  printf 'CARD=2\nROLE=kade\nCOMMIT=%s\n' "$C2" > "$AFTER_LAND_QUEUE/200-2.env"
  printf 'CARD=1\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-1.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  first="$(grep -n '#1 (kade)' <<<"$output" | head -1 | cut -d: -f1)"
  second="$(grep -n '#2 (kade)' <<<"$output" | head -1 | cut -d: -f1)"
  [ "$first" -lt "$second" ] || return 1
}

@test "the land runs the step, after the crawl, never gating the land" {
  f="$REPO/.github/workflows/werk.yml"
  awk '/name: after-land-tests/{a=NR} /name: crawl-delta/{c=NR} END{exit !(a>c && c>0)}' "$f" || return 1
  grep -A4 'name: after-land-tests' "$f" | grep -q 'continue-on-error: true' || return 1
}

@test "an entry queued while a run is going is picked up by the same run" {
  stub_werk_test 0
  # the first run's werk-test queues a second land, the way a land mid-run does
  cat >> "$T/werk-test" <<STUB2
STUB2
  sed -i '' "2i\\
[ -f $T/queued ] || { touch $T/queued; printf 'CARD=9\\\\nROLE=kade\\\\nCOMMIT=$C2\\\\n' > $AFTER_LAND_QUEUE/300-9.env; }
" "$T/werk-test"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=8\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-8.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [[ "$output" == *"#8 green"* ]] || return 1
  [[ "$output" == *"#9 green"* ]] || return 1
}

@test "it waits while the nightly holds its lock, and never takes it" {
  stub_werk_test 0
  mkdir -p "$NIGHTLY_LOCKDIR"; sleep 3 & echo $! > "$NIGHTLY_LOCKDIR/pid"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=7\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-7.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [[ "$output" == *"nightly holds its lock"* ]] || return 1
  [[ "$output" == *"#7 green"* ]] || return 1
  # the lock is still the nightly's: same dir, same pid file, not ours
  [ -d "$NIGHTLY_LOCKDIR" ] || return 1
}

stub_lines() { # exit code, extra line
  cat > "$T/werk-test" <<STUB
#!/bin/bash
echo "$2"
exit $1
STUB
  chmod +x "$T/werk-test"
  printf '#!/bin/bash\necho "$*" >> %s/nudges\n' "$T" > "$T/nudge"; chmod +x "$T/nudge"
}

@test "NEGATIVE PROOF — exit 0 with a FAIL line (advisory) is red, never green" {
  stub_lines 0 "   cargo-test:athena-make … FAIL"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=5\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-5.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [[ "$output" != *"#5 green"* ]] || return 1
  [[ "$output" == *"#5 RED"* ]] || return 1
  ls "$T/after-land/done/100-5-rc1.env" >/dev/null || return 1
}

@test "NEGATIVE PROOF — a unit with no toolchain is UNMEASURED, never green" {
  stub_lines 0 "!! jest:platform/api CHANGED but deps unavailable — FAIL LOUD"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=6\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-6.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [[ "$output" != *"#6 green"* ]] || return 1
  [[ "$output" == *"#6 UNMEASURED"* ]] || return 1
  grep -q '^kade after-land #6: UNMEASURED' "$T/nudges" || return 1
}

@test "the agent runs with the nightly's toolchain on PATH" {
  p="$REPO/platform/launchd/com.chorus.after-land-tests.plist"
  grep -q '.cargo/bin' "$p" || return 1
  grep -q '.nvm/versions/node/' "$p" || return 1
  # crates build into the persistent tree's own target/, where suites look
  if grep -q '<key>CARGO_TARGET_DIR</key>' "$p"; then return 1; fi
}

@test "a run killed mid-way leaves no stale worktree that blocks the next one" {
  stub_werk_test 0
  # the killed run: a registered worktree whose directory is gone
  git -C "$R" worktree add --detach "$T/after-land/tree/chorus" "$C1" >/dev/null 2>&1
  rm -rf "$T/after-land/tree"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=3\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-3.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [[ "$output" == *"#3 green"* ]] || return 1
  [[ "$output" != *"could not check out"* ]] || return 1
}

@test "a suite that writes beside the repo writes into the run's own box, which is removed" {
  cat > "$T/werk-test" <<STUB
#!/bin/bash
touch "\$WERK_TEST_TREE/../TEAM_PROTOCOL.md"
exit 0
STUB
  chmod +x "$T/werk-test"
  printf '#!/bin/bash\n' > "$T/nudge"; chmod +x "$T/nudge"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=2\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-2.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  # it landed in the run's box (tree/), never beside the queue or the repo
  [ ! -e "$T/after-land/TEAM_PROTOCOL.md" ] || return 1
  [ -e "$T/after-land/tree/TEAM_PROTOCOL.md" ] || return 1
  # and the next run clears it before it starts
  printf 'CARD=3\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/200-3.env"
  printf '#!/bin/bash\nexit 0\n' > "$T/werk-test"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  [ ! -e "$T/after-land/tree/TEAM_PROTOCOL.md" ] || return 1
}

@test "one persistent checkout moves to each landed commit and keeps its warm state" {
  stub_werk_test 0
  mkdir -p "$AFTER_LAND_QUEUE"
  printf 'CARD=1\nROLE=kade\nCOMMIT=%s\n' "$C1" > "$AFTER_LAND_QUEUE/100-1.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  mkdir -p "$T/after-land/tree/chorus/target" && touch "$T/after-land/tree/chorus/target/built"
  printf 'CARD=2\nROLE=kade\nCOMMIT=%s\n' "$C2" > "$AFTER_LAND_QUEUE/200-2.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  grep -q "args=1 kade replay=$C1 after=1 file=one" "$T/werk-test.args" || return 1
  grep -q "args=2 kade replay=$C2 after=1 file=two" "$T/werk-test.args" || return 1
  [ -e "$T/after-land/tree/chorus/target/built" ] || return 1
}

@test "the checkout gets canonical's installed deps, so eslint and jest can load" {
  mkdir -p "$R/pkg/node_modules/dep" && echo '{}' > "$R/pkg/package.json" && git -C "$R" add pkg/package.json && git -C "$R" -c user.email=t@t -c user.name=t commit -qm pkg
  C3="$(git -C "$R" rev-parse HEAD)"
  cat > "$T/werk-test" <<STUB
#!/bin/bash
[ -d "\$WERK_TEST_TREE/pkg/node_modules/dep" ] && echo "deps-present" > "$T/deps"
exit 0
STUB
  chmod +x "$T/werk-test"; printf '#!/bin/bash\n' > "$T/nudge"; chmod +x "$T/nudge"
  mkdir -p "$AFTER_LAND_QUEUE"; printf 'CARD=1\nROLE=kade\nCOMMIT=%s\n' "$C3" > "$AFTER_LAND_QUEUE/100-1.env"
  run env AFTER_LAND_WERK_TEST="$T/werk-test" AFTER_LAND_NUDGE="$T/nudge" CHORUS_ROOT="$R" bash "$REPO/platform/scripts/after-land-tests.sh"
  grep -q deps-present "$T/deps" || return 1
  # the link is removed with the box; canonical's own deps are untouched
  [ -d "$R/pkg/node_modules/dep" ] || return 1
}
