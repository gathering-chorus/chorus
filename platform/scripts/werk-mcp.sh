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
# stub). The pipeline trigger is now an MCP VERB — werk-present (#3241) — not a raw `act`
# CLI. The verb encapsulates the act run (canonical werk.yml, host-native, PATH); callers
# pass only {role, card_id, accepter}, like every other verb.
#
# Accept stays the human's hand (DEC-048): the pipeline stops before accept; the human
# runs `werk-accept <card> <role>`.
set -uo pipefail

ROLE="${1:-<role>}"
CARD="${2:-<card>}"
ACCEPTER="${3:-jeff}"

cat >&2 <<EOF
werk-mcp.sh is RETIRED — the pipeline is one act workflow (werk.yml, #3236), and the
trigger is one MCP verb (werk-present, #3241). Don't run raw act.

Run the pipeline via MCP (the single toolchain):
  werk-present { role: "${ROLE}", card_id: ${CARD}, accepter: "${ACCEPTER}" }

It runs Half A and PRESENTS the variant; the human GO then runs werk-land
(merge -> deploy-prod -> accept; GO = accept, #3311):
  werk-land { role: "${ROLE}", card_id: ${CARD}, accepter: "${ACCEPTER}" }
EOF
exit 1
