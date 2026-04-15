# ADR-013: Boundary Checking Operating Model

**Date**: 2026-02-19
**Status**: Deferred — proposed but not implemented. Credential scrubbing landed in chorus-hooks (write_scrubber.rs); full boundary model remains future work
**Deciders**: Jeff Bridwell, Wren (PM), Silas (Architect), Kade (Engineer)

## Context

Three autonomous AI roles (Silas, Wren, Kade) work across a shared codebase. Each role reads and writes files that other roles depend on. Today, Silas restarted the observability stack 3 times and Vikunja once without signaling Wren or Kade. No data was lost, but the gap exposed two problems:

1. **Dependency blindness**: No role knows which of its files other roles depend on, or how they depend on them. Changes to shared files propagate silently.
2. **Sensitivity blindness**: Every Claude Code session sends system prompts, memory files, and recent Slack history to Anthropic's API. Files containing IPs, credentials, or personal data can leak through normal read operations or indirect accumulation in memory files.

These are the same problem at different scales: **what data crosses which boundaries, and how do we enforce the rules at those boundaries?**

Jeff observed that this maps directly to the SOLID visibility model already in the app (ADR-003): Private/Shared/Public in the application becomes Private/Internal/Public in the operating model. The pattern is: declare the tier, enforce at the access boundary, default to most restrictive.

## Decision

### 1. Unified boundary manifest per role

Each role maintains a `.boundaries.yml` file in their directory:
- `architect/.boundaries.yml`
- `product-manager/.boundaries.yml`
- `engineer/.boundaries.yml`

Format:

```yaml
# Role boundary manifest
# Declares files this role owns that others depend on,
# and files this role considers sensitive.

role: silas
updated: 2026-02-19

files:
  # Boundary declarations: who depends on this file and why
  shared-observability/docker-compose.yml:
    tier: internal
    depended_by:
      - role: kade
        usage: "App stack joins observability-network defined here"
        break_risk: "Network rename breaks app metrics/logs"
    on_change: boundary  # [boundary] commit tag + signal

  architect/ontology/chorus.ttl:
    tier: public
    depended_by:
      - role: kade
        usage: "Class names, property URIs used in SPARQL queries"
        break_risk: "Renamed class breaks queries and Turtle generation"
    on_change: boundary

  architect/infrastructure-constraints.md:
    tier: internal
    depended_by:
      - role: kade
        usage: "Hard constraints (C1-C7) referenced before adding services"
        break_risk: "Changed constraint = wrong capacity assumptions"
      - role: wren
        usage: "Constraints that bound product scope"
        break_risk: "Wren plans work that violates constraints"
    on_change: boundary

  # Sensitivity-only declarations: no dependency, just classification
  architect/network-inventory.md:
    tier: internal
    depended_by: []
    on_change: none

  .env:
    tier: private
    depended_by: []
    on_change: none

# Patterns to scrub from memory/context assembly
# Applied by bridge before API calls and by roles before writing to memory files
scrub_patterns:
  - type: ipv4
    pattern: '\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b'
  - type: mac_address
    pattern: '([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}'
  - type: api_token
    pattern: '(xoxb|xoxp|xapp|sk-|ghp_|gho_)[A-Za-z0-9\-]+'
```

### 2. Three tiers (matching SOLID model)

| Tier | Meaning | Read enforcement | Write enforcement | Memory scrub |
|------|---------|-----------------|-------------------|-------------|
| **public** | Safe to leave the machine. Already on GitHub or equivalent. | None | None | No |
| **internal** | System details that help attackers or expose infrastructure. IPs, ports, architecture diagrams, cost data. | PreToolUse hook: warn + log | PreToolUse hook: warn if writing internal data to a public-tier file | Scrub patterns before writing to memory files |
| **private** | Personal data, credentials, health info, family details. Must never leave the machine. | PreToolUse hook: hard block (no override) | Hard block on write to any non-private file | Scrub from Slack context before API calls |

Default tier for unlisted files: **public** (most files are code/docs that are already on GitHub).

### 3. Three enforcement points

**Point 1: PreToolUse hook (Read/Write gating)**
- Owner: Silas
- Fires before any Read or Write tool call
- Checks requested path against union of all three `.boundaries.yml` manifests
- For `internal`: logs the read, continues (awareness, not blocking)
- For `private`: blocks the read, returns "File classified as private — access denied"
- For write to a lower-tier destination: warns "Writing internal data to public file"

