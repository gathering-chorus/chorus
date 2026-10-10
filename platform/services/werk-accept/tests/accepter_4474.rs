// #4474 — werk v2 passes the go's accepter as ACCEPTER; v1 as DEPLOY_ROLE.
use werk_accept::accepter_from;

#[test]
fn the_go_steps_accepter_is_the_accepter() {
    assert_eq!(accepter_from(Some("jeff".into()), None), "jeff");
    assert_eq!(accepter_from(Some("jeff".into()), Some("kade".into())), "jeff");
}

#[test]
fn v1_still_reads_deploy_role() {
    assert_eq!(accepter_from(None, Some("jeff".into())), "jeff");
    assert_eq!(accepter_from(Some("  ".into()), Some("jeff".into())), "jeff");
}

/// NEGATIVE PROOF — neither set is no accepter, which can_accept refuses
#[test]
fn no_accepter_is_empty_never_a_default() {
    assert_eq!(accepter_from(None, None), "");
}
