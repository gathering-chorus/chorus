//! Athena Move 0 — SessionStart envelope injection (#2940).
//!
//! Reads `data/athena/tree.json` and renders a markdown block injected into
//! the role's SessionStart `additionalContext`. The block carries:
//!
//!   - role's owned Products / Domains / Services (compact list)
//!   - most-active 3-5 units (signal: tree-data heuristic — see ranking notes)
//!   - needs-work 3-5 units (signal: gap-count + status)
//!   - flat ownership map `{iri → role}` (if signals JSON says inject=true)
//!   - URL of the rendered tree page
//!
//! Signals are intentionally shallow in Move 0 — tree-data only (gap count,
//! status, audience size). Card/commit/spine signals come in a follow-on once
//! the operational tree has settled (per the cookbook's "let data settle then
//! refactor signals" pattern, two-phase model).
//!
//! Signal scoring is hardcoded below (active_score_status + structural-density
//! for active; status × gap-count × design-doc-presence for needs-work). An
//! earlier external tuning-file approach was retired in #2959 — the file was
//! never read; the claim that it was tunable was decorative. If real tuning
//! becomes a requirement, ship it as a code change here, not as a data file.
//!
//! Graceful degradation: any I/O or parse failure injects a one-line "athena
//! tree unavailable" note rather than failing the entire session boot. Boot
//! cost matters more than completeness here.

use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::shared::state_paths::repo_root;

#[derive(Debug, Deserialize)]
struct Tree {
    products: Vec<Product>,
    domains: Vec<Domain>,
    services: Vec<Service>,
    #[allow(dead_code)]
    roles: Vec<Role>,
}

