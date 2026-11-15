#!/usr/bin/env bats
# @test-type: unit — runs platform/scripts/land-4977-rows main() with stubs; no store, no Grafana
# @domain: monitors
# @card: #4977 · owner: silas
# Checks platform/scripts/land-4977-rows and platform/scripts/chorus-icd-load.sh.
#
# #4977 AC6 — a rule tells once because Grafana gets its repeat wait. convergence-land
# builds rules from the ICD maps in the store; #4970's repeatInterval map was only in the
# ttl, so the disk rule landed with no repeat_interval and re-told every 4h (2026-11-15
# 00:37). The land loads the ICD before any rule, and refuses a rule built without a wait.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  B="$BATS_TEST_TMPDIR"; LOG="$B/calls"; : > "$LOG"
  printf '#!/bin/sh\necho icd-load >> "%s"\nexit "${ICD_RC:-0}"\n' "$LOG" > "$B/icd"
  # the builder stub: prints a rule with a repeat wait unless BARE names this rule
  printf '#!/bin/sh\necho "built $2" >> "%s"\nif [ "$2" = "$BARE" ]; then echo "{\\"notification_settings\\": {\\"receiver\\": \\"r\\"}}"; else echo "{\\"notification_settings\\": {\\"receiver\\": \\"r\\", \\"repeat_interval\\": \\"8760h\\"}}"; fi\n' "$LOG" > "$B/conv"
  chmod +x "$B/icd" "$B/conv"
}

# main(--apply) with the door write and the rule land stubbed; the land of each rule is logged
apply() {
  ICD_LOAD="$B/icd" CONVERGENCE_LAND="$B/conv" LOG="$LOG" python3 - "$ROOT/platform/scripts/land-4977-rows" <<'EOF'
import importlib.machinery, importlib.util, os, sys
p = sys.argv[1]; sys.argv = [p, "--apply"]
loader = importlib.machinery.SourceFileLoader("l4977", p)
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("l4977", loader)); loader.exec_module(m)
m.land.main = lambda argv: 0
def landed(n):
    open(os.environ["LOG"], "a").write(f"landed {n}\n"); return True
m.l4970.land_rule = landed
sys.exit(m.main(["--apply"]))
EOF
}

@test "the land loads the ICD before it lands any rule" {
  run apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(head -1 "$LOG")" = "icd-load" ] || { cat "$LOG"; return 1; }
  grep -q '^landed bedroom-disk-full$' "$LOG" || return 1
  [ "$(grep -c '^landed ' "$LOG")" -eq 5 ] || return 1
}
@test "NEGATIVE PROOF: a refused ICD load lands no rule" {
  ICD_RC=1 run apply
  [ "$status" -eq 1 ] || return 1
  ! grep -q '^landed ' "$LOG" || return 1
}
@test "NEGATIVE PROOF: a rule built with no repeat_interval stops the land before any rule lands" {
  BARE=fuseki-memory-high run apply
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"no repeat_interval"*"fuseki-memory-high"* ]] || { echo "$output"; return 1; }
  ! grep -q '^landed ' "$LOG" || return 1
}
@test "the ICD ttl the load reads carries the repeatInterval map" {
  grep -q 'icd:sourceField "notification_settings.repeat_interval"' \
    "$ROOT/building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl" || return 1
  grep -q 'INSTANCES_DIR="$CHORUS_ROOT/building/products/convergence/domains/integrations/icd-instances"' \
    "$ROOT/platform/scripts/chorus-icd-load.sh" || return 1
}
