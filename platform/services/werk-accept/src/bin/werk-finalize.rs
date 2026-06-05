//! werk-finalize (#3237) — the MECHANICAL post-deploy finalize. `werk-finalize <card> <role>`.
//! Thin entry; logic in werk_accept::finalize. NO authority (act runs it after merge+
//! deploy-prod): board Done + card.accepted + teardown + chorus/accept. Ok→0, Err→1.
fn main() {
    std::process::exit(match werk_accept::run_finalize() {
        Ok(msg) => {
            println!("{}", msg);
            0
        }
        Err(e) => {
            eprintln!("werk-finalize: {}", e);
            1
        }
    });
}
