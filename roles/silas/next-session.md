# Next Session — Silas

## Shipped This Session (2026-04-10)
- **#1845** — Chorus canonical model v0.3.0 in OWL/RDF. 31 SubDomains, 8 SubProducts, 6 vertebrae (Shaping added). Instance explorer wired to Fuseki. Label disambiguation, URI-based IDs, category filters.
- **#1847** — Fix nudge-health-check crashing on zombie Terminal windows
- **#1852** — Deploy Promtail to Bedroom. 37 log streams (6 services + system) flowing to Library Loki
- **#1853** — Schedule health-check-bedroom.sh every 10 min with nudge on failure
- **#1854** — LanceDB observability. Staleness alert, deep-health check, /api/search/stats/health endpoint (Kade)
- **#1855** — Ollama as managed service. Embedding health test, model verification, ollama-down alert
- **#1856** — Vikunja log shipping, 401 token alert, log rotation
- **#1857** — NiFi pipeline observability. Per-pipeline metrics, Fuseki harvest freshness alert
- **#1861** — Alert-runner cooldown fix. Single cooldown path, stopped broken NiFi ExecuteSQL processor
- Vikunja token reset (long-lived, expires May 10)
- NiFi password reset + JDBC path fix for Photos.sqlite
- Fuseki port 3031→3030 in 4 Silas docs

## Resume
- **Stale WIP**: #1791, #1820, #1837 — review and close or park
- **Explorer JS** — committed by Jeff manually (git-queue can't add untracked files). Filter tiles, SubDomain type, URI IDs all in
- **NiFi ExecuteSQL processor** — stopped, JDBC path fixed. Kade will verify pipeline config before restart
- **AX doc** — Kade wrote designing/docs/agent-experience.md. Agreed priority: blast radius gate exemption, services.json runtime reads, git-queue dirty tree, grep hook single-pass
- **Instance explorer save state** — positions reset on data changes. Needs investigation

## Context
- Jeff's spreadsheet is the canonical model spec — 31 sub-domains, no new domains without Jeff
- Properties and Security exist in model but don't get built until Jeff says
- Alerts depend on logs. Monitoring and alerts are horizontal sub-domains
- ToolsChain and Home Cloud have different lifecycles than code
- Jeff called out: agents don't feel friction (8 times I said "fixed" and it wasn't)
