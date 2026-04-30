use crate::state::chorus_log;
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

static WRONG_DATASET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"localhost:3030/(ds|jeff|dataset|sparql)/").unwrap()
});

static SPARQL_KEYWORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(SELECT|CONSTRUCT|ASK|COUNT)").unwrap()
});

static GRAPH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)GRAPH").unwrap()
});

static POST_SPARQL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"POST.*3030/pods/sparql").unwrap()
});

static CARDS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Card|board|task|vikunja|bucket|kanban").unwrap()
});

static ROLE_STATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)role.*state|declared\.json|role.*building|role.*waiting|role.*idle").unwrap()
});

static BOARD_TS_JQ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"cards.*(list|view|mine).*jq").unwrap()
});

pub async fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let cmd = input.get_tool_input_str("command");
    if cmd.is_empty() || !cmd.contains("3030") {
        return HookResponse::allow();
    }

    let mut warnings = Vec::new();

    if WRONG_DATASET_RE.is_match(&cmd) {
        warnings.push("Wrong dataset — use /pods not /ds or /jeff. Correct: http://localhost:3030/pods/query");
    }

    if SPARQL_KEYWORD_RE.is_match(&cmd) && !GRAPH_RE.is_match(&cmd) {
        warnings.push("No GRAPH clause — bare patterns hit the empty default graph. Use: GRAPH ?g { ... } FILTER(STRSTARTS(STR(?g), \"http://localhost:3000/pods/jeff/\"))");
    }

    if POST_SPARQL_RE.is_match(&cmd) {
        warnings.push("POST to /pods/sparql returns 405. Use GET to /pods/query instead.");
    }

    if CARDS_RE.is_match(&cmd) {
        warnings.push("DEC-093: Cards data has an API endpoint. Use: GET /api/chorus/cards (with ?owner=X&status=Y filters) instead of raw SPARQL.");
    }

    if ROLE_STATE_RE.is_match(&cmd) {
        warnings.push("DEC-093: Role state has an API endpoint. Use: GET /api/chorus/roles or /api/chorus/roles/:id/state instead of raw SPARQL or file reads.");
    }

    if BOARD_TS_JQ_RE.is_match(&cmd) {
        warnings.push("DEC-093: Parsing cards output? Use: GET /api/chorus/cards for structured JSON instead.");
    }

    if !warnings.is_empty() {
        let msg = warnings
            .iter()
            .map(|w| format!("\u{26a0} SPARQL guard: {}", w))
            .collect::<Vec<_>>()
            .join("\n");

        // Log to chorus (fire-and-forget)
        tokio::spawn(async move {
            chorus_log(
                "guard.sparql.warned",
                "system",
                &[("warning", "sparql-pattern-match")],
            )
            .await;
        });

        return HookResponse::warn_stderr(&msg);
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use crate::shared::state_paths::chorus_root;
    use serde_json::json;

    fn make_input(tool: &str) -> HookInput {
        HookInput {
            tool_name: Some(tool.to_string()),
            tool_input: Some(json!({"command": "echo test", "file_path": "/tmp/test", "skill": "demo"})),
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some(format!("{}/architect", chorus_root())),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: Some("silas".to_string()),
            chorus_worktree_override: None,}
    }

    #[tokio::test]
    async fn allows_non_matching_tool() {
        let input = make_input("Read");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_normal_input() {
        let input = make_input("Bash");
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
