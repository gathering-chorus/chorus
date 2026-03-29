use crate::state::{chorus_log, AppState};
use crate::types::{decision_allow_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

// === PreToolUse:AskUserQuestion patterns ===

struct PrefPattern {
    id: &'static str,
    regex: Regex,
    preference: &'static str,
    source: &'static str,
}

static PREF_PATTERNS: LazyLock<Vec<PrefPattern>> = LazyLock::new(|| {
    vec![
        PrefPattern {
            id: "P001",
            regex: Regex::new(r"(?i)should i (commit|push)|shall i (commit|push)|want me to (commit|push)").unwrap(),
            preference: "Yes. Commit and push when work is complete.",
            source: "DEC-025, DEC-058",
        },
        PrefPattern {
            id: "P002",
            regex: Regex::new(r"(?i)should i proceed|shall i (go ahead|continue|start)|want me to (proceed|go ahead|start)|ready to").unwrap(),
            preference: "Yes. If Jeff directed it, execute. Don't ask again.",
            source: "DEC-025, DEC-069",
        },
        PrefPattern {
            id: "P003",
            regex: Regex::new(r"(?i)should i card|want me to card|shall i create a card").unwrap(),
            preference: "Yes. Card it. No work without a card.",
            source: "DEC-033",
        },
        PrefPattern {
            id: "P004",
            regex: Regex::new(r"(?i)option (a|b|1|2)|which (do you|would you) prefer|which approach|which one").unwrap(),
            preference: "Don't present options you know the answer to. If predictable, just do it.",
            source: "DEC-069 rule 3",
        },
        PrefPattern {
            id: "P005",
            regex: Regex::new(r"(?i)should i deploy|want me to deploy|shall i deploy").unwrap(),
            preference: "Only deploy if src/ changed. Views/static are bind-mounted.",
            source: "CLAUDE.md",
        },
        PrefPattern {
            id: "P012",
            regex: Regex::new(r"(?i)should i (plan|create a plan|write a plan)|want me to plan").unwrap(),
            preference: "Only for horizontal work. Vertical: just execute.",
            source: "DEC-058",
        },
        PrefPattern {
            id: "P015",
            regex: Regex::new(r"(?i)here'?s (my|the) plan|here'?s what i'?m thinking|let me (outline|walk you through)").unwrap(),
            preference: "If it's your vertical, skip the pitch. Execute and report the outcome.",
            source: "DEC-058, DEC-069",
        },
    ]
});

// === Stop hook: permission-seeking patterns ===

static SEEKING_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)shall i (?:go ahead|proceed|start|do|create|build|update|make|write|run)").unwrap(),
        Regex::new(r"(?i)should i (?:go ahead|proceed|start|do|create|build|update|make|write|run)").unwrap(),
        Regex::new(r"(?i)would you like me to\b").unwrap(),
        Regex::new(r"(?i)want me to (?:go ahead|proceed|start|do|create|build|update|make|write|run)").unwrap(),
        Regex::new(r"(?i)do you want me to\b").unwrap(),
        Regex::new(r"(?i)ready to proceed\?").unwrap(),
        Regex::new(r"(?i)i can do (?:this|that).+(?:want|like|prefer)").unwrap(),
        Regex::new(r"(?i)here.s (?:what i.m thinking|my plan|the plan).+(?:\?|let me know|sound good)").unwrap(),
        Regex::new(r"(?i)i.ll (?:go ahead|proceed|start).+(?:if you|unless you)").unwrap(),
        Regex::new(r"(?i)(?:option [a-c]|two options|three options).+(?:which|what|prefer|resonate)").unwrap(),
    ]
});

static LEGIT_SIGNALS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"(?i)genuinely ambiguous").unwrap(),
        Regex::new(r"(?i)i don.t (?:know|have enough)").unwrap(),
        Regex::new(r"(?i)unclear whether").unwrap(),
        Regex::new(r"(?i)could go either way").unwrap(),
        Regex::new(r"(?i)trade-?off").unwrap(),
        Regex::new(r"(?i)risk.+(?:worth|accept)").unwrap(),
    ]
});

static JEFF_ASKED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(what |how |why |where |when |which |who |is |are |can |do (?:you|we|i\b)|does |should |could |would |will )").unwrap()
});

