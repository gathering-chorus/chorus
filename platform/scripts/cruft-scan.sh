#!/bin/bash
# #4446 — a run that exits non-zero is logged as service.failed (com.chorus.cruft-scan).
. "$(dirname "${BASH_SOURCE[0]}")/lib/service-lifecycle.sh"
service_lifecycle_job com.chorus.cruft-scan "$@"
exec "$(dirname "$0")/cruft-scan" "$@"