**Point 2: Bridge context scrubbing (Slack → API)**
- Owner: Kade
- Runs before the Slack bridge assembles context for API calls
- Applies `scrub_patterns` from all three manifests to incoming Slack history
- Replaces matches with `[REDACTED:type]` (e.g., `[REDACTED:ipv4]`)
- Defense in depth: also runs final scrub on assembled context before API call

**Point 3: Memory write scrubbing (activity.md, MEMORY.md)**
- Owner: Kade (implementation), all roles (discipline)
- Before appending to any memory file, apply `scrub_patterns`
- Catches "restarted Fuseki on 192.168.1.50" → "restarted Fuseki on [REDACTED:ipv4]"
- Pattern matching catches the majority; human judgment catches context-dependent references

### 4. Two change signals

Changes to files listed in `.boundaries.yml` produce different signals depending on the change type:

| Change type | Commit tag | Signal | Response |
|------------|-----------|--------|----------|
| **Boundary change**: Modified a file that other roles depend on (structural change, renamed property, changed constraint value) | `[boundary]` | Post to #all-gathering + affected role channels | Dependent roles verify their usage on next session start |
| **Infrastructure change**: Operational event (restart, brief outage, config reload) that doesn't change the contract but temporarily affects availability | `[infra]` | Post to #all-gathering immediately | Affected roles note the outage window, no verification needed |

Commits with `[boundary]` tag also emit a Chorus event via the post-commit hook (already wired for card-commit linking).

### 5. Session-start boundary check

On session start, each role:
1. Reads all three `.boundaries.yml` manifests
2. Scans `git log` for `[boundary]` commits since last session touching files they depend on
3. If any found: reports "Boundary changes since your last session" with the file, commit, and the `break_risk` from the manifest
4. Role verifies their usage still works (e.g., "Do my SPARQL queries still match the ontology class names?")

This is the "how to check our boundaries" mechanism Jeff asked for. It runs automatically, references the declared contracts, and tells the role exactly what to verify.

### 6. Policy document (Wren owns)

Wren writes `product-manager/data-classification-policy.md` — the human-readable policy that defines each tier, gives examples per domain, and establishes the principles. The `.boundaries.yml` manifests are the machine-readable implementation of that policy. If the manifest and the policy disagree, the policy wins (same principle as "Slack is wind, files are ground" — the policy is the source of truth, the manifest is enforcement).

## Rationale

- **Same pattern, different scale**: The app uses Private/Shared/Public visibility on collections (ADR-003). The operating model uses Private/Internal/Public on files. Same three-tier model, same default-deny, same enforcement-at-boundary pattern. Consistency across scales.
- **Declarative, not procedural**: Roles declare their boundaries in a YAML file. Enforcement reads the declarations. No role needs to remember what other roles depend on — the manifest tracks it.
- **Defense in depth**: Three enforcement points (read gating, context scrubbing, memory scrubbing) catch leaks at different stages. No single point of failure.
- **Minimal overhead**: The manifest is written once and updated when files change. The session-start check is automatic. The scrub patterns run passively. The `[boundary]` commit tag is the only manual discipline required.
- **Per-role ownership**: Each role declares their own sensitive files and dependencies. No central authority bottleneck. Union logic means adding a file to any manifest protects it globally.

## Consequences

- All three roles must create and maintain `.boundaries.yml` files
- Wren must write the classification policy document before enforcement goes live
- PreToolUse hook adds latency to every Read/Write tool call (file path lookup against manifest — should be <1ms for a flat file)
- Bridge scrubbing may occasionally over-redact (IP-like strings that aren't actually IPs)
- Memory files will contain `[REDACTED:type]` markers, reducing some context for future sessions
- `[boundary]` and `[infra]` commit tags become mandatory team conventions
- Session-start check adds ~5 seconds to role synchronization (git log scan + manifest read)

## Implementation Order

1. Wren: Write `data-classification-policy.md` (policy before enforcement)
2. Silas: Create `architect/.boundaries.yml` (first manifest, establishes format)
3. Kade, Wren: Create their own `.boundaries.yml` files
4. Silas: Wire PreToolUse hook for Read gating
5. Kade: Wire bridge context scrubbing
6. Kade: Wire memory write scrubbing
7. All: Add session-start boundary check to role CLAUDE.md files

## Relationship to Other ADRs

- **ADR-003** (Visibility Enforcement): Same three-tier pattern at application level
- **ADR-006** (Bridge Scope Guardrail): Existing PreToolUse hook for docker commands — this extends the same mechanism for file reads
- **ADR-012** (Network Bind Security): Internal-tier data (ports, IPs) motivated by the bind audit that ADR-012 addressed

— Silas