// Stop hook response-scan preference matchers
static STOP_PREF_MATCHERS: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    vec![
        ("P001", Regex::new(r"(?i)(?:should i|shall i|want me to|let me) (?:commit|push)").unwrap()),
        ("P002", Regex::new(r"(?i)(?:should i|shall i|want me to) (?:proceed|go ahead|continue|start)").unwrap()),
        ("P003", Regex::new(r"(?i)(?:should i|shall i|want me to) (?:card|create a card)").unwrap()),
        ("P004", Regex::new(r"(?i)(?:option [a-c]|two options|three options|which (?:approach|do you|would you|resonat))").unwrap()),
        ("P005", Regex::new(r"(?i)(?:should i|shall i|want me to) deploy").unwrap()),
        ("P006", Regex::new(r"(?i)(?:should i|shall i|want me to) update (?:state|docs|memory)").unwrap()),
        ("P007", Regex::new(r"(?i)(?:should i|shall i|want me to) (?:brief|send a brief|notify)").unwrap()),
        ("P012", Regex::new(r"(?i)(?:should i|shall i|want me to) (?:use plan|enter plan|plan mode)").unwrap()),
        ("P014", Regex::new(r"(?i)(?:should i|shall i|want me to) (?:pull|pick up) (?:the next|another)").unwrap()),
        ("P015", Regex::new(r"(?i)here.s (?:what i.m thinking|my plan|the plan)").unwrap()),
        ("P017", Regex::new(r"(?i)(?:should i|shall i|can i) mark.+done").unwrap()),
    ]
});

/// Detect trigger type from input
enum Trigger {
    UserPrompt,
    PreTool,
    Stop,
}

fn detect_trigger(input: &HookInput) -> Trigger {
    if input.stop_hook_active.is_some() || input.hook_type.as_deref() == Some("stop") {
        Trigger::Stop
    } else if input.prompt.is_some() && input.tool_name.is_none() {
        Trigger::UserPrompt
    } else {
        Trigger::PreTool
    }
}

pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    match detect_trigger(input) {
        Trigger::UserPrompt => handle_user_prompt(input, state).await,
        Trigger::PreTool => handle_pre_tool(input).await,
        Trigger::Stop => handle_stop(input, state).await,
    }
}

/// UserPromptSubmit — count "jdi" overrides, cache last human message
async fn handle_user_prompt(input: &HookInput, state: &AppState) -> HookResponse {
    let prompt = input.prompt.as_deref().unwrap_or("");
    let role = input.role();

    // Cache the human message for the Stop hook
    if let Some(sid) = &input.session_id {
        state.set_last_human_msg(sid, prompt.to_string()).await;
    }

    let prompt_lower = prompt.trim().to_lowercase();
    match prompt_lower.as_str() {
        "jdi" | "/jdi" | "just do it" | "just do it." | "just do it!" => {
            tokio::spawn(async move {
                chorus_log(
                    "decision.gate.jdi_override",
                    role.as_str(),
                    &[("hook", "autonomy-guard")],
                )
                .await;
            });
        }
        _ => {}
    }

    HookResponse::allow()
}

/// PreToolUse:AskUserQuestion — check preferences
async fn handle_pre_tool(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "AskUserQuestion" {
        return HookResponse::allow();
    }

    let question = input.get_tool_input_str("question");
    if question.is_empty() {
        return HookResponse::allow();
    }

    let role = input.role();
    let question_lower = question.to_lowercase();

    // Check preference patterns
    for pref in PREF_PATTERNS.iter() {
        if pref.regex.is_match(&question_lower) {
            let role_str = role.as_str().to_string();
            let pref_id = pref.id.to_string();
            let q_short: String = question.chars().take(80).collect();
            tokio::spawn(async move {
                chorus_log(
                    "decision.gate.matched",
                    &role_str,
                    &[
                        ("pref", &pref_id),
                        ("hook", "autonomy-guard"),
                        ("question", &q_short),
                    ],
                )
                .await;
            });

            let msg = format!(
                "[DEC-069] Autonomy guard: Jeff's known preference is: {} (source: {}, pref: {}). Don't ask — just do it.",
                pref.preference, pref.source, pref.id
            );
            return HookResponse::allow_with_message(&decision_allow_json(&msg));
        }
    }

    // No match — log pass
    let role_str = role.as_str().to_string();
    let q_short: String = question.chars().take(80).collect();
    tokio::spawn(async move {
        chorus_log(
            "decision.gate.pass",
            &role_str,
            &[("hook", "autonomy-guard"), ("question", &q_short)],
        )
        .await;
    });

    HookResponse::allow()
}

