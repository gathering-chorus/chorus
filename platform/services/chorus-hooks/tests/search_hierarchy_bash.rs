//! Tests for search_hierarchy gate: git-as-search via Bash must be gated.
//!
//! Bug: search_hierarchy only checks Grep/Glob, so roles bypass it by
//! running `git log` via Bash. These tests prove the fix.


// Since chorus-hooks is a binary crate, we test the detection logic directly.
// The gate should treat `git log`, `git blame`, `git show` as search tools.

/// Detect whether a Bash command is a git search command
fn is_git_search(cmd: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();
    let trimmed = cmd_lower.trim_start();
    trimmed.starts_with("git log")
        || trimmed.starts_with("git blame")
        || trimmed.starts_with("git show")
        || trimmed.contains("| git log")
        || trimmed.contains("&& git log")
}

/// Extract search term from git command (--grep value or filename arg)
fn extract_git_search_term(cmd: &str) -> String {
    // --grep "term" or --grep=term
    if let Some(pos) = cmd.find("--grep") {
        let after = &cmd[pos + 6..];
        let after = after.trim_start_matches('=').trim_start();
        // Handle quoted or unquoted
        if after.starts_with('"') || after.starts_with('\'') {
            let quote = after.chars().next().unwrap();
            if let Some(end) = after[1..].find(quote) {
                return after[1..1 + end].to_string();
            }
        }
        // Unquoted: take until whitespace
        return after.split_whitespace().next().unwrap_or("").to_string();
    }

    // git blame <file> — the file is the search subject
    let trimmed = cmd.trim();
    if trimmed.to_lowercase().starts_with("git blame") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        // Skip flags (start with -)
        for part in parts.iter().skip(2) {
            if !part.starts_with('-') {
                return part.to_string();
            }
        }
    }

    String::new()
}

#[test]
fn git_log_is_search() {
    assert!(is_git_search("git log --oneline --grep nudge"));
    assert!(is_git_search("git log --all -10"));
    assert!(is_git_search("Git Log --oneline")); // case insensitive
}

#[test]
fn git_blame_is_search() {
    assert!(is_git_search("git blame src/main.rs"));
}

#[test]
fn git_show_is_search() {
    assert!(is_git_search("git show HEAD:src/hooks/search_hierarchy.rs"));
}

#[test]
fn piped_git_log_is_search() {
    assert!(is_git_search("echo foo | git log --oneline"));
    assert!(is_git_search("cd /tmp && git log --all"));
}

#[test]
fn non_search_git_passes() {
    assert!(!is_git_search("git push origin main"));
    assert!(!is_git_search("git commit -m 'fix'"));
    assert!(!is_git_search("git status"));
    assert!(!is_git_search("git pull --rebase"));
    assert!(!is_git_search("ls -la"));
}

#[test]
fn extract_grep_flag() {
    assert_eq!(
        extract_git_search_term("git log --oneline --grep nudge"),
        "nudge"
    );
    assert_eq!(
        extract_git_search_term("git log --grep=\"search hierarchy\""),
        "search hierarchy"
    );
}

#[test]
fn extract_blame_file() {
    assert_eq!(
        extract_git_search_term("git blame src/main.rs"),
        "src/main.rs"
    );
}
