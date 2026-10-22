#!/usr/bin/env bats
# @test-type: unit — the clearing-evaluate land rows' store-write plan against a stub door; no live service
# @domain: pipelines — scripts/land-4682-rows writes the rows pipelines/cicd-land.yaml is generated from (#4682)
# What Jeff sees: before any prod write, a dry run lists the clearing-evaluate skill and
# the cicd-land-7 StepSkill, says CHANGE or ADD, SAME for rows already right, and writes nothing; an unreadable door gives no plan.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  PLAN="$ROOT/platform/scripts/land-4682-rows"
  D="$BATS_TEST_TMPDIR/door"; mkdir -p "$D"
  cat > "$BATS_TEST_TMPDIR/door.py" <<'PY'
import http.server, json, os, sys, time
D = sys.argv[1]
ALLOWED = ["human", "agent", "deterministic", "http", "pipeline"]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        p = os.path.join(D, self.path.split("?")[0].strip("/").replace("/", "_") + ".json")
        if not os.path.exists(p):
            self.send_response(404); self.end_headers(); self.wfile.write(b'{"data":{}}'); return
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
        self.wfile.write(open(p).read().encode())
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length).decode() if length else "{}"
        try: payload = json.loads(body)
        except Exception: payload = {}
        fields = payload.get("fields", {})
        if isinstance(fields, str): fields = json.loads(fields)
        if "/skills/skills" in self.path:
            executor = fields.get("executor")
            if executor not in ALLOWED:
                self.send_response(422)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"error": f"executor {executor!r} not in allowed {ALLOWED}"}).encode())
                return
            row_file = os.path.join(D, "skills_skills_" + fields.get("name", "clearing-evaluate") + ".json")
            with open(row_file, "w") as f: json.dump({"data": fields, "links": {}}, f)
        elif "/skills/stepskills" in self.path:
            row_file = os.path.join(D, "skills_stepskills_" + fields.get("name", "cicd-land-7") + ".json")
            with open(row_file, "w") as f: json.dump({"data": fields, "links": {}}, f)
        open(os.path.join(D, "writes"), "a").write(self.path + "\n")
        self.send_response(201 if "add" in self.path else 200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b"{}")
    do_PATCH = do_POST
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(os.path.join(D, "port"), "w").write(str(s.server_address[1]))
s.timeout, end = 1, time.time() + 20
while time.time() < end:
    s.handle_request()
PY
  python3 "$BATS_TEST_TMPDIR/door.py" "$D" >/dev/null 2>&1 3>&- &
  for _ in $(seq 1 50); do [ -s "$D/port" ] && break; perl -e 'select(undef,undef,undef,0.1)'; done
  export OWL_URL="http://127.0.0.1:$(cat "$D/port")" CHORUS_RUNNER="$BATS_TEST_TMPDIR/runner"

  cat > "$BATS_TEST_TMPDIR/runner" <<'PY'
#!/usr/bin/env python3
import json, os, sys, urllib.request

door = os.environ.get("OWL_URL")
args = sys.argv[1:]
verb = args[1]
dom = [a.split("=")[1] for a in args if a.startswith("domain=")][0]
cls = [a.split("=")[1] for a in args if a.startswith("class=")][0]
fields_raw = [a[len("fields="):] for a in args if a.startswith("fields=")][0]
url = f"{door}/{dom}/{cls}/{verb}"

req = urllib.request.Request(url, data=json.dumps({"fields": fields_raw}).encode(), headers={"Content-Type": "application/json"}, method="POST")
try:
    with urllib.request.urlopen(req) as resp:
        ok_tag = " → 201" if verb == "athena-add" else " → 200"
        print(f"call {verb} {dom}/{cls}{ok_tag}")
        sys.exit(0)
except urllib.error.HTTPError as e:
    err_body = e.read().decode()
    print(f"call {verb} refused {e.code}: {err_body}")
    sys.exit(1)
PY
  chmod +x "$BATS_TEST_TMPDIR/runner"
}

@test "an empty store plans clearing-evaluate skill and stepskill and writes nothing" {
  run python3 "$PLAN"
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 2 add, 0 change; 0 same — nothing written (dry run)"* ]] || return 1
  [[ "$output" == *'ADD    skills/skills/clearing-evaluate '*'"hasDomain": "cards"'* ]] || return 1
  [[ "$output" == *'ADD    skills/stepskills/cicd-land-7 '*'"callsSkill": "clearing-evaluate"'* ]] || return 1
  [ ! -e "$D/writes" ]
}

