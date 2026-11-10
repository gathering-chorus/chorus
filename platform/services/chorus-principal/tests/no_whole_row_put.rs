// @test-type: unit — a source guard: no whole-row PUT of a Session row comes back
// @domain: identity
//! #4944 AC3 — every 422 since 11-09 13:19 was a whole-row PUT of a Session row. The
//! guard reads chorus-principal's source and fails on any of the shapes that sent one.

/// The lines of `src` that write a Session row whole. Pure, so the negative proof
/// below runs it on a fixture.
fn whole_row_session_puts(src: &str) -> Vec<String> {
    src.lines().enumerate().filter(|(_, l)| {
        let code = l.split("//").next().unwrap_or("");
        code.contains("putable(") || code.contains("closed_row(")
            || (code.contains("identity/sessions") && (code.contains("put_row(") || code.contains("Some(&name)") || code.contains("\"PUT\"")))
    }).map(|(i, l)| format!("{}: {}", i + 1, l.trim())).collect()
}

#[test]
fn no_session_row_is_written_whole() {
    let dir = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src");
    let mut found = vec![];
    let mut files = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().map(|x| x == "rs").unwrap_or(false) {
            files += 1;
            for hit in whole_row_session_puts(&std::fs::read_to_string(&p).unwrap()) { found.push(format!("{}:{}", p.display(), hit)); }
        }
    }
    assert!(files >= 3, "the guard must read the crate's source, read {files} files from {}", dir.display());
    assert!(found.is_empty(), "whole-row Session writes:\n{}", found.join("\n"));
}

/// Negative proof: each shape that sent a whole Session row is caught.
#[test]
fn negative_proof_each_old_shape_is_caught() {
    for bad in [
        r#"    let mut row = crate::rows::putable(row);"#,
        r#"let Some(done) = lifecycle::closed_row(&row.to_string(), &name, &now) else { continue };"#,
        r#"if put_row(ctx, role, "identity/sessions", "session", &s) { kept += 1 }"#,
        r#"let (code, _) = api_send(ctx, &owner, "identity/sessions", Some(&name), &done, "sweep");"#,
    ] {
        assert_eq!(whole_row_session_puts(bad).len(), 1, "not caught: {bad}");
    }
    assert!(whole_row_session_puts(r#"patch_row(ctx, &owner, "identity/sessions", "session", &name, &body)"#).is_empty(), "a PATCH is not a whole-row write");
    assert!(whole_row_session_puts("// putable( was retired by #4944").is_empty(), "a comment is not code");
}