/// Stop hook — scan response for permission-seeking
async fn handle_stop(input: &HookInput, state: &AppState) -> HookResponse {
    // Don't loop — if we already blocked once this turn, let it through
    if input.stop_hook_active == Some(true) {
        return HookResponse::allow();
    }

    let session_id = match &input.session_id {
        Some(s) if !s.is_empty() => s.clone(),
        _ => return HookResponse::allow(),
    };

    let cwd = input.cwd.as_deref().unwrap_or("");
    if cwd.is_empty() {
        return HookResponse::allow();
    }

    let role = input.role();

    // Get last human message from state (cached from UserPromptSubmit)
    let last_human = state.get_last_human_msg(&session_id).await;

    // If no cached human message, try reading from JSONL
    let (last_human_msg, last_response) = if last_human.is_some() {
        // We have the human msg cached; still need the response from JSONL
        let response = read_last_response_from_jsonl(cwd, &session_id, &state.config.home_dir);
        (last_human.unwrap_or_default(), response)
    } else {
        // No cache — read both from JSONL
        let (h, r) = read_last_messages_from_jsonl(cwd, &session_id, &state.config.home_dir);
        (h, r)
    };

    if last_response.is_empty() {
        return HookResponse::allow();
    }

    // Strip code blocks from response for analysis
    let stripped = strip_code_blocks(&last_response);
    let tail: String = stripped.chars().rev().take(500).collect::<String>().chars().rev().collect();
    let tail_lower = tail.to_lowercase();

    // Check if Jeff asked a question (legitimate response)
    let jeff_lower = last_human_msg.trim().to_lowercase();
    if jeff_lower.is_empty() {
        return HookResponse::allow();
    }
    if jeff_lower.ends_with('?') || JEFF_ASKED_RE.is_match(&jeff_lower) {
        return HookResponse::allow();
    }

    // Check for permission-seeking patterns
    let mut is_seeking = false;
    for pattern in SEEKING_PATTERNS.iter() {
        if pattern.is_match(&tail_lower) {
            // Check legit signals
            let mut is_legit = false;
            for legit in LEGIT_SIGNALS.iter() {
                if legit.is_match(&tail_lower) {
                    is_legit = true;
                    break;
                }
            }
            if !is_legit {
                is_seeking = true;
                break;
            }
        }
    }

    if !is_seeking {
        return HookResponse::allow();
    }

    // Cross-reference against preferences
    let mut pref_match: Option<(&str, String, String)> = None;
    for (pid, regex) in STOP_PREF_MATCHERS.iter() {
        if regex.is_match(&tail_lower) {
            // Load preference details from file (or use hardcoded)
            if let Some((ptext, psource)) = lookup_preference(pid, &state.config.prefs_file) {
                pref_match = Some((pid, ptext, psource));
                break;
            }
        }
    }

    let role_str = role.as_str().to_string();

    if let Some((pid, ptext, psource)) = pref_match {
        let pid_str = pid.to_string();
        tokio::spawn(async move {
            chorus_log(
                "decision.gate.matched",
                &role_str,
                &[
                    ("pref", &pid_str),
                    ("hook", "autonomy-guard"),
                    ("source", "response_text"),
                ],
            )
            .await;
        });

        let msg = format!(
            "DEC-069 gate: Your response asked for permission instead of executing. Jeff's known preference ({}): {} (source: {})\n\nRewrite your response: remove the question, do the work, report what you did.",
            pid, ptext, psource
        );
        HookResponse::block_with_stderr(&msg)
    } else {
        tokio::spawn(async move {
            chorus_log(
                "decision.gate.text_leak",
                &role_str,
                &[("hook", "autonomy-guard")],
            )
            .await;
        });

        HookResponse::block_with_stderr(
            "DEC-025 gate: Your response asked for permission instead of executing. Jeff's intent was clear from his last message — go do it.\n\nRules:\n- Don't ask \"should I...\" when the answer is obviously yes\n- Don't narrate a plan and wait for approval\n- Don't present options you already know the answer to\n- Execute and report the outcome\n\nRewrite your response: remove the question, do the work, report what you did."
        )
    }
}

fn strip_code_blocks(text: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    for line in text.lines() {
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if !in_code_block {
            // Also strip inline code
            let clean: String = line
                .split('`')
                .enumerate()
                .filter_map(|(i, s)| if i % 2 == 0 { Some(s) } else { None })
                .collect::<Vec<_>>()
                .join("");
            result.push_str(&clean);
            result.push('\n');
        }
    }
    result
}

/// Read last assistant response from JSONL (for Stop hook)
fn read_last_response_from_jsonl(
    cwd: &str,
    session_id: &str,
    home_dir: &std::path::Path,
) -> String {
    let (_, response) = read_last_messages_from_jsonl(cwd, session_id, home_dir);
    response
}

