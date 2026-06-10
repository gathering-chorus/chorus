#!/bin/bash
# test-in-demo: exercise the DEMO SLOT's werk-deploy executable (#3317 crate mode)
# end-to-end against a sandboxed fixture — real binary, stubbed system surface.
set -u
# Resolves the binary under test: env override → role slot → PATH (installed).
BIN="${WERK_DEPLOY_BIN_UNDER_TEST:-${WERK_SILAS_BIN:+$WERK_SILAS_BIN/werk-deploy}}"
BIN="${BIN:-$(command -v werk-deploy)}"
T="$(mktemp -d -t wd-demo-XXXXXX)"
trap 'rm -rf "$T"' EXIT
FIX="$T/canonical"; STUB="$T/stub"; LOGS="$T/logs"
mkdir -p "$FIX" "$STUB" "$LOGS"
export CHORUS_BIN="$T/chorus-bin"; mkdir -p "$CHORUS_BIN"
export CHORUS_HOME="$FIX"
export CHORUS_DEPLOY_LIVENESS_TIMEOUT_S=2
export CHORUS_MCP_SMOKE_TIMEOUT_S=3
export CS_MARKER="$T/installed.marker"
export DEPLOY_ROLE=silas
unset CHORUS_TRACE_ID

# --- fixture canonical repo: rust service crate + plist + TS daemon dir ---
cd "$FIX"
git init -q -b main . >/dev/null
mkdir -p platform/services/chorus-inject/src config/launchagents platform/api/src platform/scripts
printf '[package]\nname="chorus-inject"\n' > platform/services/chorus-inject/Cargo.toml
printf '// v1\n' > platform/services/chorus-inject/src/lib.rs
printf '<plist><string>chorus-inject</string></plist>' > config/launchagents/com.chorus.inject.plist
printf '{"name":"chorus-api","scripts":{"build":"tsc"}}' > platform/api/package.json
git add . >/dev/null && git -c user.email=t@t -c user.name=t commit -qm "silas: #9999 fixture" >/dev/null
mkdir -p platform/services/chorus-inject/target/release
printf 'BINARY-V1' > platform/services/chorus-inject/target/release/chorus-inject

# --- stubs (PATH precedence) ---
cat > "$STUB/chorus-bin-install" <<EOF
#!/bin/sh
# honest stub: actually place the binary (real chorus-bin-install atomically mv's it).
echo "\$@" >> "$LOGS/install"
case "\$*" in *--rollback*) exit 0 ;; esac
src=""; name=""
for a; do src="\$name"; name="\$a"; done
[ -f "\$src" ] && cp "\$src" "\$CHORUS_BIN/\$name"
touch "\$CS_MARKER"
exit 0
EOF
cat > "$STUB/launchctl" <<EOF
#!/bin/sh
echo "\$@" >> "$LOGS/launchctl"
if [ "\$1" = print ]; then echo 'state = running'; echo 'pid = 123'; fi
exit 0
EOF
cat > "$STUB/codesign" <<EOF
#!/bin/sh
# honest stub: real codesign FAILS on a missing file — so must we.
for last; do :; done
[ -e "\$last" ] || { echo "\$last: No such file" >&2; exit 1; }
case "\$*" in
  *target/release*) echo "CDHash=DEADBEEF" ;;
  *) if [ -f "\$CS_MARKER" ]; then echo "CDHash=DEADBEEF"; else echo "CDHash=OLD000"; fi ;;
