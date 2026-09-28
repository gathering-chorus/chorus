#!/usr/bin/env bash
# @test-type: unit — stubs ssh/scp/tdb2; no live services
# @domain: backups
#
# #4399 — the nightly restore test reds the first night a backup is missed.
# On 2026-09-28 the drill job had pointed at a deleted script since #4141 and
# the last proven restore was 09-15; nobody heard. The drill restores the NEWEST
# dump and fails when that dump is older than 26 hours.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin"
now_name() { date -v-"$1"H '+pods_%Y-%m-%d_%H-%M-%S.nq.gz'; }
mk() {  # $1 = dump name the stub ssh lists
  cat > "$T/bin/ssh" <<S
#!/bin/bash
case "\$*" in *"ls -1"*) echo "/d/dumps/$1" ;; *) exit 0 ;; esac
S
  printf '#!/bin/bash\nexit 1\n' > "$T/bin/scp"   # never reached on a stale dump
  chmod +x "$T/bin/ssh" "$T/bin/scp"
}
run() { PATH="$T/bin:$PATH" FUSEKI_BACKUP_DEST=/d FUSEKI_DRILL_NEWEST=1 FUSEKI_DRILL_SCRATCH="$T/s" CHORUS_LOG=/usr/bin/true bash "$ROOT/platform/scripts/fuseki-restore-dump.sh" 2>&1; }
pass=0; fail=0
mk "$(now_name 50)"; out=$(run); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q "a night was missed"; then echo "PASS a 50h-old newest dump is red: a night was missed"; pass=$((pass+1)); else echo "FAIL stale dump not red (rc=$rc): $out"; fail=$((fail+1)); fi
# NEGATIVE PROOF: a dump from last night passes the age check (it then fails
# later at scp, which the stub refuses; the point is it is NOT the missed-night red)
mk "$(now_name 10)"; out=$(run)
if ! printf '%s' "$out" | grep -q "a night was missed"; then echo "PASS NEGATIVE: a 10h-old dump is not called a missed night"; pass=$((pass+1)); else echo "FAIL a fresh dump was called missed: $out"; fail=$((fail+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
