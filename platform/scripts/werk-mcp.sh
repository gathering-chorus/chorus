#!/usr/bin/env bash
# werk-mcp.sh — RETIRED into werk.yml (#3236). Fail-loud deprecation stub.
#
# The 8-step bash orchestrator is gone. The pipeline is now ONE act workflow,
# .github/workflows/werk.yml, run via act (ADR-030/032). werk-mcp.sh and werk.yml were
# both thin flat sequences over the SAME atomic verbs (werk-commit, werk-push,
# chorus_build, chorus_deploy, chorus_env_up, werk-demo, werk-merge) — collapsing to one
# changes nothing about the verbs; it removes the second sequencer. The atomicity lives
# in the verbs; the orchestrator is a list. (Jeff, 2026-06-05: "werk-mcp.sh fundamentally
# makes all of our verbs atomic.")
#
# This is a fail-loud stub (the team's retirement pattern, cf. the retired bash `nudge`
# stub). It does NOT re-exec act itself — a re-exec would have to choose canonical-vs-werk
# werk.yml and would break self-modifying cards. Callers run the act pipeline directly.
#
# Accept stays the human's hand (DEC-048): the pipeline stops before accept; the human
# runs `werk-accept <card> <role>`.
set -uo pipefail

ROLE="${1:-<role>}"
CARD="${2:-<card>}"
ACCEPTER="${3:-jeff}"

cat >&2 <<EOF
werk-mcp.sh is RETIRED (#3236) — the pipeline is now one act workflow: werk.yml.

Run the pipeline directly:
  act workflow_dispatch -W .github/workflows/werk.yml -P macos-latest=-self-hosted \\
    --input card_id=${CARD} --input role=${ROLE} --input accepter=${ACCEPTER}

(run from the card's werk so act uses that werk's werk.yml). The run stops before accept;
then the human accepts:
  DEPLOY_ROLE=${ACCEPTER} werk-accept ${CARD} ${ROLE}
EOF
exit 1
