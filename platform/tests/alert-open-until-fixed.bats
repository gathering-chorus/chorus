#!/usr/bin/env bats
# @test-type: integration — drives platform/scripts/alert-case and alert-digest against a fake door; never reaches a live door.
# @domain: alerts
# @card: #4957 · owner: silas
#
# #4957 AC3 — a case closes only when its source measures the cause fixed
# (alert-case close: stillTrue false), never by mute or by age. Jeff 2026-11-11
# 09:50: Bedroom's disk at 98% stays open and owned until it is not full.
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  mkdir -p "$T/bin" "$T/digests"
  cat > "$T/bin/curl" <<EOS
#!/bin/bash
m=GET; body=""; url=""
while [ \$# -gt 0 ]; do case "\$1" in -X) m="\$2"; shift ;; --data-binary) body=\$(cat "\${2#@}"); shift ;; -H|-o|-w|--max-time) shift ;; http*) url="\$1" ;; esac; shift; done
python3 - "$T/rows.json" "\$m" "\$url" "\$body" <<'PY'
import json,sys,urllib.parse as u
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
    print(json.dumps({"data":out}, indent=1)); print(200); sys.exit()
b=json.loads(body)
if m=="POST": rows.append(b)
else:
    name=u.urlparse(url).path.rsplit("/",1)[-1]
    for r in rows:
        if r["name"]==name: r.update(b)
json.dump(rows,open(p,"w")); print("{}"); print(200)
PY
EOS
  chmod +x "$T/bin/curl"
  export ALERT_CURL="$T/bin/curl" ALERT_DOOR="http://door.test" ALERT_DIGEST_DIR="$T/digests"
  CASE="$ROOT/platform/scripts/alert-case"; DIGEST="$ROOT/platform/scripts/alert-digest"
  # an open case first seen 25h ago, last seen 25h ago: nothing has re-fired since
  old=$(python3 -c 'import time;print(time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime(time.time()-25*3600)))')
  printf '[{"name":"bedroom-disk-old","caseOf":"bedroom-disk","caseSubject":"bedroom-external","ownedBy":"principal-silas","firstSeen":"%s","lastSeen":"%s","stillTrue":true,"caseCause":"Bedroom external disk at 98%%"}]' "$old" "$old" > "$T/rows.json"
}

open_cases() { python3 -c 'import json,sys;print(sum(1 for r in json.load(open(sys.argv[1])) if not r.get("caseClosedAt")))' "$T/rows.json"; }

@test "NEGATIVE PROOF: an unchanged condition 25h old is still open, and still in the digest" {
  run "$DIGEST" silas
  test "$status" -eq 0
  test "$(open_cases)" -eq 1
  grep -qF "Open alert cases for silas — 1" "$T/digests/silas.md"
  grep -qF "bedroom-disk / bedroom-external" "$T/digests/silas.md"
}

@test "a re-firing keeps the same case open, with the original firstSeen" {
  first=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[0]["firstSeen"])' "$T/rows.json")
  run "$CASE" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$(open_cases)" -eq 1
  test "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[0]["firstSeen"])' "$T/rows.json")" = "$first"
}

@test "only the source's measured fix closes it: stillTrue false, closedAt set, out of the digest" {
  run "$CASE" close bedroom-disk bedroom-external
  test "$status" -eq 0
  test "$(open_cases)" -eq 0
  python3 -c 'import json,sys;r=json.load(open(sys.argv[1]))[0];assert r["stillTrue"] is False and r["caseClosedAt"], r' "$T/rows.json"
  run "$DIGEST" silas
  grep -qF "Open alert cases for silas — 0" "$T/digests/silas.md"
}

@test "alert-case has no mute, snooze or expire verb" {
  for v in mute snooze expire ack timeout; do
    run "$CASE" "$v" bedroom-disk bedroom-external
    test "$status" -ne 0
  done
  test "$(open_cases)" -eq 1
}
