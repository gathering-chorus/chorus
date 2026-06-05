#!/bin/bash
# #3237 seam-proof — the live cross-binary handshake (Wren+Kade pair, 2026-06-05).
# Proves the REAL new thing: werk-accept/werk-do-more write a byte-exact demo.decision
# that werk-demo's blocking poll reads, and the exit codes are right — cross-process.
#
# Isolated (Kade's safety call): temp CHORUS_HOME, fake cards, NO pipeline merge, NO
# deploy to ~/.chorus/bin. The team-live swap is the LAND (atomic: producers+demo+werk.yml
# together), a separate deliberate op.
#
# Run from anywhere:  bash PAIR-3237-seam-proof.sh
# Expect:             GO_EXIT=0   MORE_EXIT=2
set -u
SVC="$(cd "$(dirname "$0")" && pwd)/platform/services"
WA="$SVC/werk-accept/target/release"; WD="$SVC/werk-demo/target/release"

# release build (plain — werk-* are plain CLI; signing is only for TCC binaries)
( cd "$SVC/werk-accept" && cargo build --release --bins -q ) || exit 1
( cd "$SVC/werk-demo"   && cargo build --release -q )        || exit 1

T=$(mktemp -d)
mkdir -p "$T/platform/scripts" "$T/ops/logs" "$T/bin" "$T/werk"
# cards shim: view→WIP/kade (json) or 2/2 AC (human); everything else no-ops
cat > "$T/platform/scripts/cards" <<'SH'
#!/bin/sh
J=0; for a in "$@"; do [ "$a" = "--json" ] && J=1; done
if [ "$1" = "view" ]; then
  if [ "$J" = "1" ]; then echo '{"status":"WIP","owner":"kade"}'
  else printf 'Demo card\nAC\n- [x] one\n- [x] two\n'; fi
fi
exit 0
SH
for s in chorus-log chorus-werk; do printf '#!/bin/sh\nexit 0\n' > "$T/platform/scripts/$s"; done
for s in curl gh werk-deploy;  do printf '#!/bin/sh\nexit 0\n' > "$T/bin/$s"; done
chmod +x "$T/platform/scripts/"* "$T/bin/"*
export PATH="$T/bin:$PATH" CHORUS_HOME="$T" CHORUS_WERK_BASE="$T/werk" CHORUS_BIN="$T/bin"
export CHORUS_DEMO_GATE_WAIT_SECS=0 CHORUS_DEMO_ACK_WINDOW_SECS=0 CHORUS_DEMO_COMMENT_WINDOW_SECS=0
export CHORUS_DEMO_POLL_SECS=1 CHORUS_DEMO_MAX_BLOCK_SECS=60
W="$T/ops/logs/werk-demo.jsonl"
seed(){ for g in product code quality arch ops; do
  echo "{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"kade\",\"card_id\":$1,\"trace_id\":\"seed\",\"gate\":\"$g\",\"result\":\"pass\"}" >> "$W"; done; }
# wait until werk-demo reaches its block (demo.awaiting_decision in the witness) — robust, no fixed sleep
await_block(){ for _ in $(seq 1 30); do grep -q "\"event\":\"demo.awaiting_decision\".*\"card_id\":$1," "$W" 2>/dev/null && return 0; sleep 0.5; done; return 1; }

run(){ # $1=card $2=writer-bin $3=decision-args...  echoes the demo exit code
  local card=$1; shift; local bin=$1; shift
  seed "$card"
  DEPLOY_ROLE=kade "$WD/werk-demo" "$card" >/dev/null 2>&1 & local pid=$!
  await_block "$card" || { echo "demo never blocked for $card"; kill "$pid" 2>/dev/null; echo 99; return; }
  DEPLOY_ROLE=jeff "$WA/$bin" "$card" kade "$@" >/dev/null 2>&1
  wait "$pid"; echo $?
}

GO=$(run 999001 werk-accept)
MORE=$(run 999002 werk-do-more more)
echo "==== #3237 SEAM-PROOF ===="
echo "GO_EXIT=$GO   (expect 0 — go → werk-demo exits 0 → act continues to merge)"
echo "MORE_EXIT=$MORE (expect 2 — more → werk-demo exits 2 → act stops, nothing merged)"
[ "$GO" = "0" ] && [ "$MORE" = "2" ] && echo "RESULT: PASS — seam proven" || echo "RESULT: FAIL"
rm -rf "$T"