/// Read last human message and assistant response from session JSONL
fn read_last_messages_from_jsonl(
    cwd: &str,
    session_id: &str,
    home_dir: &std::path::Path,
) -> (String, String) {
    let project_key = cwd.replace('/', "-");
    // Remove leading dash if present
    let project_key = if project_key.starts_with('-') {
        &project_key[1..]
    } else {
        &project_key
    };
    let jsonl_path = home_dir
        .join(".claude/projects")
        .join(format!("-{}", project_key))
        .join(format!("{}.jsonl", session_id));

    let file = match std::fs::File::open(&jsonl_path) {
        Ok(f) => f,
        Err(_) => return (String::new(), String::new()),
    };

    // Read last ~500 lines efficiently
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    let start = if lines.len() > 500 {
        lines.len() - 500
    } else {
        0
    };

    let mut last_human = String::new();
    let mut last_response = String::new();

    for line in &lines[start..] {
        if line.is_empty() {
            continue;
        }
        if let Ok(d) = serde_json::from_str::<serde_json::Value>(line) {
            match d.get("type").and_then(|t| t.as_str()) {
                Some("human") => {
                    if let Some(content) = d
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                    {
                        for block in content {
                            if let Some(text) = block
                                .as_object()
                                .and_then(|o| {
                                    if o.get("type")?.as_str()? == "text" {
                                        o.get("text")?.as_str()
                                    } else {
                                        None
                                    }
                                })
                                .or_else(|| block.as_str())
                            {
                                last_human = text.to_string();
                            }
                        }
                    }
                }
                Some("assistant") => {
                    if let Some(content) = d
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                    {
                        for block in content {
                            if let Some(text) = block.as_object().and_then(|o| {
                                if o.get("type")?.as_str()? == "text" {
                                    o.get("text")?.as_str()
                                } else {
                                    None
                                }
                            }) {
                                last_response = text.to_string();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Truncate
    let h: String = last_human.chars().rev().take(200).collect::<String>().chars().rev().collect();
    let r: String = last_response.chars().rev().take(800).collect::<String>().chars().rev().collect();
    (h, r)
}

/// Load preference text from jeff-preferences.json
fn lookup_preference(pref_id: &str, prefs_path: &std::path::Path) -> Option<(String, String)> {
    let content = std::fs::read_to_string(prefs_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let prefs = json.get("preferences")?.as_array()?;

    for p in prefs {
        if p.get("id")?.as_str()? == pref_id {
            let pref_text = p.get("preference")?.as_str()?.to_string();
            let source = p.get("source")?.as_str()?.to_string();
            return Some((pref_text, source));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookInput;
    use serde_json::json;

    fn ask_question(question: &str) -> HookInput {
        HookInput {
            tool_name: Some("AskUserQuestion".to_string()),
            tool_input: Some(json!({"question": question})),
            tool_response: None,
            session_id: Some("test-session".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    fn user_prompt(text: &str) -> HookInput {
        HookInput {
            tool_name: None,
            tool_input: None,
            tool_response: None,
            session_id: Some("test-session".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: Some(text.to_string()),
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        }
    }

    // === Trigger detection ===

    #[test]
    fn test_detect_trigger_user_prompt() {
        let input = user_prompt("jdi");
        assert!(matches!(detect_trigger(&input), Trigger::UserPrompt));
    }

    #[test]
    fn test_detect_trigger_pre_tool() {
        let input = ask_question("Should I commit?");
        assert!(matches!(detect_trigger(&input), Trigger::PreTool));
    }

    #[test]
    fn test_detect_trigger_stop() {
        let input = HookInput {
            tool_name: None,
            tool_input: None,
            tool_response: None,
            session_id: Some("test".to_string()),
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: Some(true),
            hook_type: None,
            deploy_role: None,
        };
        assert!(matches!(detect_trigger(&input), Trigger::Stop));
    }

    // === PreTool: AskUserQuestion preference matching ===

    #[tokio::test]
    async fn test_pref_p001_commit() {
        let state = AppState::new();
        let input = ask_question("Should I commit and push these changes?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("P001") || out.contains("Commit and push"));
    }

    #[tokio::test]
    async fn test_pref_p002_proceed() {
        let state = AppState::new();
        let input = ask_question("Shall I go ahead with this?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("execute") || out.contains("P002"));
    }

    #[tokio::test]
    async fn test_pref_p003_card() {
        let state = AppState::new();
        let input = ask_question("Should I card this issue?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("Card it") || out.contains("P003"));
    }

    #[tokio::test]
    async fn test_pref_p004_options() {
        let state = AppState::new();
        let input = ask_question("Option A or Option B — which do you prefer?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("P004") || out.contains("just do it"));
    }

    #[tokio::test]
    async fn test_pref_p005_deploy() {
        let state = AppState::new();
        let input = ask_question("Should I deploy this now?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("deploy") || out.contains("P005"));
    }

    #[tokio::test]
    async fn test_pref_p012_plan() {
        let state = AppState::new();
        let input = ask_question("Should I create a plan for this?");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("Vertical") || out.contains("P012"));
    }

    #[tokio::test]
    async fn test_pref_p015_pitch() {
        let state = AppState::new();
        let input = ask_question("Here's what I'm thinking for the approach");
        let r = check(&input, &state).await;
        assert!(r.stdout.is_some());
        let out = r.stdout.unwrap();
        assert!(out.contains("Execute") || out.contains("P015"));
    }

    #[tokio::test]
    async fn test_no_match_genuine_question() {
        let state = AppState::new();
        let input = ask_question("What port is Fuseki running on?");
        let r = check(&input, &state).await;
        // No preference match — should allow without message
        assert!(r.stdout.is_none() || !r.stdout.as_ref().unwrap().contains("DEC-069"));
        assert_eq!(r.exit_code, 0);
    }

    // === UserPromptSubmit: jdi detection ===

    #[tokio::test]
    async fn test_jdi_caches_prompt() {
        let state = AppState::new();
        let input = user_prompt("jdi");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
        // Should have cached the human message
        let cached = state.get_last_human_msg("test-session").await;
        assert_eq!(cached, Some("jdi".to_string()));
    }

    #[tokio::test]
    async fn test_user_prompt_caches_message() {
        let state = AppState::new();
        let input = user_prompt("fix the broken test");
        let r = check(&input, &state).await;
        assert_eq!(r.exit_code, 0);
        let cached = state.get_last_human_msg("test-session").await;
        assert_eq!(cached, Some("fix the broken test".to_string()));
    }

    // === Non-AskUserQuestion tool passes through ===

    #[tokio::test]
    async fn test_non_ask_tool_passes() {
        let state = AppState::new();
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            tool_input: Some(json!({"command": "ls"})),
            tool_response: None,
            session_id: None,
            cwd: Some("/Users/jeffbridwell/CascadeProjects/architect".to_string()),
            prompt: None,
            stop_hook_active: None,
            hook_type: None,
            deploy_role: None,
        };
        let r = check(&input, &state).await;
        assert!(r.stdout.is_none());
        assert_eq!(r.exit_code, 0);
    }

    // === Permission-seeking pattern regexes ===

    #[test]
    fn test_seeking_patterns_match() {
        let cases = vec![
            "shall i go ahead and build this?",
            "should i proceed with the refactor?",
            "would you like me to fix this?",
            "want me to go ahead and deploy?",
            "do you want me to update that?",
            "ready to proceed?",
            "i can do this if you want",
            "here's what i'm thinking for the plan. sound good?",
            "i'll go ahead and start if you approve",
            "option a or option b — which resonates?",
        ];
        for case in cases {
            let matched = SEEKING_PATTERNS.iter().any(|p| p.is_match(case));
            assert!(matched, "Expected SEEKING match for: {}", case);
        }
    }

    #[test]
    fn test_legit_signals_match() {
        let cases = vec![
            "this is genuinely ambiguous",
            "i don't know enough about this",
            "unclear whether we should use option A",
            "could go either way here",
            "there's a trade-off between speed and safety",
            "the risk might be worth accepting",
        ];
        for case in cases {
            let matched = LEGIT_SIGNALS.iter().any(|p| p.is_match(case));
            assert!(matched, "Expected LEGIT match for: {}", case);
        }
    }

    #[test]
    fn test_jeff_asked_re_matches() {
        let cases = vec![
            "what is the port?",
            "how does this work?",
            "why is disk at 87%?",
            "is this working?",
            "can you check?",
            "do we need this?",
            "should we migrate?",
        ];
        for case in cases {
            assert!(JEFF_ASKED_RE.is_match(case), "Expected match for: {}", case);
        }
    }

    // === strip_code_blocks ===

    #[test]
    fn test_strip_code_blocks() {
        let input = "before\n```rust\nlet x = 1;\n```\nafter";
        let result = strip_code_blocks(input);
        assert!(result.contains("before"));
        assert!(result.contains("after"));
        assert!(!result.contains("let x"));
    }

    #[test]
    fn test_strip_inline_code() {
        let input = "use `app-state.sh` for deploys";
        let result = strip_code_blocks(input);
        assert!(result.contains("use"));
        assert!(result.contains("for deploys"));
        assert!(!result.contains("app-state.sh"));
    }
}
