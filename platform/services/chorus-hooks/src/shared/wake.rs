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

/// #4362 — pulse now types the nudge itself. A prompt is a relay when it is the
/// old wake line, or when pulse's store holds a delivered nudge with exactly
/// this text. Words nobody delivered stay Jeff's.
pub fn is_relay(prompt: &str) -> bool {
    is_wake_line(prompt) || delivered_by_pulse(&crate::shared::state_paths::messages_db(), prompt)
}

/// Read-only lookup in pulse's messages.db; any read failure answers false.
pub fn delivered_by_pulse(db: &str, prompt: &str) -> bool {
    let text = prompt.trim();
    if text.is_empty() {
        return false;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return false;
    };
    conn.query_row(
        "SELECT 1 FROM messages WHERE type = 'nudge' AND delivery_status = 'delivered' AND trim(content) = ?1 LIMIT 1",
        [text],
        |_| Ok(()),
    )
    .is_ok()
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
        // read at run time: a path baked in at compile time points into the werk
        // the binary was built in, which the nightly no longer has (Silas, 09-27)
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR for tests");
        let ts = format!("{dir}/../../pulse/src/delivery-worker.ts");
        let src = std::fs::read_to_string(ts).expect("pulse delivery-worker.ts must exist beside chorus-hooks");
        // #4362 — pulse types the message itself, never the old line
        assert!(src.contains("return row.content;"), "pulse no longer types the message itself");
        assert!(!src.contains(WAKE_LINE), "pulse types the old wake line again");
    }

    fn store_with(content: &str, status: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("messages.db").to_string_lossy().to_string();
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE messages (id INTEGER PRIMARY KEY, type TEXT, content TEXT, delivery_status TEXT);").unwrap();
        c.execute("INSERT INTO messages (type, content, delivery_status) VALUES ('nudge', ?1, ?2)", [content, status]).unwrap();
        (dir, path)
    }

    #[test]
    fn a_delivered_nudge_is_a_relay_4362() {
        let (_d, db) = store_with("[nudge from silas | x] hi", "delivered");
        assert!(delivered_by_pulse(&db, "  [nudge from silas | x] hi\n"));
    }

    /// NEGATIVE PROOF: words pulse never delivered are Jeff's.
    #[test]
    fn undelivered_words_are_not_a_relay_4362() {
        let (_d, db) = store_with("[nudge from silas | x] hi", "delivered");
        assert!(!delivered_by_pulse(&db, "[nudge from silas | x] something else"));
        assert!(!delivered_by_pulse(&db, "go"));
        let (_p, pending) = store_with("[nudge from silas | x] hi", "pending");
        assert!(!delivered_by_pulse(&pending, "[nudge from silas | x] hi"));
        assert!(!delivered_by_pulse("/nonexistent/messages.db", "[nudge from silas | x] hi"));
    }
}
