// @test-type: unit — pure: the product verdict over borg-prove output text; no door, no network.
// @domain: decisions
// @card: #4882
//! #4882 AC4 — the product gate decides the AC from `borg-prove verify`, never from box state.
use werk_demo::product_proof;

const PROVEN: &str = "#4882 against http://localhost:3343\nac-4882-1    met        the reply is the turn's text\n             pass\nac-4882-2    met        a rejected draft is not published\n             pass\n#4882 verify: Proven\n";

#[test]
fn proven_checks_pass_whatever_the_boxes_say_4882() {
    // run 1 of #4882 went red on 3 proven AC whose boxes were unticked: proof now decides
    let ctx = product_proof(true, PROVEN).expect("proven is not red");
    assert!(ctx.contains("deciding source") && ctx.contains("ac-4882-2    met"));
}

#[test]
fn a_failed_check_is_red_naming_the_ac_4882() {
    let out = "ac-4882-1    met        a\n             pass\nac-4882-2    not met    b\n             fail\n#4882 verify: Not met\n";
    let e = product_proof(false, out).unwrap_err();
    assert!(e.contains("ac-4882-2 not met") && !e.contains("ac-4882-1"), "{e}");
}

#[test]
fn an_ac_with_no_check_is_red_even_when_ticked_4882() {
    // negative proof (Silas 17:06): a hand-ticked AC with no check must not pass
    let out = "ac-4882-1    met        a\n             pass\nac-4882-3    unmeasured c (by hand)\n             no check\n#4882 verify: Unmeasured\n";
    let e = product_proof(false, out).unwrap_err();
    assert!(e.contains("ac-4882-3 unmeasured"), "{e}");
}

#[test]
fn a_deferred_ac_is_not_named_red_4882() {
    let out = "ac-4882-1    not met    a\n             fail\nac-4882-2    deferred   b\n             by #4900\n";
    let e = product_proof(false, out).unwrap_err();
    assert!(e.contains("ac-4882-1 not met") && !e.contains("ac-4882-2"), "{e}");
}

#[test]
fn borg_prove_that_cannot_run_is_red_not_a_pass_4882() {
    let e = product_proof(false, "borg-prove failed to start: No such file").unwrap_err();
    assert!(e.contains("failed to start"), "{e}");
}
