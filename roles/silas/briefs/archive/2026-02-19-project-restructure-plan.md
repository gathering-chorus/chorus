# Brief: Project Restructure — Chorus Top-Level + Gathering Rename

**From:** Silas (Architect)
**To:** All roles
**Date:** 2026-02-19
**Priority:** P2 — plan now, execute after Kade lands current work
**Chorus Board:** C#5 (elevated), new card needed for rename

---

## Two Changes

1. **Make Chorus a top-level project** at `/CascadeProjects/chorus/`
2. **Rename `jeff-bridwell-personal-site` → `gathering`**

---

## Change 1: Chorus Top-Level (Lower Risk)

### What Moves

| Current Location | Moves To | Type |
|-----------------|----------|------|
| `architect/chorus/gate-registry.md` | `chorus/docs/` | Gate definitions |
| `architect/chorus/communication-flows.md` | `chorus/docs/` | Sequence diagrams |
| `architect/chorus/chorus-activity-dashboard-summary.md` | `chorus/docs/` | Dashboard summary |
| `product-manager/chorus-overview.md` | `chorus/docs/` | Master reference |
| `messages/scripts/chorus-audit.sh` | `chorus/scripts/` | Fitness function runner |
| `messages/scripts/chorus-board.sh` | `chorus/scripts/` | Board wrapper |
| `messages/scripts/chorus-log.sh` | `chorus/scripts/` | Event emitter |
| `shared-observability/dashboards/chorus-activity.json` | `chorus/observability/` | Grafana dashboard |
| `shared-observability/config/.../chorus-alerts.yaml` | `chorus/observability/` | Alert rules |
| `architect/ontology/building.ttl` | `chorus/ontology/` | Protocol ontology |

### What Stays

| Location | Reason |
|----------|--------|
| `messages/team-architecture.md` | Foundation doc — shared by all, not Chorus-specific |
| `messages/scripts/board.sh` | Gathering board wrapper — stays |
| `messages/scripts/slack-*.sh` | Shared team tools |
| `messages/scripts/team-scan.sh` | Shared session hook |
| `messages/slack-bridge/` | Shared signal bus |
| `messages/logs/chorus.log` | Stays — Promtail scrapes from here |
| `engineer/.claude/hooks/infra-guardrails.sh` | Stays — Kade-session-specific |

### Proposed Structure

```
chorus/
├── CLAUDE.md              # Chorus-specific instructions (new)
├── docs/
│   ├── chorus-overview.md
│   ├── gate-registry.md
│   ├── communication-flows.md
│   └── dashboard-summary.md
├── scripts/
│   ├── chorus-audit.sh
│   ├── chorus-board.sh
│   └── chorus-log.sh
├── ontology/
│   └── building.ttl
├── observability/
│   ├── chorus-activity.json
│   └── chorus-alerts.yaml
└── briefs/                # Chorus-specific briefs (new)
```

### Script Path Updates

Every script that currently lives at `messages/scripts/chorus-*.sh` is referenced in:
- All 3 role `settings.local.json` files (chorus-audit.sh in SessionStart hook)
- CLAUDE.md files (chorus-board.sh usage instructions)
- The git post-commit hook (chorus-log.sh)

**Option A**: Move scripts, update all paths
**Option B**: Move scripts, leave symlinks at old locations
**Recommendation**: Option B — symlinks prevent breaking anything during transition

### Effort: ~2 hours
- Create directory structure
- Move files
- Create symlinks
- Update CLAUDE.md references
- Test chorus-audit.sh still runs from hooks

---

## Change 2: Rename jeff-bridwell-personal-site → gathering (Higher Risk)

### Blast Radius

