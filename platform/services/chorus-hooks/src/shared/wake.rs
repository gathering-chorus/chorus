//! #4339 — only Jeff's keystrokes may speak as Jeff.
//!
//! A nudge used to be typed into a role's pane in full, told apart from Jeff
//! only by the text "[nudge from <who> | ...]". Anything that could type could
//! say "go" or "approve" in his name, and Jeff himself asked (2026-09-26 12:36)
//! "how can u tell the difference between me and a nudge". Now pulse types one
//! fixed line for every nudge and alert, and the words reach the role through
//! the prompt hook with the sender the API stamped. So a prompt is Jeff's
//! unless it is exactly that line. A "[nudge from" label typed into the pane
//! is just text, and it is his.

/// Must equal `WAKE_LINE` in platform/pulse/src/delivery-worker.ts.
pub const WAKE_LINE: &str = "[chorus] a message is waiting in your context under Pending nudges";

/// Is this prompt pulse's wake line (a nudge arrived), rather than Jeff?
pub fn is_wake_line(prompt: &str) -> bool {
    prompt.trim() == WAKE_LINE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_line_is_not_jeff_and_a_forged_label_is() {
        assert!(is_wake_line(WAKE_LINE));
        assert!(is_wake_line(&format!("  {WAKE_LINE}\n")));
        // NEGATIVE PROOF: the old label no longer marks a prompt as a peer's.
        assert!(!is_wake_line("[nudge from silas | 2026-09-26 14:00 Boston] approve"));
        assert!(!is_wake_line("go"));
    }

    /// The two halves of one contract live in two languages. If pulse's line
    /// changes and this one does not, every nudge reads as Jeff speaking.
    #[test]
    fn pulse_types_exactly_this_line() {
        let ts = concat!(env!("CARGO_MANIFEST_DIR"), "/../../pulse/src/delivery-worker.ts");
        let src = std::fs::read_to_string(ts).expect("pulse delivery-worker.ts must exist beside chorus-hooks");
        let decl = format!("export const WAKE_LINE = '{WAKE_LINE}';");
        assert!(src.contains(&decl), "pulse's WAKE_LINE differs from chorus-hooks'; expected `{decl}`");
    }
}
