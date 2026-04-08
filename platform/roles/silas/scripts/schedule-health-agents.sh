#!/usr/bin/env bash
# schedule-health-agents.sh — Wire into session-start to schedule health crons
# Called by the session-start hook. Outputs cron prompts for Claude to schedule.
#
# This script doesn't create crons directly — it outputs the schedule spec
# that the session-start hook reads and passes to CronCreate.
#
# Usage: schedule-health-agents.sh
#   Outputs JSON-like schedule definitions, one per line.

cat <<'EOF'
SCHEDULE bedroom-health "*/30 * * * *" "Run $CHORUS_ROOT/platform/roles/silas/scripts/health-check-bedroom.sh --card and report any failures. If all OK, stay silent."
SCHEDULE fuseki-baseline "3 9 * * *" "Run $CHORUS_ROOT/platform/roles/silas/scripts/fuseki-baseline.sh and report timing. Alert if any query exceeds 10 seconds."
SCHEDULE stale-handoffs "17 */4 * * *" "Scan briefs directories for handoffs older than 24h. List them. Delete briefs from completed workflows."
SCHEDULE doc-staleness "23 9 * * 1" "Check data/about/*.md for stale numbers: compare test count to actual (grep -c 'it(' tests/**/*.test.*), message count to Chorus index, card count to board. Card any doc that's >10% off."
EOF
