# Data Classification Policy

**Owner:** Wren
**Date:** 2026-02-19
**Status:** Active
**Refs:** DEC-025 (autonomous authority), SOLID ACL model (app-level parallel)

## Purpose

Every Claude Code session and Slack bridge API call sends file contents to Anthropic's servers. This policy classifies data so sensitive information never leaves the machine.

## Three Tiers

### Public

Data that's already on GitHub or could be without risk.

- Source code, TypeScript, CSS, HTML
- CLAUDE.md role definitions
- Decision log (`decisions.md`) — product rationale, no secrets
- Backlog, project status docs
- ADR documents (architectural decisions, no implementation secrets)
- Brief files (work requests and responses)
- Ontology `.ttl` files (schema definitions)
- Team-architecture.md, activity.md (coordination artifacts)

**Enforcement:** None. Freely readable by all roles and sent in API context.

### Internal

System details that help an attacker map the infrastructure. Not personal, but operationally sensitive.

- Network inventory (IPs, hostnames, MAC addresses, device list)
- Docker compose files with port bindings and service names
- Infrastructure constraints with specific resource limits
- Prometheus/Grafana configuration (scrape targets, alert thresholds)
- System architecture docs with endpoint URLs and port numbers
- `.env` files (even if they don't contain secrets, the key names reveal structure)
- Cost data with specific dollar amounts and API key usage patterns
- Terraform state files

**Enforcement:** PreToolUse hook prompts "File classified as Internal — confirm intent" before allowing read. Logged to chorus.log. Not included in bridge context assembly.

### Private

Personal data that must never leave the machine under any circumstances.

- `stories.md` — Jeff's personal stories, family details, health context
- Any file containing personal health information
- Credentials, API keys, tokens (`.env` files with values)
- Jeff's personal photos and media content
- Financial details beyond aggregate cost tracking
- Family member names, addresses, personal details

**Enforcement:** PreToolUse hook hard-blocks read. No override. Logged to chorus.log as security event.

## Sensitive Paths by Role

Each role maintains a `.sensitive-paths` file in their directory declaring Internal and Private files. The PreToolUse hook reads all manifests.

### Format

```yaml
# .sensitive-paths — files that should not be sent to external APIs
internal:
  - path/to/file.md
  - path/to/directory/*
private:
  - path/to/secrets.md
```

### Current Sensitive Files

**Architect (Silas):**
- Internal: `architect/network-inventory.md`, `shared-observability/docker-compose.yml`, `shared-observability/prometheus.yml`, `shared-observability/blackbox.yml`, `architect/infrastructure-constraints.md`
- Private: none currently

**Engineer (Kade):**
- Internal: `jeff-bridwell-personal-site/docker-compose.yml`, `jeff-bridwell-personal-site/terraform/`, `messages/slack-bridge/.env`
- Private: `jeff-bridwell-personal-site/.env`

**Product Manager (Wren):**
- Internal: `messages/cost-log.md` (specific dollar amounts)
- Private: `stories.md` (in memory directory — Jeff's personal narratives)

## Indirect Leak Prevention

### Memory Files
When writing to MEMORY.md or activity.md, scrub:
- IP addresses (192.168.x.x, 10.x.x.x patterns)
- Hostnames and FQDNs
- Port numbers in context of services
- API tokens and credential patterns
- Personal names beyond Jeff's

### Slack Channel Context
The bridge assembles recent Slack messages as API context. Before assembly:
- Strip IP address patterns
- Strip token/credential patterns
- Flag but don't strip service names (needed for conversational continuity)

### Rule of Thumb
If you wouldn't put it in a GitHub README, don't put it in a file that gets sent as API context. When in doubt, classify as Internal.

## Enforcement Stack

| Layer | Owner | Mechanism |
|-------|-------|-----------|
| Classification policy | Wren | This document |
| `.sensitive-paths` manifests | Each role | YAML in role directory |
| PreToolUse hook (Read) | Silas | Block/ask before file read |
| Bridge context scrubber | Kade | Strip patterns before API call |
| Memory write scrubber | Kade | Strip patterns before file write |
| Audit trail | All | All blocks/asks logged to chorus.log |