| Category | Files | Risk | Notes |
|----------|-------|------|-------|
| **Docker containers** | 3 containers, 2 volumes, 1 network | HIGH | All named `jeff-bridwell-personal-site-*` |
| **Terraform variables** | 4 files | HIGH | `project_name` default drives all naming |
| **app-state.sh** | 2 copies, 20+ references | HIGH | Hardcoded container names throughout |
| **SOLID auth** | 1 client registration | HIGH | `SOLID_CLIENT_NAME=jeff-bridwell-personal-site` |
| **TLS certificates** | 2 cert generation scripts | HIGH | DNS SAN entries |
| **Prometheus config** | 3 files | MEDIUM | Container hostname references in scrape targets |
| **Grafana datasources** | 2 files | MEDIUM | Loki/Prometheus URLs use container names |
| **package.json** | 1 file | LOW | NPM package name |
| **Documentation** | 14+ files across all roles | LOW | Path and container name references |
| **GitHub repo** | 1 remote | MEDIUM | Rename on GitHub + update all local remotes |
| **.env / .env.sample** | 4 absolute paths | MEDIUM | Filesystem paths |

### The Terraform Variable Shortcut

The good news: most Docker naming flows from ONE Terraform variable:

```terraform
# terraform/environments/dev/variables.tf
variable "project_name" {
  default = "jeff-bridwell-personal-site"  # Change THIS
}
```

Container names, volume names, and network names are all `${var.project_name}-*`. Changing the default propagates to most infrastructure automatically.

**But**: app-state.sh has ~20 hardcoded references that DON'T use the Terraform variable. Those need manual updating.

### Execution Sequence

**Phase 1: Prep (no downtime)**
1. Branch: `git checkout -b rename-to-gathering`
2. Update `variables.tf` defaults (4 files) → `"gathering"`
3. Update `app-state.sh` hardcoded references → `gathering-*`
4. Update `package.json` name → `"gathering"`
5. Update Prometheus scrape configs (container hostnames)
6. Update Grafana datasource configs
7. Update cert generation scripts (DNS SANs)
8. Update `.env.sample` paths

**Phase 2: Destructive (requires downtime)**
1. `app-state.sh stop` — bring everything down
2. Rename directory: `mv jeff-bridwell-personal-site gathering`
3. `terraform destroy` — clean old state
4. Regenerate TLS certs
5. `terraform apply` — create new containers with `gathering-*` names
6. Migrate volume data (Fuseki TDB2, node_modules) to new volume names
7. Verify SOLID auth still works (may need re-registration)
8. `app-state.sh status` — confirm everything healthy

**Phase 3: Documentation (no downtime)**
1. Update all 3 CLAUDE.md files
2. Update ADRs, briefs, architecture docs
3. Update `settings.local.json` hooks paths
4. Rename GitHub repo (if desired)
5. Update git remotes across all clones
6. Brief team on new paths

### Critical Risk: Volume Data Migration

Docker named volumes (`jeff-bridwell-personal-site-fuseki-data`, `-node-modules`) contain:
- **Fuseki TDB2**: All RDF triples (knowledge graph data)
- **node_modules**: Build artifacts (rebuildable)

Renaming means Terraform creates NEW empty volumes. Fuseki data must be migrated:
```bash
# Before rename
docker cp jeff-bridwell-personal-site-fuseki:/fuseki/databases ./fuseki-backup/

# After rename, new containers up
docker cp ./fuseki-backup/ gathering-fuseki:/fuseki/databases
```

Or: keep old volume names as an override in Terraform (less clean, but zero-risk).

### Effort: ~4-6 hours (including downtime and verification)

---

## Recommended Sequencing

| Order | Change | When | Risk |
|-------|--------|------|------|
| 1 | **Chorus top-level** | After Kade lands current work | Low — file moves + symlinks |
| 2 | **Gathering rename** | Separate maintenance window | High — requires downtime, volume migration |

Do Chorus first — it's low risk, independent, and gives us the structural separation immediately. The rename is bigger surgery and should be its own focused effort.

---

## Cards Needed

- **C#5** (existing): "Elevate building/ to top-level project" → expand scope to "Create /chorus/ top-level with scripts, docs, observability, ontology"
- **New Gathering card**: "Rename jeff-bridwell-personal-site → gathering" — P2, assigned to Kade + Silas pair

---

— Silas
