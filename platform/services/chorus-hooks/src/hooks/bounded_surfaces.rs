//! #3424 — the bounded-surface guard: werk verbs and role skills are a
//! DECLARED set, not whatever happens to exist on disk.
//!
//! Werk-simplify's premise is that the pipeline stays comprehensible because
//! its surface is bounded. Nothing enforced that: today there are 13 werk-*
//! crates and 46 skills, and every one of them arrived by someone adding a
//! directory. No single addition was wrong; the absence of a moment where
//! anyone had to decide is what let the set drift.
//!
//! So the bound lives in `platform/config/canonical-surfaces.json` and this
//! guard reads it. Widening stays cheap and legitimate — edit the list, and
//! the diff shows a human chose to. What is refused is the accidental path:
//! a new verb crate or skill appearing beside the others without that edit.
//!
//! Deliberately narrow: it fires only on paths that CREATE a surface
//! (`platform/services/werk-*/…`, `skills/<name>/…`) and is silent
//! everywhere else. A guard that nags on unrelated edits gets disabled, and
//! a disabled guard bounds nothing.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSurfaces {
    pub verbs: BTreeSet<String>,
    pub skills: BTreeSet<String>,
}

/// Parse the canonical list. A list that does not parse, or that is missing
/// either axis, is an ERROR — never an empty set that would silently permit
/// everything (#3734: the guarded condition must be able to go red).
pub fn parse_surfaces(json: &str) -> Result<CanonicalSurfaces, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("canonical-surfaces.json does not parse: {e}"))?;
    let take = |key: &str| -> Result<BTreeSet<String>, String> {
        let arr = v
            .get(key)
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("canonical-surfaces.json has no '{key}' array — refusing to guard with half a list"))?;
        let set: BTreeSet<String> = arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
        if set.is_empty() {
            return Err(format!("canonical-surfaces.json '{key}' is empty — an empty bound bounds nothing"));
        }
        Ok(set)
    };
    Ok(CanonicalSurfaces { verbs: take("verbs")?, skills: take("skills")? })
}

/// The surface a path would create, if any.
fn surface_of(path: &str) -> Option<(&'static str, String)> {
    let p = path.trim_start_matches("./");
    if let Some(rest) = p.strip_prefix("platform/services/") {
        let name = rest.split('/').next().unwrap_or_default();
        if name.starts_with("werk-") {
            return Some(("verb", name.to_string()));
        }
        return None;
    }
    if let Some(rest) = p.strip_prefix("skills/") {
        let name = rest.split('/').next().unwrap_or_default();
        // `skills/foo.bats` and other loose files are not a skill directory.
        if !name.is_empty() && !name.contains('.') {
            return Some(("skill", name.to_string()));
        }
    }
    None
}

/// Some(refusal) when this path introduces a surface outside the declared set.
/// None when the path is on-list, unrelated, or the list file itself (widening
/// must never be blocked by the guard that demands it).
pub fn check_path(set: &CanonicalSurfaces, path: &str) -> Option<String> {
    if path.ends_with("platform/config/canonical-surfaces.json") {
        return None;
    }
    let (kind, name) = surface_of(path)?;
    let known = match kind {
        "verb" => set.verbs.contains(&name),
        _ => set.skills.contains(&name),
    };
    if known {
        return None;
    }
    Some(format!(
        "BOUNDED SURFACE (#3424): '{name}' is a new {kind} outside the canonical set. \
         The pipeline's surface is declared, not discovered — if this {kind} should exist, \
         add it to platform/config/canonical-surfaces.json in the same change, so the diff \
         shows the set widening on purpose."
    ))
}

/// True when this path is one the guard governs (a verb crate or a skill
/// directory) — used to fail loud only where the bound applies.
pub fn governs(path: &str) -> bool {
    surface_of(path).is_some()
}

/// Read the list from a repo root. Err carries the loud reason.
pub fn load_surfaces(repo_root: &str) -> Result<CanonicalSurfaces, String> {
    let path = format!("{repo_root}/platform/config/canonical-surfaces.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("canonical-surfaces.json unreadable at {path}: {e}"))?;
    parse_surfaces(&raw)
}
