#!/usr/bin/env bash
# test-launchagent-boot-audit.sh — #4027 boot-herd guard.
#
# launchagent-boot-audit.sh must FAIL (exit 1, name the label) when an
# interval agent (StartInterval, no KeepAlive) carries RunAtLoad=true, PASS
# on a long-running service (KeepAlive=true + RunAtLoad), and PASS on an
# interval agent without RunAtLoad. --fix must remove the key and leave the
# dir passing. The test brings its own plist dir — it never reads
# ~/Library/LaunchAgents (#3528).
set -uo pipefail
PASS=0; FAIL=0
trap 'echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUDIT="$SCRIPT_DIR/launchagent-boot-audit.sh"
T=$(mktemp -d); trap 'rm -rf "$T"; echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT

plist() { # plist <file> <label> <body-xml>
  cat > "$1" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$2</string>
  <key>ProgramArguments</key><array><string>/usr/bin/true</string></array>
  $3
</dict></plist>
XML
}
ok()  { PASS=$((PASS+1)); echo "PASS: $1"; }
bad() { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

# --- negative proof: interval + RunAtLoad → RED, label named ---
mkdir -p "$T/red"
plist "$T/red/com.test.herd.plist" com.test.herd '<key>StartInterval</key><integer>900</integer><key>RunAtLoad</key><true/>'
out=$("$AUDIT" "$T/red" 2>&1); rc=$?
if [ $rc -eq 1 ] && grep -q 'com.test.herd' <<<"$out"; then ok "violation → exit 1 and names com.test.herd"; else bad "violation not caught (rc=$rc): $out"; fi

# --- service with KeepAlive + RunAtLoad → GREEN ---
mkdir -p "$T/svc"
plist "$T/svc/com.test.svc.plist" com.test.svc '<key>KeepAlive</key><true/><key>RunAtLoad</key><true/>'
if "$AUDIT" "$T/svc" >/dev/null 2>&1; then ok "KeepAlive service keeps RunAtLoad"; else bad "service flagged"; fi

# --- interval without RunAtLoad → GREEN ---
mkdir -p "$T/ok"
plist "$T/ok/com.test.tick.plist" com.test.tick '<key>StartInterval</key><integer>60</integer>'
if "$AUDIT" "$T/ok" >/dev/null 2>&1; then ok "interval without RunAtLoad passes"; else bad "clean interval flagged"; fi

# --- StartCalendarInterval + RunAtLoad → RED too ---
mkdir -p "$T/cal"
plist "$T/cal/com.test.cal.plist" com.test.cal '<key>StartCalendarInterval</key><dict><key>Hour</key><integer>3</integer></dict><key>RunAtLoad</key><true/>'
"$AUDIT" "$T/cal" >/dev/null 2>&1; rc=$?
if [ $rc -eq 1 ]; then ok "calendar job with RunAtLoad → exit 1"; else bad "calendar job passed (rc=$rc)"; fi

# --- --fix removes the key and the dir goes green; plist still valid ---
"$AUDIT" --fix "$T/red" >/dev/null 2>&1
if "$AUDIT" "$T/red" >/dev/null 2>&1 && plutil -lint -s "$T/red/com.test.herd.plist" && [ "$(plutil -extract StartInterval raw "$T/red/com.test.herd.plist")" = "900" ]; then ok "--fix removes RunAtLoad, keeps StartInterval, plist lints"; else bad "--fix did not converge"; fi

# --- empty/missing dir must fail loudly, never pass vacuously ---
"$AUDIT" "$T/nope" >/dev/null 2>&1; rc=$?
if [ $rc -eq 2 ]; then ok "missing dir → exit 2 (no vacuous pass)"; else bad "missing dir rc=$rc"; fi

[ $FAIL -eq 0 ]
