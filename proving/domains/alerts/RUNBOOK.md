# Alert Runbook (#3519)

Alerts report the OBSERVATION (what was measured). Diagnosis + remediation live HERE,
keyed by alert name — the operator (or a reasoning layer) reads the data, then consults this.
An alert never prescribes a fix in its message; a wrong guess in a page is harmful (e.g. "regenerate"
a token that is valid for years).

| alert | observation it reports | likely causes to investigate | remediation hints |
|---|---|---|---|
| app-down | :3000/health non-200 | app crashed, port conflict, deploy mid-flight | app-state.sh restart jeff-bridwell-personal-site; check ~/Library/Logs/gathering-app.log |
| daily-review-missing | no ops.review.completed for today by 06:30 | LaunchAgent unloaded, review crashed | launchctl list \| grep daily-review; run review manually |
| fuseki-harvest-stale | photo-triple SPARQL COUNT = 0 | NiFi PG stopped on Bedroom, harvest unscheduled (#3368) | check NiFi process groups on Bedroom |
| hook-server-down | com.chorus.hooks PID/socket absent | daemon crashed/hung | launchctl kickstart -k gui/$(id -u)/com.chorus.hooks; verify /tmp/chorus-hooks.sock |
| lancedb-stale | newest data/lance mtime > 24h | index build unscheduled (#3367), Ollama down | verify the index build job; Ollama on Bedroom |
| loom-principles-hash-drift | per-role principle hashes not equal | a role booted before a graph change | coordinate /reboot to realign |
| loom-principles-orphans | SPARQL COUNT orphan principles > 0 | mid-write or rename drift | inspect /loom/principles.html or SPARQL |
| seed-write-failure | gathering logs matched seed/SPARQL-fail in 90s | Fuseki down, SPARQL update error | curl localhost:3030/$/ping; Loki seed errors |
| tunnel-down | PID present but 3x /health probes failed | tunnel disconnected | launchctl kickstart -k com.cloudflare.tunnel; cloudflared.log |
| vikunja-auth-failure | {job=vikunja} status=401 count in 5m | a caller is auth-rejected — NOTE: not necessarily the .env token (verify its expiry first) | identify the 401'd caller before touching any token |
| ci-main-red | quality.yml on main: red:<url> OR unverifiable:GH_TOKEN-absent | main landed red, or CI cannot be checked | gh run list --branch main --workflow quality.yml; set GH_TOKEN if unverifiable |