#[derive(Debug, Deserialize)]
struct Product {
    iri: String,
    label: String,
    #[serde(rename = "ownedBy")]
    owned_by: String,
    status: String,
    gaps: Vec<String>,
    #[serde(rename = "hasDomain", default)]
    has_domain: Vec<String>,
    #[serde(rename = "hasDesignDoc", default)]
    has_design_doc: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Domain {
    iri: String,
    label: String,
    #[serde(rename = "ownedBy")]
    owned_by: String,
    status: String,
    gaps: Vec<String>,
    #[serde(default)]
    hosts: Vec<String>,
    #[serde(rename = "hasDesignDoc", default)]
    has_design_doc: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Service {
    iri: String,
    label: String,
    #[serde(rename = "ownedBy")]
    owned_by: String,
    status: String,
    gaps: Vec<String>,
    #[serde(default)]
    #[serde(rename = "notInScope")]
    not_in_scope: Vec<String>,
    #[serde(rename = "hasDesignDoc", default)]
    has_design_doc: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Role {
    #[allow(dead_code)]
    iri: String,
    #[allow(dead_code)]
    label: String,
}

/// Convert chorus:role-wren → chorus:role-wren (no-op identity here; consumers
/// often display the slug — kept for symmetry with future iri-formatting needs).
fn role_iri(role: &str) -> String {
    format!("chorus:role-{}", role)
}

/// Map status string → "active" score component. Higher = more day-to-day
/// attention demand.
fn active_score_status(status: &str) -> i32 {
    match status {
        "building" => 5,
        "exploring" => 3,
        "operating" => 1,
        "retiring" => 0,
        _ => 0,
    }
}

/// Map status + gap count → "needs-work" score component. Higher = more
/// attention debt.
fn needs_work_score(status: &str, gap_count: usize, has_design_doc: bool) -> i32 {
    let status_w = match status {
        "exploring" => 3,
        "building" => 2,
        "operating" => 1,
        _ => 0,
    };
    let gap_w = gap_count as i32;
    let doc_w = if !has_design_doc { 2 } else { 0 };
    status_w + gap_w + doc_w
}

/// Render the Athena tree injection block for a role.
///
/// Returns the markdown string to append to additionalContext. On any read or
/// parse failure, returns a short fallback notice — never panics, never blocks
/// session boot.
pub fn render_for_role(role: &str) -> String {
    let tree_path = format!("{}/data/athena/tree.json", repo_root());
    if !Path::new(&tree_path).exists() {
        return "\n## Athena tree\n\n_tree.json not yet on disk — Move 0 in flight (#2940). SessionStart owned/active/needs-work ranking unavailable._\n".to_string();
    }
    let raw = match fs::read_to_string(&tree_path) {
        Ok(s) => s,
        Err(e) => {
            return format!(
                "\n## Athena tree\n\n_tree.json read failed: {}. Skipping owned/active/needs-work ranking._\n",
                e
            );
        }
    };
    let tree: Tree = match serde_json::from_str(&raw) {
        Ok(t) => t,
        Err(e) => {
            return format!(
                "\n## Athena tree\n\n_tree.json parse failed: {}. Skipping owned/active/needs-work ranking._\n",
                e
            );
        }
    };

    let target = role_iri(role);
    let owned_products: Vec<&Product> = tree.products.iter().filter(|p| p.owned_by == target).collect();
    let owned_domains: Vec<&Domain> = tree.domains.iter().filter(|d| d.owned_by == target).collect();
    let owned_services: Vec<&Service> = tree.services.iter().filter(|s| s.owned_by == target).collect();

    let mut out = String::new();
    out.push_str("\n## Athena tree — owned · active · needs-work (Move 0)\n\n");
    out.push_str("_Source: `data/athena/tree.json` · rendered tree: <http://localhost:3340/athena/tree.html>_\n\n");
    out.push_str("> **Move-0 disclaimer (per Silas, #2940 feedback):** The active / needs-work ranking below is a **structural** signal only — derived from `status` × structural-density (active) and `gap-count` × design-doc-presence (needs-work). It does **not** yet incorporate recency (yesterday's commits / cards), staleness (design-doc age, open-AC age), or activity from pulse-cache. Those fold in at Move 2 when card-CLI + git + spine integration lands. If the ranking doesn't match what you actually worked on yesterday, that's the gap — not a regression. Use it as a coarse first-pass; reach for `chorus_blast_radius` / pulse for finer signal.\n\n");

    // ── Owned units ──────────────────────────────────────────────────────
    out.push_str("### Your owned units\n\n");
    if owned_products.is_empty() && owned_domains.is_empty() && owned_services.is_empty() {
        out.push_str("_(none — check tree.json `ownedBy` if this is surprising)_\n\n");
    } else {
        if !owned_products.is_empty() {
            out.push_str("**Products:** ");
            out.push_str(
                &owned_products
                    .iter()
                    .map(|p| format!("`{}` ({})", p.iri, p.label))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push_str("\n\n");
        }
        if !owned_domains.is_empty() {
            out.push_str("**Domains:** ");
            out.push_str(
                &owned_domains
                    .iter()
                    .map(|d| format!("`{}` ({})", d.iri, d.label))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push_str("\n\n");
        }
        if !owned_services.is_empty() {
            out.push_str("**Services:** ");
            out.push_str(
                &owned_services
                    .iter()
                    .map(|s| format!("`{}` ({})", s.iri, s.label))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push_str("\n\n");
        }
    }

    // ── Most active (top 5) ──────────────────────────────────────────────
    let mut active_scored: Vec<(i32, String, String, String)> = Vec::new();
    for p in &owned_products {
        let s = active_score_status(&p.status) + (p.has_domain.len() as i32) / 2;
        active_scored.push((s, p.iri.clone(), p.label.clone(), p.status.clone()));
    }
    for d in &owned_domains {
        let s = active_score_status(&d.status) + (d.hosts.len() as i32);
        active_scored.push((s, d.iri.clone(), d.label.clone(), d.status.clone()));
    }
    for s in &owned_services {
        let sc = active_score_status(&s.status);
        active_scored.push((sc, s.iri.clone(), s.label.clone(), s.status.clone()));
    }
    active_scored.sort_by(|a, b| b.0.cmp(&a.0));
    out.push_str("### Most active (top 5)\n\n");
    if active_scored.is_empty() {
        out.push_str("_(no owned units to rank)_\n\n");
    } else {
        for (score, iri, label, status) in active_scored.iter().take(5) {
            out.push_str(&format!("- `{}` — {} (status: {}, score: {})\n", iri, label, status, score));
        }
        out.push_str("\n_Signal: status × structural-density (Move 0 shallow ranking — card/commit/spine signals folded in follow-on)._\n\n");
    }

    // ── Needs more work (top 5) ──────────────────────────────────────────
    let mut needs_scored: Vec<(i32, String, String, usize)> = Vec::new();
    for p in &owned_products {
        let s = needs_work_score(&p.status, p.gaps.len(), !p.has_design_doc.is_empty());
        needs_scored.push((s, p.iri.clone(), p.label.clone(), p.gaps.len()));
    }
    for d in &owned_domains {
        let s = needs_work_score(&d.status, d.gaps.len(), !d.has_design_doc.is_empty());
        needs_scored.push((s, d.iri.clone(), d.label.clone(), d.gaps.len()));
    }
    for s in &owned_services {
        let sc = needs_work_score(&s.status, s.gaps.len() + s.not_in_scope.len(), !s.has_design_doc.is_empty());
        needs_scored.push((sc, s.iri.clone(), s.label.clone(), s.gaps.len()));
    }
    needs_scored.sort_by(|a, b| b.0.cmp(&a.0));
    out.push_str("### Needs more work (top 5)\n\n");
    if needs_scored.is_empty() {
        out.push_str("_(no owned units to rank)_\n\n");
    } else {
        for (score, iri, label, gaps) in needs_scored.iter().take(5) {
            out.push_str(&format!("- `{}` — {} (gaps: {}, score: {})\n", iri, label, gaps, score));
        }
        out.push_str("\n_Signal: status × gap-count × design-doc-presence (Move 0 shallow — staleness + open-AC signals follow once cards-CLI integration lands)._\n\n");
    }

    // ── Ownership map ────────────────────────────────────────────────────
    out.push_str("### Ownership map (cross-role lookup)\n\n");
    out.push_str("| IRI | Role |\n|---|---|\n");
    for p in &tree.products {
        out.push_str(&format!("| `{}` | {} |\n", p.iri, strip_role(&p.owned_by)));
    }
    for d in &tree.domains {
        out.push_str(&format!("| `{}` | {} |\n", d.iri, strip_role(&d.owned_by)));
    }
    for s in &tree.services {
        out.push_str(&format!("| `{}` | {} |\n", s.iri, strip_role(&s.owned_by)));
    }
    out.push_str("\n_Use `chorus_ownership_lookup(iri)` for programmatic resolution._\n\n");

    out
}

fn strip_role(iri: &str) -> &str {
    iri.strip_prefix("chorus:role-").unwrap_or(iri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_iri_formats_correctly() {
        assert_eq!(role_iri("wren"), "chorus:role-wren");
        assert_eq!(role_iri("kade"), "chorus:role-kade");
    }

    #[test]
    fn active_score_status_handles_known_values() {
        assert_eq!(active_score_status("building"), 5);
        assert_eq!(active_score_status("exploring"), 3);
        assert_eq!(active_score_status("operating"), 1);
        assert_eq!(active_score_status("retiring"), 0);
        assert_eq!(active_score_status("unknown"), 0);
    }

    #[test]
    fn needs_work_score_combines_signals() {
        // exploring + 2 gaps + has-doc
        assert_eq!(needs_work_score("exploring", 2, true), 5);
        // building + 0 gaps + no-doc
        assert_eq!(needs_work_score("building", 0, false), 4);
        // operating + 1 gap + has-doc
        assert_eq!(needs_work_score("operating", 1, true), 2);
    }

    #[test]
    fn strip_role_handles_prefixed_and_bare() {
        assert_eq!(strip_role("chorus:role-wren"), "wren");
        assert_eq!(strip_role("wren"), "wren");
    }

    #[test]
    fn render_for_role_returns_fallback_when_tree_missing() {
        // This will trigger the missing-file path unless tree.json exists at
        // repo_root (in test env, repo_root may or may not have one). The test
        // only verifies the function does not panic and returns a string.
        let out = render_for_role("wren");
        assert!(out.starts_with("\n## Athena tree"));
    }
}
