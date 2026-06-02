//! Pure-helper units for werk-acp: the composition plan is the contract, so it is
//! asserted directly (order, the gating deploy target, the accepter step).

use werk_acp::{branch_name, plan, resolve_trace};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3176), "kade/3176");
}

#[test]
fn plan_is_commit_push_deploy_accept_in_order() {
    let p = plan();
    let labels: Vec<&str> = p.iter().map(|s| s.label).collect();
    assert_eq!(labels, vec!["commit", "push", "deploy", "accept"], "the composition order is the contract");
}

#[test]
fn deploy_step_targets_canonical_and_is_before_accept() {
    let p = plan();
    let deploy = p.iter().find(|s| s.label == "deploy").unwrap();
    assert_eq!(deploy.verb, "werk-deploy");
    assert!(deploy.extra_args.contains(&"--target") && deploy.extra_args.contains(&"canonical"),
        "deploy gates on canonical (test-in-prod) before accept");
    let deploy_i = p.iter().position(|s| s.label == "deploy").unwrap();
    let accept_i = p.iter().position(|s| s.label == "accept").unwrap();
    assert!(deploy_i < accept_i, "deploy+verify must gate accept");
}

#[test]
fn only_accept_runs_as_the_accepter() {
    let p = plan();
    let accepter_steps: Vec<&str> = p.iter().filter(|s| s.accepter_step).map(|s| s.label).collect();
    assert_eq!(accepter_steps, vec!["accept"], "only the finalize step carries the accepter identity (DEC-048)");
}

#[test]
fn resolve_trace_honors_the_inherited_env() {
    std::env::set_var("CHORUS_TRACE_ID", "shared-trace-xyz");
    assert_eq!(resolve_trace(3176), "shared-trace-xyz", "one trace threads the whole accept");
    std::env::remove_var("CHORUS_TRACE_ID");
}
