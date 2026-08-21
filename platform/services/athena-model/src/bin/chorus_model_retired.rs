//! #3718 — `chorus-model` is RETIRED. This binary exists only to fail loudly.
//!
//! Jeff, 2026-07-31: *"i feel we rename chorus-model to athena-model — to me the
//! model part is a strong analog to coding — why i mention werk-code."*
//!
//! `werk-*` is the governed writer for CODE. `athena-model` is the governed
//! writer for the MODEL. The binary name should carry the product it serves.
//!
//! A silent alias would let callers drift on the old name indefinitely, which is
//! how two names for one thing become two implementations. A stub that exits
//! non-zero and names the replacement migrates every caller exactly once. Same
//! treatment the bash `nudge` wrapper got when it was retired (#2804).
fn main() {
    eprintln!(
        "chorus-model is RETIRED (#3718) — use `athena-model` instead.\n\
         \n\
         Same verbs, same behaviour; the name now carries the product it serves:\n\
         werk-* governs CODE, athena-model governs the MODEL.\n\
         \n\
         Update the caller — this stub will not be aliased."
    );
    std::process::exit(2);
}
