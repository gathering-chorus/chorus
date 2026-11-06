// @domain: spine
// #4916 — stated intent stall gate on Gemini runtime (prompt_response, no transcript_path).
use chorus_hooks::{load_commitments, stated_intent_from_raw};

#[test]
fn gemini_closing_line_promising_work_is_caught() {
    let p = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../config/stated-intent-vocabulary.json");
    let vocab = load_commitments(&p);

    let raw = serde_json::json!({
        "prompt_response": "--- Borg Queen | 2026-11-06 16:13 Boston | #4916 | Werk v1.11.0 ---\n\n\
Locked on your pragmatic 4-item scope to strip noise with zero rewrite drag:\n\n\
```\n1. Pulse Header\n```\n\n\
Executing slice 1 now."
    });

    let res = stated_intent_from_raw(&raw, &vocab);
    assert_eq!(res, Ok(Some("bare-gerund".to_string())), "16:13 reply must be caught by stated intent gate");
}

#[test]
fn gemini_advancing_promise_is_caught() {
    let p = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../config/stated-intent-vocabulary.json");
    let vocab = load_commitments(&p);

    let raw = serde_json::json!({
        "prompt_response": "--- Borg Queen | 2026-11-06 16:51 Boston | #4916 | Werk v1.11.0 ---\n\n\
```\nSlice 2 green: borg-prove reopens to Later\n```\n\n\
Advancing to Slice 3."
    });

    let res = stated_intent_from_raw(&raw, &vocab);
    assert_eq!(res, Ok(Some("bare-gerund".to_string())), "advancing promise must be caught");
}

#[test]
fn gemini_done_report_passes() {
    let p = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../config/stated-intent-vocabulary.json");
    let vocab = load_commitments(&p);

    let raw = serde_json::json!({
        "prompt_response": "--- Borg Queen | 2026-11-06 17:00 Boston | #4916 | Werk v1.11.0 ---\n\n\
```\nAll tests green and verified across all suites.\n```\n\n\
Nudged Wren."
    });

    let res = stated_intent_from_raw(&raw, &vocab);
    assert_eq!(res, Ok(None), "done report must pass");
}

#[test]
fn missing_prompt_response_returns_error_for_logging() {
    let p = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../config/stated-intent-vocabulary.json");
    let vocab = load_commitments(&p);

    let raw = serde_json::json!({});
    let res = stated_intent_from_raw(&raw, &vocab);
    assert_eq!(res, Err("missing_prompt_response"), "must report missing_prompt_response rather than silent pass");
}
