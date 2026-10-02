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
  [ -z "$(git -C "$R" worktree list | sed 1d)" ] || return 1
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
