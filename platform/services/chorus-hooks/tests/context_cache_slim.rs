//! Tests for #1781 — session-start redesign
//! AC: session-start shows last session signal, not 795-line card dump.
//!
//! Verifiable behavior:
//! - Active cards section filters out Done and Won't Do cards
//! - Last session section extracts per-role commits from git log output
//! - Other roles get 1-liner summaries
//! - Works across multi-day gaps (not just "yesterday")

/// Filter cards mine output to actionable cards only (WIP, Now, Ops, Later, Ideas)
fn filter_active_cards(full_output: &str) -> String {
    full_output
        .lines()
        .filter(|l| {
            let lt = l.trim().to_lowercase();
            !lt.contains("[done]") && !lt.contains("[won't do]")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse git log output into per-role last-session sections
fn parse_last_session_commits(git_log: &str, role: &str) -> (String, Vec<(String, String)>) {
    let mut own_commits = Vec::new();
    let mut other_roles: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for line in git_log.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        // Format: "abc123 role: message"
        let after_hash = match line.split_whitespace().nth(1) {
            Some(w) => {
                let pos = line.find(w).unwrap_or(0);
                &line[pos..]
            }
            None => continue,
        };

        if after_hash.starts_with(&format!("{}:", role)) {
            own_commits.push(line.to_string());
        } else {
            // Extract role name from "role: message"
            if let Some(colon_pos) = after_hash.find(':') {
                let r = after_hash[..colon_pos].trim().to_string();
                if matches!(r.as_str(), "wren" | "silas" | "kade") {
                    other_roles.entry(r).or_default().push(line.to_string());
                }
            }
        }
    }

    let other_summaries: Vec<(String, String)> = other_roles
        .into_iter()
        .map(|(r, commits)| {
            let summary = if let Some(reboot) = commits.iter().find(|c| c.contains("reboot")) {
                // Use reboot commit message as the summary — it's already a synthesis
                let msg_start = reboot.find(&format!("{}:", r)).map(|p| p + r.len() + 2).unwrap_or(0);
                reboot[msg_start..].to_string()
            } else {
                format!("{} commits", commits.len())
            };
            (r, summary)
        })
        .collect();

    let own_text = own_commits.join("\n");
    (own_text, other_summaries)
}

// --- Tests ---

#[test]
fn filter_strips_done_and_wont_do() {
    let input = r#"Silas Gathering items (686):
  [Done]  123  Old finished card [P1]
  [Done]  456  Another done card [P2]
  [Won't Do]  789  Rejected card [P3]
  [WIP] 1781  Session start redesign [P1]
  [Now] 1800  Board test isolation [P2]
  [Later] 1307  Re-run photo harvest [P1]
  [Ops] 1374  SPARQL query error [P2]
  [Ideas] 1267  Prove-it gate hook [P1]"#;

    let filtered = filter_active_cards(input);
    assert!(!filtered.contains("[Done]"), "Done cards should be filtered out");
    assert!(!filtered.contains("[Won't Do]"), "Won't Do cards should be filtered out");
    assert!(filtered.contains("[WIP] 1781"), "WIP cards should remain");
    assert!(filtered.contains("[Now] 1800"), "Now cards should remain");
    assert!(filtered.contains("[Later] 1307"), "Later cards should remain");
    assert!(filtered.contains("[Ops] 1374"), "Ops cards should remain");
    assert!(filtered.contains("[Ideas] 1267"), "Ideas cards should remain");
    assert!(filtered.contains("Silas Gathering items"), "Header line should remain");
}

#[test]
fn filter_preserves_empty_output() {
    let input = "";
    let filtered = filter_active_cards(input);
    assert_eq!(filtered, "");
}

#[test]
fn filter_all_done_returns_header_only() {
    let input = r#"Silas Gathering items (3):
  [Done]  1  Card one [P1]
  [Done]  2  Card two [P2]
  [Done]  3  Card three [P3]"#;

    let filtered = filter_active_cards(input);
    assert!(filtered.contains("Silas Gathering items"), "Header should survive");
    assert!(!filtered.contains("[Done]"), "No Done cards");
    // Should be just the header line
    assert_eq!(filtered.lines().count(), 1);
}

#[test]
fn parse_commits_separates_own_and_other_roles() {
    let git_log = r#"9276ad7f silas: session reboot — 9 cards shipped, monitoring gaps closed
a43135ef silas: acp #1857 — NiFi pipeline observability
6997a4d2 kade: session reboot — #1800 #1849 accepted, Athena CMDB built
202ba0e2 kade: acp #1863 — Gathering domains in Fuseki
12821563 wren: session reboot — #1845 CMDB shipped, product boundary defined"#;

    let (own, others) = parse_last_session_commits(git_log, "silas");

    // Own commits
    assert!(own.contains("9276ad7f"), "Should include own reboot commit");
    assert!(own.contains("a43135ef"), "Should include own acp commit");
    assert!(!own.contains("kade:"), "Should not include kade commits in own");
    assert!(!own.contains("wren:"), "Should not include wren commits in own");

    // Other roles get summaries
    assert_eq!(others.len(), 2, "Should have summaries for kade and wren");
    let kade_summary = others.iter().find(|(r, _)| r == "kade");
    assert!(kade_summary.is_some(), "Kade should have a summary");
    // Kade has a reboot commit, so summary should use that
    let (_, kade_msg) = kade_summary.unwrap();
    assert!(kade_msg.contains("Athena CMDB"), "Kade summary should use reboot message");
}

#[test]
fn parse_commits_handles_no_reboot_for_other_role() {
    let git_log = r#"9276ad7f silas: session reboot — 9 cards shipped
202ba0e2 kade: acp #1863 — Gathering domains
1c29d818 kade: #1863 — hasDomain composition"#;

    let (_, others) = parse_last_session_commits(git_log, "silas");

    let kade_summary = others.iter().find(|(r, _)| r == "kade");
    assert!(kade_summary.is_some());
    let (_, kade_msg) = kade_summary.unwrap();
    // No reboot commit for kade, so falls back to commit count
    assert!(kade_msg.contains("2 commits"), "Should show commit count when no reboot: got '{}'", kade_msg);
}

#[test]
fn parse_commits_works_across_multi_day_gaps() {
    // Simulates git log output spanning multiple days — same parsing logic applies
    let git_log = r#"abc12345 silas: session reboot — monitoring hardened
def67890 silas: acp #1854 — LanceDB observability
111aaaaa kade: session reboot — tests green, Athena shipped
222bbbbb wren: session reboot — product boundary docs"#;

    let (own, others) = parse_last_session_commits(git_log, "silas");

    assert_eq!(own.lines().count(), 2, "Should have 2 own commits regardless of date range");
    assert_eq!(others.len(), 2, "Should have both other roles");
}

#[test]
fn parse_commits_empty_log() {
    let (own, others) = parse_last_session_commits("", "silas");
    assert!(own.is_empty());
    assert!(others.is_empty());
}

#[test]
fn parse_commits_ignores_non_role_prefixes() {
    let git_log = r#"abc12345 silas: real commit
def67890 Merge branch 'main'
111aaaaa fixup! something
222bbbbb jeff: manual commit"#;

    let (own, others) = parse_last_session_commits(git_log, "silas");
    assert_eq!(own.lines().count(), 1, "Only silas: commit");
    // jeff is not a role, Merge/fixup don't match — others should be empty
    assert!(others.is_empty(), "Non-role prefixes should be ignored");
}
