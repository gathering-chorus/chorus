#!/usr/bin/env bats
# @test-type: integration — drives platform/scripts/alert-case against a fake door (a stub curl that keeps rows in a file); never reaches the identity or model door.
# @domain: alerts
# @card: #4957 · owner: silas
#
# #4957 AC1 — an alert is one AlertCase row per (alert, subject). A second firing
# of the same condition moves lastSeen on the same row, never a new one. Jeff
# 2026-11-11 09:50: "every time we touch alerts they're all just like snowflakes."
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/alert-case"
  T="$BATS_TEST_TMPDIR"
  echo '[]' > "$T/rows.json"
  # the fake door: GET lists rows matching the query's caseOf/caseSubject (open ones when
  # caseClosedAt.absent=true); POST adds a row; PATCH merges fields into the named row.
  # Every write is logged as "<METHOD> <url> <body>" to writes.log.
  cat > "$T/curl" <<EOS
#!/bin/bash
m=GET; body=""; url=""; auth=0
while [ \$# -gt 0 ]; do case "\$1" in -X) m="\$2"; shift ;; --data-binary) body=\$(cat "\${2#@}"); shift ;; -H) case "\$2" in @*) grep -q '^Authorization: Bearer .' "\${2#@}" && auth=1 ;; esac; shift ;; -o|-w|--max-time) shift ;; http*) url="\$1" ;; esac; shift; done
export FAKE_AUTH=\$auth
[ "\$m" = GET ] || printf '%s %s %s\n' "\$m" "\$url" "\$body" >> "$T/writes.log"
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
    out=[r for r in rows if all(r.get(k)==v for k,v in q.items() if k in("caseOf","caseSubject"))
         and (q.get("caseClosedAt.absent")!="true" or not r.get("caseClosedAt"))]
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
  chmod +x "$T/curl"
  export ALERT_CURL="$T/curl" ALERT_DOOR="http://door.test"
  # the alerts principal's token (a stub minter: asked for "alerts", it answers one)
  printf '#!/bin/bash\n[ "$1" = alerts ] && echo tok-alerts || { echo "no credential for $1" >&2; exit 3; }\n' > "$T/token"; chmod +x "$T/token"
  export ALERT_TOKEN_BIN="$T/token"
}

rows()  { python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))))' "$T/rows.json"; }
field() { python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[int(sys.argv[2])].get(sys.argv[3],""))' "$T/rows.json" "$1" "$2"; }

@test "a first firing opens one case: what, since, owner, still true, cause, the rule it is a case of" {
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -eq 0
  test "$(rows)" -eq 1
  test "$(field 0 caseOf)" = bedroom-disk
  test "$(field 0 caseSubject)" = bedroom-external
  test "$(field 0 ownedBy)" = principal-silas
  test "$(field 0 stillTrue)" = True
  test "$(field 0 caseCause)" = "Bedroom external disk at 98%"
  test -n "$(field 0 firstSeen)"
  test "$(field 0 firstSeen)" = "$(field 0 lastSeen)"
}

@test "NEGATIVE PROOF: the same condition fired twice makes one row, and the second moves lastSeen" {
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  first=$(field 0 firstSeen)
  sleep 1
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -eq 0
  test "$(rows)" -eq 1
  test "$(field 0 firstSeen)" = "$first"
  test "$(field 0 lastSeen)" != "$first"
  # the second firing PATCHed the open case, it did not POST another
  test "$(grep -c '^POST ' "$T/writes.log")" -eq 1
  test "$(grep -c '^PATCH ' "$T/writes.log")" -eq 1
}

@test "a different subject of the same rule is its own case" {
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  run "$SCRIPT" open bedroom-disk bedroom-internal silas "Bedroom internal disk at 91%"
  test "$(rows)" -eq 2
}

@test "a door that cannot be read opens nothing and says so, never a second row" {
  printf '#!/bin/bash\necho "curl: (7) Failed to connect" >&2; exit 7\n' > "$T/curl"
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -ne 0
  printf '%s' "$output" | grep -qF "alert-case: the door did not answer"
}

@test "NEGATIVE PROOF: with no token for the alerts principal, nothing is written and it says why" {
  export ALERT_PRINCIPAL=nobody
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -ne 0
  printf '%s' "$output" | grep -qF "no token for nobody"
  test "$(rows)" -eq 0
}

@test "NEGATIVE PROOF: a write without the Authorization header is refused by the door, as the real door does" {
  printf '#!/bin/bash\necho ""\n' > "$T/token"   # a minter that answers nothing usable
  run "$SCRIPT" open bedroom-disk bedroom-external silas "Bedroom external disk at 98%"
  test "$status" -ne 0
  test "$(rows)" -eq 0
}
