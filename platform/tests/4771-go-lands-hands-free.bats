#!/usr/bin/env bats
# @test-type: unit — reads the generated pipelines and fixture runs; no live dagu
# @domain: cicd
# @card: #4771
# @owner: kade
# #4771 — after Jeff signs go, the card lands with nobody touching it. What Jeff
# sees: one go per card, and that go starts the land at once; an older go of
# the same card cannot be signed. (#4642's land on 11-01 took four tries: the
# fixes for those are 4771-land-retry-data-dir.bats and the deploy credential
# in services/shared/deploy_credential.rs. This file proves the go half.)

REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# go_starts_land <cicd-go.yaml> — ok, or the reason the go does not start the land.
go_starts_land() {
  python3 - "$1" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
steps = re.split(r"\n  - name: ", text.split("\nsteps:", 1)[-1])
by = {}
for s in steps[1:]:
    name = s.split("\n", 1)[0].strip()
    by[name] = s
go = by.get("skill-werk-go", "")
if "action: human.task" not in go:
    sys.exit("no go step (skill-werk-go human.task)")
after = [(n, s) for n, s in by.items() if re.search(r"depends:\s*\[[^\]]*skill-werk-go", s)]
land = [(n, s) for n, s in after if re.search(r"enqueue\s+cicd-land\b", s)]
if not land:
    sys.exit("no step after the go enqueues cicd-land")
n, s = land[0]
if "human.task" in s:
    sys.exit(f"{n}: a human answers it — the land waits for hands")
if not re.search(r'condition:\s*"\$\{skill_werk_go\.outputs\.decision\}"\s*\n\s*expected:\s*"go"', s):
    sys.exit(f"{n}: runs without decision=go")
if "ACCEPTER=${skill_werk_go.outputs.accepter}" not in s:
    sys.exit(f"{n}: the land does not carry the go's accepter")
others = [m for m, t in after if m != n and "human.task" in t]
if others:
    sys.exit(f"a human step after the go: {others}")
print("ok")
PY
}

@test "AC1: Jeff's go starts the land in the same run, with no human step between" {
  run go_starts_land "$REPO/platform/pipelines/cicd-go.yaml"
  [ "$status" -eq 0 ] && [ "$output" = "ok" ] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a go that does not enqueue the land is caught" {
  bad="$BATS_TEST_TMPDIR/cicd-go.yaml"
  sed 's/enqueue cicd-land/echo cicd-land/' "$REPO/platform/pipelines/cicd-go.yaml" > "$bad"
  run go_starts_land "$bad"
  [ "$status" -ne 0 ] && [[ "$output" == *"no step after the go enqueues cicd-land"* ]] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a land step that waits for a human again is caught" {
  bad="$BATS_TEST_TMPDIR/cicd-go.yaml"
  python3 - "$REPO/platform/pipelines/cicd-go.yaml" "$bad" <<'PY'
import sys
s = open(sys.argv[1]).read()
s = s.replace('    description: "enqueue the land on the werk-land queue"',
              '    description: "enqueue the land on the werk-land queue"\n    action: human.task', 1)
open(sys.argv[2], "w").write(s)
PY
  run go_starts_land "$bad"
  [ "$status" -ne 0 ] && [[ "$output" == *"a human answers it"* ]] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a land that starts on a no is caught" {
  bad="$BATS_TEST_TMPDIR/cicd-go.yaml"
  sed 's/expected: "go"/expected: "no"/' "$REPO/platform/pipelines/cicd-go.yaml" > "$bad"
  run go_starts_land "$bad"
  [ "$status" -ne 0 ] && [[ "$output" == *"runs without decision=go"* ]] || { echo "$output"; return 1; }
}

# ── AC2: one go per card ──────────────────────────────────────────────────────

# supersede_first <pipeline-start> — ok when the card's older go is stopped
# before its new cicd run is enqueued.
supersede_first() {
  python3 - "$1" <<'PY'
import re, sys
lines = open(sys.argv[1]).read().splitlines()
sup = next((i for i, l in enumerate(lines) if "cicd-supersede" in l and not l.lstrip().startswith("#")), None)
enq = next((i for i, l in enumerate(lines) if re.search(r"enqueue\s+(\"\$workflow\"|cicd\b(?!-))", l) and not l.lstrip().startswith("#")), None)
if sup is None:
    sys.exit("pipeline-start never calls cicd-supersede")
if enq is None:
    sys.exit("pipeline-start never enqueues cicd")
if sup > enq:
    sys.exit(f"cicd-supersede (line {sup+1}) runs after the cicd enqueue (line {enq+1})")
print("ok")
PY
}

@test "AC2: a card's new cicd run stops its older go before it is enqueued" {
  run supersede_first "$REPO/platform/scripts/pipeline-start"
  [ "$status" -eq 0 ] && [ "$output" = "ok" ] || { echo "$output"; return 1; }
}

@test "AC2: the older go waiting for Jeff is the one stopped, never a land" {
  run python3 - "$REPO/platform/scripts/cicd-supersede" <<'PY'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("sup", sys.argv[1])
spec = importlib.util.spec_from_loader("sup", loader); sup = importlib.util.module_from_spec(spec); loader.exec_module(sup)
runs = [{"name": "cicd-go", "dagRunId": "go-old", "statusLabel": "waiting", "params": "CARD=4771 ROLE=kade CICD_RUN=old"},
        {"name": "cicd-land", "dagRunId": "land", "statusLabel": "waiting", "params": "CARD=4771 ROLE=kade"}]
print(sup.superseded(runs, 4771))
PY
  [ "$output" = "[('cicd-go', 'go-old')]" ] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a pipeline-start that enqueues before it supersedes is caught" {
  bad="$BATS_TEST_TMPDIR/pipeline-start"
  python3 - "$REPO/platform/scripts/pipeline-start" "$bad" <<'PY'
import sys
lines = open(sys.argv[1]).read().splitlines()
i = next(i for i, l in enumerate(lines) if "cicd-supersede" in l and not l.lstrip().startswith("#"))
moved = lines.pop(i)
open(sys.argv[2], "w").write("\n".join(lines + [moved]) + "\n")
PY
  run supersede_first "$bad"
  [ "$status" -ne 0 ] && [[ "$output" == *"runs after the cicd enqueue"* ]] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a pipeline-start with no supersede is caught" {
  bad="$BATS_TEST_TMPDIR/pipeline-start"
  grep -v cicd-supersede "$REPO/platform/scripts/pipeline-start" > "$bad"
  run supersede_first "$bad"
  [ "$status" -ne 0 ] && [[ "$output" == *"never calls cicd-supersede"* ]] || { echo "$output"; return 1; }
}
