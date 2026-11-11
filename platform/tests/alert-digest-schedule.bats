#!/usr/bin/env bats
# @test-type: integration — drives platform/scripts/alert-case and alert-digest against a fake door and stub tmux/osascript/nudge binaries; never reaches a live door or pane.
# @domain: alerts
# @card: #4957 · owner: silas
#
# #4957 AC2 — each role reads its open cases as one digest; nothing is typed into a
# pane by an alert. Jeff 2026-11-11 09:50: "if the alerts come to a queue and you
# check alerts every five minutes via dagu… I could accept that."
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/bin" "$T/digests"
  echo '[]' > "$T/rows.json"
  # the fake door (shared shape with alert-one-row.bats): GET filters by caseOf,
  # caseSubject, ownedBy and caseClosedAt.absent; POST adds; PATCH merges by name
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
m=GET; body=""; url=""; auth=0
while [ \$# -gt 0 ]; do case "\$1" in -X) m="\$2"; shift ;; --data-binary) body=\$(cat "\${2#@}"); shift ;; -H) case "\$2" in @*) grep -q '^Authorization: Bearer .' "\${2#@}" && auth=1 ;; esac; shift ;; -o|-w|--max-time) shift ;; http*) url="\$1" ;; esac; shift; done
export FAKE_AUTH=\$auth
python3 - "$T/rows.json" "\$m" "\$url" "\$body" <<'PY'
import json,sys,os,urllib.parse as u
p,m,url,body=sys.argv[1:5]; rows=json.load(open(p))
# #4957 — like the real door: only the routes it serves answer, and a listing is
# pretty-printed over many lines (a first-line parse and a wrong path both went live)
path=u.urlparse(url).path
if not (path.startswith("/v1/alerts/cases") or path.startswith("/v1/roles/roles")):
    print(json.dumps({"error":"not-found","detail":"no such route: "+path}, indent=1)); print(404); sys.exit()
if m=="GET":
    q=dict(u.parse_qsl(u.urlparse(url).query))
    out=[r for r in rows if all(r.get(k)==v for k,v in q.items() if k in("caseOf","caseSubject","ownedBy"))
         and (q.get("caseClosedAt.absent")!="true" or not r.get("caseClosedAt"))]
    if "/v1/roles/roles" in url: out=[{"name":"silas","roleKind":"agent"},{"name":"kade","roleKind":"agent"},{"name":"jeff","roleKind":"human"}]
    print(json.dumps({"data":out}, indent=1)); print(200); sys.exit()
if m!="GET" and os.environ.get("FAKE_AUTH")!="1":
    print(json.dumps({"error":"authn-missing"}, indent=1)); print(401); sys.exit()
b=json.loads(body)
if m=="POST": rows.append(b)
else:
    name=u.urlparse(url).path.rsplit("/",1)[-1]
    for r in rows:
        if r["name"]==name: r.update(b)
json.dump(rows,open(p,"w")); print("{}"); print(200)
PY
EOS
  # anything that could reach a pane records that it was called
  for b in tmux osascript chorus-hook-shim ops-nudge nudge; do
    printf '#!/bin/bash\necho "%s $*" >> "%s/pane.log"\n' "$b" "$T" > "$T/bin/$b"
  done
  chmod +x "$T/bin/"*
  export PATH="$T/bin:$PATH" ALERT_CURL="$T/bin/curl" ALERT_DOOR="http://door.test" ALERT_DIGEST_DIR="$T/digests"
  # the alerts principal's token (a stub minter: asked for "alerts", it answers one)
  printf '#!/bin/bash\n[ "$1" = alerts ] && echo tok-alerts || { echo "no credential for $1" >&2; exit 3; }\n' > "$T/token"; chmod +x "$T/token"
  export ALERT_TOKEN_BIN="$T/token"
  export CHORUS_HOOK_SHIM="$T/bin/chorus-hook-shim"
  CASE="$ROOT/platform/scripts/alert-case"; DIGEST="$ROOT/platform/scripts/alert-digest"
}

@test "a role's digest lists its open cases, one line each, and no other role's" {
  "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  "$CASE" open deep-health cost-metrics silas "cost-metrics.log is 15h stale"
  "$CASE" open crawler-out-of-sync code kade "code out of step with git for 2h"
  run "$DIGEST" silas kade
  test "$status" -eq 0
  grep -qF "Open alert cases for silas — 2" "$T/digests/silas.md"
  grep -qF "bedroom-disk / bedroom-external: Bedroom external disk at 98%" "$T/digests/silas.md"
  grep -qF "deep-health / cost-metrics" "$T/digests/silas.md"
  test -z "$(grep -F crawler-out-of-sync "$T/digests/silas.md" || true)"
  grep -qF "Open alert cases for kade — 1" "$T/digests/kade.md"
}

@test "a closed case leaves the digest" {
  "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  "$CASE" close bedroom-disk bedroom-external
  run "$DIGEST" silas
  grep -qF "Open alert cases for silas — 0" "$T/digests/silas.md"
}

@test "NEGATIVE PROOF: an alert firing between digests writes nothing to any pane" {
  run "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -eq 0
  run "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  run "$DIGEST" silas
  test "$status" -eq 0
  test ! -s "$T/pane.log"
  # not vacuous: the case was written and the digest names it
  grep -qF "bedroom-disk / bedroom-external" "$T/digests/silas.md"
}

@test "a door that does not answer keeps the last digest and says so, never an empty one" {
  "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  "$DIGEST" silas
  printf '#!/bin/bash\necho "curl: (7) Failed to connect" >&2; exit 7\n' > "$T/bin/curl"
  run "$DIGEST" silas
  test "$status" -ne 0
  printf '%s' "$output" | grep -qF "the last digest stays"
  grep -qF "bedroom-disk / bedroom-external" "$T/digests/silas.md"
}

@test "with no roles named, every agent role the roles door lists gets a digest, and a person does not" {
  "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  run "$DIGEST"
  test "$status" -eq 0
  test -f "$T/digests/silas.md"
  test -f "$T/digests/kade.md"
  test ! -e "$T/digests/jeff.md"
}

# the schedule itself: platform/pipelines/alert-digest.yaml (made by chorus-make graph from
# the Pipeline rows) runs alert-digest every 5 minutes on its own one-at-a-time queue
@test "the alert-digest pipeline runs alert-digest every 5 minutes, one at a time" {
  Y="$ROOT/platform/pipelines/alert-digest.yaml"
  test -f "$Y"
  grep -qF '# GENERATED by chorus-make from the graph' "$Y"
  grep -qxF 'schedule: "*/5 * * * *"' "$Y"
  grep -qxF 'queue: alert-digest' "$Y"
  grep -qF 'command: "${CHORUS_HOME}/platform/scripts/alert-digest"' "$Y"
  # the queue it names exists, capped at one run
  grep -A1 -- '- name: alert-digest' "$ROOT/platform/config/dagu.yaml" | grep -qF 'max_concurrency: 1'
  # a scheduled run has no card: its exit wakes no one (run-finished refuses an empty card)
  test -z "$(grep -F run-finished "$Y" || true)"
}
