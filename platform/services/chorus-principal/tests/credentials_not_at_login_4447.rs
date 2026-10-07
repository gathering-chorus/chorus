// @domain: identity
//! #4447 — a login never writes Credential rows as the role. Credential rows
//! live in the security graph, which no role grant opens (#4204: that grant is
//! the power to rewrite who exists). The identity owner writes them once, with
//! `chorus-principal credentials <role>`.

/// Lines that call record_credentials anywhere but the owner's verb.
fn login_writes(src: &str) -> Vec<String> {
    src.lines()
        .filter(|l| l.contains("record_credentials(") && !l.trim_start().starts_with("fn ") && !l.trim_start().starts_with("//"))
        .filter(|l| !l.contains("\"credentials\" =>"))
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn no_login_path_writes_credential_rows() {
    let src = std::fs::read_to_string(format!("{}/src/lib.rs", std::env::var("CARGO_MANIFEST_DIR").unwrap())).unwrap();
    assert!(src.contains("fn record_credentials("), "the guard's target is gone; fail loud, not vacuous");
    assert_eq!(login_writes(&src), Vec::<String>::new());
}

#[test]
fn negative_proof_a_login_that_writes_credentials_is_caught() {
    let planted = "fn record_run(ctx: &Ctx) {\n    record_credentials(ctx, role, role);\n}\n\"credentials\" => record_credentials(&ctx, &r, &w),\n";
    assert_eq!(login_writes(planted), vec!["record_credentials(ctx, role, role);".to_string()]);
}