@test "negative proof: a row already right reads SAME, and one that differs never does" {
  printf '{"data":{"label":"cicd land 7: clearing-evaluate","skillOrder":"7","withArguments":"continue_on=failure enqueue=true"},"links":{"callsSkill":"chorus:skill-clearing-evaluate","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-abby-normal"}}' > "$D/skills_stepskills_cicd-land-7.json"
  run python3 "$PLAN"
  [[ "$output" == *"SAME   skills/stepskills/cicd-land-7"* ]] || return 1
  printf '{"data":{"label":"cicd land 7: clearing-evaluate","skillOrder":"7","withArguments":""},"links":{"callsSkill":"chorus:skill-clearing-evaluate","forStep":"chorus:pipeline-step-cicd-land","ownedBy":"chorus:principal-abby-normal"}}' > "$D/skills_stepskills_cicd-land-7.json"
  run python3 "$PLAN"
  [[ "$output" == *"CHANGE skills/stepskills/cicd-land-7"* ]] || return 1
  [[ "$output" != *"SAME   skills/stepskills/cicd-land-7"* ]] || return 1
}

@test "negative proof: an unreadable door gives no plan" {
  OWL_URL="http://127.0.0.1:9" run python3 "$PLAN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"no plan"* ]] || return 1
  [[ "$output" != *"plan: door writes"* ]] || return 1
}

@test "--apply with write refused says REFUSED and exits 1, never applied" {
  CHORUS_RUNNER="$BATS_TEST_TMPDIR/no-runner" run python3 "$PLAN" --apply
  [ "$status" -eq 1 ]
  [[ "$output" == *"REFUSED 2 of 2"* ]] || return 1
  [[ "$output" != *"applied"* ]] || return 1
}

generate_land_ttl() {
  local target="$1"
  python3 -c '
import json, sys
ROOT = "'"$ROOT"'"
sys.path.insert(0, ROOT + "/platform/scripts")
import importlib.machinery, importlib.util
loader = importlib.machinery.SourceFileLoader("land", "'"$PLAN"'")
spec = importlib.util.spec_from_loader("land", loader)
land = importlib.util.module_from_spec(spec)
loader.exec_module(land)

ttl = ["""@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:principal-abby-normal a chorus:Principal .
chorus:pipeline-step-cicd-land a chorus:PipelineStep .
chorus:cards a chorus:Domain .
"""]
for s in land.SKILLS:
    ttl.append(f"""chorus:skill-{s["name"]} a chorus:Skill ;
    rdfs:label {json.dumps(s["label"])} ;
    rdfs:comment {json.dumps(s["comment"])} ;
    chorus:executor {json.dumps(s["executor"])} ;
    chorus:implementedBy {json.dumps(s["implementedBy"])} ;
    chorus:hasDomain chorus:{s["hasDomain"]} ;
    chorus:ownedBy chorus:{s["ownedBy"]} .
""")
for st in land.STEPSKILLS:
    ttl.append(f"""chorus:stepskill-{st["name"]} a chorus:StepSkill ;
    rdfs:label {json.dumps(st["label"])} ;
    chorus:forStep chorus:pipeline-step-{st["forStep"]} ;
    chorus:callsSkill chorus:skill-{st["callsSkill"]} ;
    chorus:skillOrder {st["skillOrder"]} ;
    chorus:withArguments {json.dumps(st["withArguments"])} ;
    chorus:ownedBy chorus:{st["ownedBy"]} .
""")
with open("'"$target"'", "w") as f:
    f.write("\n".join(ttl))
'
}

@test "land-4682-rows planned rows conform to skills-4467.ttl shape (#4693 AC1)" {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  SHAPE="$ROOT/roles/wren/ontology/skills-4467.ttl"
  DATA="$BATS_TEST_TMPDIR/land-rows.ttl"
  generate_land_ttl "$DATA"

  run shacl validate --shapes "$SHAPE" --data "$DATA"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "sh:conforms  true" ]] || return 1
}

@test "negative proof: pre-#4693 shape (c185fdd5a) refuses land-4682-rows executor 'pipeline' (#4693 AC2 negative proof)" {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  DATA="$BATS_TEST_TMPDIR/land-rows.ttl"
  generate_land_ttl "$DATA"
  OLD_SHAPE="$BATS_TEST_TMPDIR/old-shape.ttl"
  git -C "$ROOT" show c185fdd5a:roles/wren/ontology/skills-4467.ttl > "$OLD_SHAPE"

  run shacl validate --shapes "$OLD_SHAPE" --data "$DATA"
  [[ "$output" =~ "sh:conforms  false" ]] || return 1
  [[ "$output" =~ "not in expected values" ]] || return 1
  [[ "$output" =~ "pipeline" ]] || return 1
}

@test "--apply writes rows to door and re-run reads SAME (#4693 AC1)" {
  run python3 "$PLAN" --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 2 add, 0 change; 0 same — applied"* ]] || return 1

  run python3 "$PLAN" --apply
  [ "$status" -eq 0 ]
  [[ "$output" == *"plan: door writes 0 add, 0 change; 2 same — applied"* ]] || return 1
}
