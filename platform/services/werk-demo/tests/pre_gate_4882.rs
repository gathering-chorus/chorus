//! @test-type unit
//! #4882 AC4 — the product gate is preceded by borg-prove tick; no other gate is.
use werk_demo::pre_gate_args;

#[test]
fn only_the_product_gate_ticks_ac_from_proof_first_4882() {
    assert_eq!(pre_gate_args("product", 4882), Some(vec!["tick".to_string(), "4882".to_string()]));
    // negative proof: the other gates never touch the card
    for g in ["code", "quality", "arch", "ops"] {
        assert_eq!(pre_gate_args(g, 4882), None, "{g}");
    }
}
