#!/bin/bash
# #4040 — author the pipelines SERVICE DESIGN as graph data through the
# GENERATED write endpoints (Jeff 15:53: design as data, prose as a view).
# Idempotent: POST then fall back to PUT on already-exists. Two writes:
#   1. the design Document (claim landed by this card mounts /documents;
#      Wren's call 16:22 — Document under the knowledge domain)
#   2. the pipelines Service, hasDesignDoc → that Document
# Run after model deploy (claims live). Refusals surface loudly — this script
# is the AC1 leg of the demo, a silent skip would fake the AC.
set -euo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
OWL="${OWL_URL:-http://localhost:3360}"
TOK=$("$CHORUS_ROOT/platform/scripts/chorus-identity-token" kade 2>/dev/null \
   || "$CHORUS_ROOT/platform/scripts/chorus-identity-token" chorus-sdk)

write() { # write <route> <name> <json>
  local route="$1" name="$2" body="$3" code
  code=$(curl -s --max-time 15 -o /tmp/pipelines-design-seed.out -w '%{http_code}' \
    -X POST "$OWL/$route" -H "Authorization: Bearer $TOK" \
    -H 'Content-Type: application/json' -d "$body")
  if [ "$code" = "409" ] || grep -q 'already' /tmp/pipelines-design-seed.out 2>/dev/null; then
    code=$(curl -s --max-time 15 -o /tmp/pipelines-design-seed.out -w '%{http_code}' \
      -X PUT "$OWL/$route/$name" -H "Authorization: Bearer $TOK" \
      -H 'Content-Type: application/json' -d "$body")
  fi
  case "$code" in 2*) echo "seed: $route/$name $code" ;;
    *) echo "seed FAILED: $route/$name HTTP $code"; cat /tmp/pipelines-design-seed.out; exit 1 ;;
  esac
}

# Wren named pipelines-layer-2026-06-16.html (16:22) — that file does not exist
# anywhere in the tree; nearest live doc is the CI/CD pipeline service design.
write documents pipelines-design '{
  "name":"pipelines-design",
  "label":"Pipelines design",
  "docTitle":"Pipelines — service design (design as data; this Document is the catalog pointer)",
  "docHref":"/gathering-docs/ci-pipeline-service-design.html",
  "hasDomain":"pipelines"
}'

write services pipelines "$(cat <<'JSON'
{
  "name":"pipelines",
  "label":"Pipelines",
  "status":"exploring",
  "atStep":"Building",
  "ownedBy":"role-kade",
  "hasDesignDoc":"document-pipelines-design",
  "implementationPlan":"#4040 lands model + instances + API mounts + the nightly PipelineRun emit. Then: clearing pipeline steps as the Clearing automations firm up; borg steps with Silas; per-step metrics on runs; /loom step-health read (Wren).",
  "pathToClose":"Deploy #4040 (MODEL_SET + instance seed) -> /pipelines + /pipelineruns serve -> first real PipelineRun row from the daily test run -> Wren claims/loom review + Silas /gate-arch -> clearing and borg gain real steps when built.",
  "gaps":"clearing/borg planned only - no steps yet by design. Per-step run metrics not yet modeled (run-level only).",
  "notInScope":"No generator code changes (mounts come from claims). No invented steps for unbuilt pipelines. NiFi/data-harvest pipelines (#1925 sense) stay in their domains.",
  "overview":"Pipelines are the automation workflows for our value-stream steps (Jeff 2026-08-31). Each Pipeline holds ordered PipelineSteps; every step declares its executor blend - human, agent, or deterministic automation (the cicd demo GO is the designed human step). Runs are PipelineRun rows with the pipeline's own numbers: duration, outcome, tests run/failed/stored for the test leg, triples for athena.",
  "asIs":"Two REAL pipelines: cicd (implemented by werk: commit->build->test->demo->land) and athena (shape->forge->seed->validate). clearing and borg are planned instances with no invented steps. The nightly runner emits one PipelineRun per daily run with the required forPipeline link - a run row without it refuses at the door.",
  "toBe":"clearing and borg move planned->operating with real steps as they are built; per-step run metrics; /loom and the board read step health from /pipelines + /pipelineruns."
}
JSON
)"
echo "seed: done"