esac
EOF
cat > "$STUB/npm" <<EOF
#!/bin/sh
echo "npm \$@ pwd=\$(pwd)" >> "$LOGS/npm"
mkdir -p dist && printf 'built-js' > dist/server.js
exit 0
EOF
cat > "$STUB/curl" <<EOF
#!/bin/sh
echo "\$@" >> "$LOGS/curl"
echo '{"status":"healthy"}'
EOF
chmod +x "$STUB"/*
export PATH="$STUB:$PATH"
export CHORUS_BIN_INSTALL="$STUB/chorus-bin-install"

PASS=0; FAIL=0
ok()   { echo "  ok   $1"; PASS=$((PASS+1)); }
fail() { echo "  FAIL $1${2:+ — $2}"; FAIL=$((FAIL+1)); }

echo "T1: crate mode deploys a Rust service natively (install→kickstart→liveness→verify)"
rm -f "$CS_MARKER"; : > "$LOGS/install" 2>/dev/null; : > "$LOGS/launchctl" 2>/dev/null
out=$("$BIN" crate chorus-inject 2>&1); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -q "chorus-inject deployed target=canonical" \
  && ok "exit 0 + deployed summary" || fail "deploy" "rc=$rc out=$out"
grep -q -- "--target canonical" "$LOGS/install" && grep -q "chorus-inject" "$LOGS/install" \
  && ok "installed via chorus-bin-install --target canonical" || fail "install log" "$(cat "$LOGS/install" 2>/dev/null)"
grep -q "kickstart" "$LOGS/launchctl" && grep -q "com.chorus.inject" "$LOGS/launchctl" \
  && ok "kickstarted com.chorus.inject" || fail "kickstart log"
grep -q "print" "$LOGS/launchctl" \
  && ok "liveness-verified after kickstart (#3232 port)" || fail "liveness log"
grep -q '"event":"deploy.completed"' "$FIX/ops/logs/werk-deploy.jsonl" \
  && ok "deploy.completed witnessed" || fail "jsonl witness"

echo "T2: crate mode --rollback restores via chorus-bin-install --rollback + kickstart"
: > "$LOGS/install"; : > "$LOGS/launchctl"
out=$("$BIN" crate chorus-inject --rollback 2>&1); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -q "rolled back" \
  && ok "exit 0 + rolled back" || fail "rollback" "rc=$rc out=$out"
grep -q -- "--rollback" "$LOGS/install" \
  && ok "chorus-bin-install --rollback invoked" || fail "rollback install log" "$(cat "$LOGS/install")"
grep -q "kickstart" "$LOGS/launchctl" \
  && ok "re-kickstarted after rollback" || fail "rollback kickstart"

echo "T3: missing built binary → refuse loudly (no silent stale ship)"
out=$("$BIN" crate werk-pull 2>&1); rc=$?
[ $rc -ne 0 ] && echo "$out" | grep -q "binary missing" \
  && ok "refused with binary-missing (run werk-build first)" || fail "refusal" "rc=$rc out=$out"

echo "T4: TS daemon (chorus-api) — npm-build in canonical + kickstart + health smoke"
: > "$LOGS/launchctl"
out=$("$BIN" crate chorus-api 2>&1); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -q "chorus-api deployed" \
  && ok "exit 0 + deployed" || fail "ts deploy" "rc=$rc out=$out"
grep -q "run build" "$LOGS/npm" && grep -q "platform/api" "$LOGS/npm" \
  && ok "npm run build ran IN canonical platform/api" || fail "npm log" "$(cat "$LOGS/npm" 2>/dev/null)"
grep -q "com.chorus.api" "$LOGS/launchctl" \
  && ok "kickstarted com.chorus.api" || fail "api kickstart"
grep -q "health" "$LOGS/curl" \
  && ok "health-smoked before deploy.completed" || fail "smoke log" "$(cat "$LOGS/curl" 2>/dev/null)"

echo "T5: TS daemon rollback refuses without dist.prev (nothing to restore)"
rm -rf "$FIX/platform/api/dist.prev"
out=$("$BIN" crate chorus-api --rollback 2>&1); rc=$?
[ $rc -ne 0 ] && echo "$out" | grep -q "no dist.prev" \
  && ok "refused: no prior deploy to restore" || fail "ts rollback refusal" "rc=$rc out=$out"

echo ""
echo "Results: $PASS passed, $FAIL failed (executable: $BIN)"
[ $FAIL -eq 0 ]
