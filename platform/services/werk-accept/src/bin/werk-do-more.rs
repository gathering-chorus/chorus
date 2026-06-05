//! werk-do-more (#3237) — the STOP verdicts. `werk-do-more <card> <role> <no-go|more>`.
//! Thin entry; logic in werk_accept::do_more. Writes demo.decision{no-go|more} to the
//! witness werk-demo polls; werk-demo is what then exits 2 (this verb just writes → 0/1).
fn main() {
    std::process::exit(match werk_accept::run_do_more() {
        Ok(msg) => {
            println!("{}", msg);
            0
        }
        Err(e) => {
            eprintln!("werk-do-more: {}", e);
            1
        }
    });
}
