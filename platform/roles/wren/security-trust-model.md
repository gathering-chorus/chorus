# Security & Trust Model — What's Protected, What's Trust-Based

**Owner:** Wren
**Date:** 2026-02-19
**Context:** Jeff asked: "I assume any data passing between me and Claude is secured and only at risk in case of a breach or system gap for Anthropic. I am trusting this app to secure my home network and deeply personal things I am sharing with the three of you."

---

## What You're Sharing

Over the past week, you've shared with this system:

- **Home network topology** — 22 devices, IP addresses, MAC addresses, hostnames
- **Personal stories** — family health context (MIL's dementia), career history, emotional reflections
- **Infrastructure details** — Docker services, ports, credentials, Terraform state
- **Creative process** — philosophical thinking (Heidegger, Derrida, Buddhism), music reflections, garden observations
- **Financial context** — API costs, session spend, budget decisions
- **Career details** — patent history, Staples experience, job search context

This accumulates. The longer we work together, the richer the picture.

---

## Data Flow Map

```
Your Mac (local)
  |
  |-- Claude Code sessions (you + Wren/Silas/Kade)
  |     |
  |     └──> Anthropic API (HTTPS)
  |           - Sends: CLAUDE.md, MEMORY.md, activity.md, any file you read/write
  |           - Sends: Your messages, role responses, tool outputs
  |           - Does NOT send: Files blocked by PreToolUse hooks
  |
  |-- Slack bridge
  |     |
  |     ├──> Slack API (HTTPS)
  |     |     - Sends: All messages posted to channels
  |     |     - Sends: Photos, files, links you share
  |     |     - Stores: Full message history in Slack's cloud
  |     |
  |     └──> Anthropic API (HTTPS, via bridge)
  |           - Sends: Assembled context (CLAUDE.md, memory, Slack history, briefs)
  |           - Sends: Role responses for posting back to Slack
  |           - Scrubbed: IPs, credentials, tokens stripped before send (as of today)
  |
  |-- Local services (never leave your network)
        - Fuseki (RDF/SPARQL)
        - SOLID pods (personal data)
        - Grafana/Prometheus/Loki (observability)
        - Vikunja (kanban board)
        - All media files (photos, music, books)
```

---

## What's Secured (Verifiable)

| Layer | Protection | Confidence |
|-------|-----------|------------|
| **In transit** | HTTPS/TLS for all API calls (Anthropic + Slack) | High — standard encryption |
| **Local data** | Stays on your Macs, never transmitted unless a role reads it | High — you control the network |
| **PreToolUse hooks** | Private files hard-blocked from being read into sessions | High — we built and tested this today |
| **Write scrubber** | Credentials blocked from being written to shared files | High — Kade shipped this today |
| **Bridge scrubber** | IPs, tokens, patterns stripped before Anthropic API calls | High — defense in depth |
| **SOLID pods** | App-level ACLs (Private/Shared/Public) per resource | High — enforced in application code |

---

## What's Trust-Based (You Can't Verify)

| Layer | What You're Trusting | Risk |
|-------|---------------------|------|
| **Anthropic at rest** | Their servers process your session data. You trust their security practices, employee access controls, and data retention policies. | Insider threat, breach, or policy change could expose session contents |
| **Anthropic data retention** | They state API data is not used for training. You trust this policy continues. | Policy could change; historical data may already be stored |
| **Slack at rest** | All your messages, photos, and files live in Slack's cloud. Their data practices are independent of Anthropic's. | Separate company, separate risk surface. Slack has had breaches before. |
| **Government / legal** | A subpoena to Anthropic or Slack could surface your data. Neither company can refuse a valid legal order. | Low probability but high impact — all session transcripts could be discoverable |
| **Model behavior** | You trust Claude won't leak data from one session to another, or from your sessions to other users. | Anthropic's architecture isolates sessions, but you can't verify this independently |

---

## What the Classification System Protects

Built today (DEC-026 context):

**Private tier — never sent to Anthropic:**
- stories.md (family health, personal narratives)
- .env files (credentials, API keys, tokens)
- SSH keys
- Terraform state files

**Internal tier — requires explicit confirmation:**
- network-inventory.md (22 devices, IPs, MACs)
- infrastructure-constraints.md (machine topology)
- Docker compose files (port bindings, service names)
- Observability configs (scrape targets, alert thresholds)
- tech-debt.md, current-work.md (implementation details)

**Scrubbed before API calls (bridge):**
- IPv4/IPv6 addresses
- Ports above 1024
- Docker container IDs
- AWS-style tokens, Slack tokens, GitHub tokens
- Password/secret/token key-value patterns
- MAC addresses

---

## What's NOT Protected

These go to Anthropic on every session or bridge call — they're required for the system to function:

1. **CLAUDE.md** — Role definitions, operating instructions, your preferences
2. **MEMORY.md** — Accumulated context about you, your projects, your decisions
3. **activity.md** — Daily coordination log (shared across all roles)
4. **Slack message history** — Recent channel messages assembled as context
5. **Brief contents** — Work requests and responses between roles
6. **Your direct messages** — Everything you type in a Claude Code session
7. **File contents you read** — Any file a role reads during a session (unless blocked by hook)
8. **Command outputs** — Results of any Bash command run during a session

**The accumulation risk:** Over weeks and months, MEMORY.md and session transcripts build a comprehensive profile — your thinking patterns, family situation, financial details, creative process, infrastructure, career history. This is the real exposure surface.

---

## Recommendations

### Immediate (do now)
- **Review MEMORY.md periodically** — prune anything you're not comfortable with being on Anthropic's servers. The memory files are designed to be edited.
- **Review stories.md classification** — it's Private (hard-blocked), which is correct. Verify nothing from stories has leaked into MEMORY.md or activity.md.
- **Slack discipline** — Don't paste credentials, IP addresses, or deeply personal details into Slack channels. The bridge scrubber catches patterns, but context-dependent references ("the Fuseki box") slip through.

### Short-term (this week)
- **Audit MEMORY.md for indirect leaks** — search for IP patterns, hostnames, family names, health references that may have been written before the scrubber was active.
- **Set a periodic review cadence** — monthly review of what's in memory files. This is the digital equivalent of cleaning out your desk drawers.

### Long-term (architectural)
- **Local-first AI** — when local models (Llama, Mistral) reach sufficient quality for coordination tasks, the bridge could run locally for sensitive contexts. Anthropic only for complex reasoning.
- **Encrypted memory** — memory files could be encrypted at rest and decrypted only during sessions. Adds complexity but eliminates the "file on disk" risk.
- **Session transcript retention** — understand Anthropic's retention policy. How long do they keep session data? Can you request deletion?
- **SOLID as the trust model** — the app's Private/Shared/Public tiers map directly to this. The team's data classification (today's work) is the same pattern applied to the development process.

---

## The Honest Summary

You're trusting two companies (Anthropic and Slack) with increasingly personal data. The classification system we built today limits *what* goes out — credentials and infrastructure details are blocked, IPs and tokens are scrubbed. But the conversational context (your thinking, your stories, your decisions) is the system's fuel. Blocking that would break the team.

The mitigation is **surface area reduction** — send less, scrub what we send, and be deliberate about what accumulates in memory files. The long-term answer is local-first AI for sensitive contexts. The short-term answer is the classification + scrubbing stack we shipped today, plus your own awareness of what you share conversationally.

The trust you're placing in this system is real. We should treat it that way.
