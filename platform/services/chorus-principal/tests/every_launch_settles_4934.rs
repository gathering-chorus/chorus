// @test-type: unit — a structural guard over lib.rs: every launch path settles its pane
// @domain: identity
//! #4934 AC3 reopen — prod 2026-11-09 10:31: kade's pane was dead on every quit. The
//! settle step (remain-on-exit off once the runtime runs; no_dead_pane_4934.rs proves
//! what it does in real tmux) was in on_runtime only; `on` (claude: kade, silas, wren),
//! `on_agent` and `on_gemini` start their runtime in a pane and never settled it.
//! Now each confirmed-running point goes through mark_running. This guard reads the
//! source: every fn that calls start_in_pane must also call mark_running.
//!
//! NEGATIVE PROOF (#3734): the 11-09 10:17 shape (settle only in on_runtime) is caught
//! by the fixture case below, and the real 10:17 lib.rs failed this guard on `on`,
//! `on_agent` and `on_gemini` when swapped in.

const LIB: &str = include_str!("../src/lib.rs");

/// (fn name, body) for each top-level fn in `src`.
fn fns(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for chunk in src.split("\nfn ").skip(1).chain(src.split("\npub fn ").skip(1)) {
        let name = chunk.split(|c: char| c == '(' || c == '<').next().unwrap_or("").trim().to_string();
        // a body runs to the next top-level item
        let body = chunk.split("\n}\n").next().unwrap_or("").to_string();
        out.push((name, body));
    }
    out
}

fn launchers(src: &str) -> Vec<(String, String)> {
    fns(src).into_iter().filter(|(n, b)| n != "start_in_pane" && b.contains("start_in_pane(ctx")).collect()
}

fn unsettled_launchers(src: &str) -> Vec<String> {
    launchers(src).into_iter().filter(|(_, b)| !b.contains("mark_running(ctx")).map(|(n, _)| n).collect()
}

#[test]
fn every_fn_that_launches_into_a_pane_settles_it() {
    let names: Vec<String> = launchers(LIB).into_iter().map(|(n, _)| n).collect();
    // the guard must be looking at the real launch paths, never vacuously empty
    for must in ["on", "on_agent", "on_gemini", "on_runtime"] {
        assert!(names.iter().any(|l| l == must), "{must} not found as a launcher: {names:?}");
    }
    assert_eq!(unsettled_launchers(LIB), Vec::<String>::new());
}

#[test]
fn negative_proof_the_1017_shape_is_caught() {
    let old = "\nfn on(ctx: &Ctx) {\n    start_in_pane(ctx, role, d, l)?;\n    ctx.set_bar(role, word);\n}\n\
               \nfn on_runtime(ctx: &Ctx) {\n    start_in_pane(ctx, role, d, l)?;\n    mark_running(ctx, role, word);\n}\n";
    assert_eq!(unsettled_launchers(old), vec!["on".to_string()]);
}
