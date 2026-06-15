#!/usr/bin/env python3
"""
l2-rules-seed.py — #3426: seed Domain.hasMapsTo in tree.json with the
reconstructed file→domain prefix rules (the 06-09 L2 judgment, recovered;
high+medium confidence only — low-confidence guesses dropped to keep coverage
honest). Applies Jeff's 2026-06-14 calls: roles≠memory (no blanket
roles/→memory; subtree split), streams=Clearing-pane code, time=launchagents,
practices=data-domain (left thin/uncovered, honest).

Idempotent: replaces each domain's hasMapsTo with the seeded list. Re-run safe.
Owner-review follow-ons (NOT merged — the "while"): platform/scripts tail
(151 files), chorus-hooks file-vs-instance domain, builds/deploys as runtime
domains, the roles/memory split (Wren).
"""
import json, os, sys

TREE = os.environ.get("TREE", os.path.join(
    os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"),
    "data/athena/tree.json"))

RULES = {
  "memory": ["roles/silas/chorus", "roles/wren/chunks",
             "platform/api/src/index-worker.ts", "platform/api/src/index-all-sources.ts",
             "platform/api/src/index-all-sources-deps.ts", "platform/api/src/session-replay.ts",
             "platform/api/src/handlers/sessions.ts", "platform/tests/features/memory"],
  "roles": ["roles", "designing/claudemd", "platform/api/src/handlers/context-roles.ts",
            "platform/api/src/derive-role-state.ts"],
  "messages": ["roles/silas/briefs", "roles/silas/briefs-archive", "roles/wren/briefs",
               "roles/wren/briefs-archive", "roles/kade/briefs", "roles/kade/briefs-archive",
               "roles/architect/briefs", "roles/product-manager/briefs", "product-manager/briefs",
               "platform/api/src/handlers/chorus-conversation.ts"],
  "decisions": ["designing/decisions", "roles/wren/decisions", "roles/silas/adr",
                "platform/api/src/handlers/loom-decisions.ts", "platform/api/src/seed-loom-decisions.ts"],
  "knowledge": ["designing", "knowledge", "docs", "platform/api/public/book",
                "platform/api/src/handlers/doc-catalog.ts", "platform/api/src/handlers/doc-catalog-tree.ts",
                "platform/api/src/handlers/doc-tagger.ts", "platform/api/src/handlers/doc-tag-drift.ts",
                "platform/api/src/handlers/catalog-curation.ts"],
  "domains": ["designing/domains", "designing/ontology", "designing/schemas", "data/athena",
              "roles/silas/ontology", "platform/api/src/handlers/athena-tree.ts",
              "platform/api/src/handlers/athena-tree-schemas.ts", "platform/api/src/handlers/athena-subdomains.ts",
              "platform/api/src/handlers/athena-products.ts", "platform/api/src/handlers/athena-subproducts.ts",
              "platform/api/src/handlers/domain-identity.ts", "platform/api/src/handlers/domain-facets.ts",
              "platform/api/public/athena", "platform/services/owl-api", "platform/services/chorus-model"],
  "services": ["platform/api/src/handlers/chorus-services.ts", "platform/api/src/handlers/athena-owners.ts",
               "platform/api/src/handlers/athena-owner-write.ts"],
  "integrations": ["building/products/convergence", "platform/api/src/handlers/chorus-harvest.ts"],
  "search": ["platform/api/src/handlers/chorus-search.ts", "platform/api/src/search-worker.ts",
             "platform/api/src/search-worker-core.ts", "platform/api/src/search-rrf.ts",
             "platform/api/src/search-meta.ts", "platform/api/src/search-fusion.ts",
             "platform/api/src/fts-worker.ts", "platform/api/src/fts-worker-pool.ts",
             "platform/api/src/fts-worker-core.ts", "platform/api/src/embed-query.ts",
             "platform/api/src/embed-delta.ts", "platform/api/src/embed-delta-worker.ts",
             "platform/api/src/embed-delta-deps.ts", "platform/api/src/embed-floor.ts",
             "platform/api/src/lance-store.ts", "platform/api/src/lance-maintain-worker.ts"],
  "cards": ["directing/products/cards", "platform/api/src/handlers/athena-card-detail.ts",
            "platform/api/src/handlers/athena-subdomain-cards.ts", "platform/api/src/handlers/context-board-next.ts",
            "platform/api/src/handlers/context-board-wip.ts", "platform/api/src/handlers/context-board-swat.ts",
            "platform/api/src/board-cache.ts", "platform/api/src/cards-path.ts",
            "platform/api/src/handlers/chorus-card-story.ts"],
  "spine": ["platform/api/src/handlers/context-spine.ts", "platform/api/src/spine-event-write.ts",
            "platform/api/src/handlers/chorus-trace.ts", "designing/schemas/spine-events.json",
            "roles/silas/spine-architecture.md", "roles/silas/spine-emitter-inventory.md"],
  "streams": ["directing/clearing/src/tailer.ts", "directing/clearing/src/session-tailer.ts",
              "directing/clearing/src/transcript.ts"],
  "skills": ["skills", "platform/skills", "roles/wren/.claude/skills", "roles/silas/.claude/skills",
             "roles/kade/.claude/skills", "building/skill"],
  "principles": ["platform/api/src/handlers/loom-principles.ts"],
  "policies": ["platform/api/src/handlers/loom-policies.ts", "platform/services/chorus-hooks/src/hooks"],
  "rcas": ["platform/api/src/handlers/chorus-rcas.ts"],
  "pipelines": ["proving/workflows", "platform/workflows", "platform/workflow-engine"],
  "cicd": ["designing/claudemd/pipeline-runs", ".github/workflows",
           "platform/scripts/gate-spine-vikunja-bridge.sh"],
  "version-control": ["platform/services/werk-commit", "platform/services/werk-push",
                      "platform/services/werk-pull", "platform/services/werk-merge",
                      "platform/services/werk-sync", "platform/services/werk-unpull",
                      "platform/services/werk-review", "platform/services/werk-build",
                      "platform/services/werk-accept", "platform/services/werk-demo",
                      "platform/services/werk-deploy", "platform/hooks"],
  "code": ["platform/api/src/handlers/chorus-domain-code.ts", "platform/api/src/discover-code.ts",
           "platform/api/src/handlers/codebase-topology.ts", "platform/scripts/mcp-tool-description-lint.js",
           "eslint.config.js"],
  "tests": ["platform/tests", "platform/api/tests", "directing/clearing/tests", "proving/scripts/tests",
            "platform/api/src/handlers/chorus-tests.ts", "platform/api/src/discover-tests.ts"],
  "logs": ["platform/api/src/handlers/logs-query.ts", "platform/logs", "roles/silas/logging-strategy.html"],
  "properties": ["platform/config"],
  "alerts-monitors": ["proving/domains/alerts", "dashboards", "platform/api/public/borg",
                      "platform/api/src/handlers/context-alerts.ts",
                      "platform/api/src/handlers/athena-subdomain-alerts.ts",
                      "platform/api/src/eventloop-alert.ts"],
  "security-trust": ["platform/services/chorus-hooks/src/hooks/write_scrubber.rs",
                     "platform/services/chorus-hooks/src/hooks/sensitive_paths.rs",
                     "roles/wren/security-trust-model.md"],
  "time": ["config/launchagents", "platform/launchagents", "platform/launchd",
           "proving/config/launchagents", "platform/scripts/launchagents-canonical",
           "platform/scripts/launchagents-secondary", "platform/services/pair-heartbeat"],
  "heralds": ["platform/services/chorus-inject"],
  "infrastructure": ["platform/apps", "roles/silas/infrastructure-constraints.md"],
  "analytics": ["platform/api/src/handlers/chorus-attention-analytics.ts",
                "platform/api/src/handlers/chorus-voice-analytics.ts",
                "platform/api/src/handlers/chorus-reprompt-analytics.ts"],
  "metrics": ["platform/api/src/metrics.ts", "platform/api/src/handlers/chorus-perf.ts",
              "platform/api/src/handlers/chorus-cost.ts", "designing/schemas/metrics-manifest.json"],
}

def main():
    d = json.load(open(TREE))
    by_label = {dom.get("label"): dom for dom in d.get("domains", [])}
    applied = 0
    missing = []
    for label, prefixes in RULES.items():
        dom = by_label.get(label)
        if not dom:
            missing.append(label); continue
        dom["hasMapsTo"] = prefixes
        applied += len(prefixes)
    json.dump(d, open(TREE, "w"), indent=2)
    print(f"seeded hasMapsTo: {len(RULES)} domains, {applied} prefixes -> {TREE}")
    if missing: print(f"WARN: labels not found in tree: {missing}")

if __name__ == "__main__":
    main()
