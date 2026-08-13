// #3842 — repo-aware werk verbs: the ONE resolver for a card's target repo.
// Source-included (like failure_class.rs / scope_units.rs) into werk-pull,
// werk-accept, and werk-unpull, so all verbs agree on what `repo:<name>`
// means. A card may declare its deliverable's repo with a `repo:<name>` label
// (in `cards --json` these arrive in the "domains" array). Default is `home`
// (the chorus repo). A named target resolves to a SIBLING of home under the
// same projects directory — the ADR-041 layout — and must already be a git
// repo. Every failure is a typed refusal; there is deliberately NO fallback
// to chorus (#3734): a card that says shared-security must never silently
// operate on the substrate.
//
// Expects `R<T>` (Result<T, String>) in the including crate's scope.

pub fn target_repo_root(home: &std::path::Path, card_json: &str) -> R<std::path::PathBuf> {
    let name = match card_json.find("\"repo:") {
        None => return Ok(home.to_path_buf()),
        Some(i) => {
            let after = &card_json[i + 6..];
            let end = after.find('"').ok_or("target-repo-invalid: unterminated label")?;
            after[..end].to_string()
        }
    };
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err(format!("target-repo-invalid: label 'repo:{}' must be a bare sibling-directory name", name));
    }
    let parent = home.parent().ok_or("target-repo-invalid: home has no parent directory")?;
    let root = parent.join(&name);
    if !root.is_dir() {
        return Err(format!("target-repo-missing: {} does not exist (label repo:{})", root.display(), name));
    }
    if !root.join(".git").exists() {
        return Err(format!("target-repo-not-git: {} exists but is not a git repository", root.display()));
    }
    Ok(root)
}
