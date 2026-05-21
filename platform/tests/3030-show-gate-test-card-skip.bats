#!/usr/bin/env bats
# #3030 — show-gate.sh must skip synthetic test card 99998 without emitting any
# demo.show.* event (it was ~70% of the phantom no_demo_started pain-board class).
# Real cards and the no-card case are covered by the existing guards.

setup() {
  TMP="$(mktemp -d)"
  mkdir -p "$TMP/platform/scripts"
  cat > "$TMP/platform/scripts/chorus-log" <<'EOF'
#!/usr/bin/env bash
echo "$@" >> "$CHORUS_LOG_SINK"
EOF
  chmod +x "$TMP/platform/scripts/chorus-log"
  export CHORUS_LOG_SINK="$TMP/emitted.log"
  export CHORUS_ROOT="$TMP"
  GATE="$BATS_TEST_DIRNAME/../../skills/demo/gates/show-gate.sh"
}

teardown() { rm -rf "$TMP"; }

@test "card 99998 exits 0 and emits nothing" {
  run bash "$GATE" 99998 wren
  [ "$status" -eq 0 ]
  [ ! -s "$CHORUS_LOG_SINK" ]   # sink empty: no demo.show.* emitted
}

@test "no-card invocation exits 0 and emits nothing" {
  run bash "$GATE" "" wren
  [ "$status" -eq 0 ]
  [ ! -s "$CHORUS_LOG_SINK" ]
}

@test "a real card id is NOT skipped — it proceeds to emit demo.show.started" {
  # Real card with no Loki backing will ultimately fail the gate, but it must at
  # least get past the skip and emit demo.show.started (proving 99998 is special-cased,
  # not a blanket skip). Loki query returns nothing here, so it exits 1 after emitting.
  run bash "$GATE" 1234 wren
  grep -q "demo.show.started" "$CHORUS_LOG_SINK"
}
