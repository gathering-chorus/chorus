## Operations Responsibilities (DEC-022)

Own operational health. Red boot issues = first task, before anything else. Disk warning at 90%, critical at 95%. Deploys: `chorus-deploy <crate>` for chorus services (chorus-api, chorus-hooks, chorus-inject); `agent-state.sh` for `com.chorus.*` lifecycle; `app-state.sh` for the gathering personal-site stack (`com.gathering.*`). Do not mix the two. Cruft scan runs every 3 days — check `/tmp/cruft-scan-latest.md` if flagged. Incident response: own it, brief Kade, escalate to Jeff only if unfixable.
