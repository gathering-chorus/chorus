//! #3670 — bounded tail reads for append-only logs.
//!
//! The 2026-07-23 jetsam: the daemon read the ENTIRE unrotated 1.7GB spine log
//! (`~/.chorus/chorus.log`) with `read_to_string` at four call sites — once per
//! prompt inject, per pulse tick, per role-state read, per observer card lookup.
//! Concurrent injects × 1.7GB strings peaked chorus-hooks at 28.5GB RSS and the
//! kernel killed WindowServer (Jeff's "hard OOM crash"). #3607 fixed exactly this
//! class in nudge_poll; this module promotes that fix to the shared home so no
//! call site ever reads a log whole again.
//!
//! The spine file itself is deliberately NEVER rotated — it's the team's memory
//! layer (Jeff, 2026-07-23). Growth is a disk concern (~15MB/day); reads are the
//! only thing that must be bounded.

use std::path::Path;

/// Default tail budget. Consumers scan at most the last few hundred lines
/// (~100B/line); 8MB covers every current window with orders-of-magnitude slack.
pub const TAIL_BYTES: u64 = 8 * 1024 * 1024;

/// Read the last `tail_bytes` of `path` as (lossy) UTF-8, dropping the partial
/// first line when the read starts mid-file. None on missing/unreadable file.
pub fn read_log_tail_bytes(path: &Path, tail_bytes: u64) -> Option<String> {
    use std::fs;
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let start = size.saturating_sub(tail_bytes);
    if start > 0 {
        f.seek(SeekFrom::Start(start)).ok()?;
    }
    let mut bytes = Vec::with_capacity((size - start) as usize);
    f.read_to_end(&mut bytes).ok()?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        match text.find('\n') {
            Some(nl) => {
                text.drain(..=nl);
            }
            None => return Some(String::new()),
        }
    }
    Some(text)
}

/// Tail read with the default budget — the drop-in replacement for
/// `fs::read_to_string(log)` at every log-scanning call site.
pub fn read_log_tail(path: &Path) -> Option<String> {
    read_log_tail_bytes(path, TAIL_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_log(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("log-tail-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn small_file_returned_whole() {
        let p = tmp_log("small.log", "line1\nline2\nline3\n");
        assert_eq!(read_log_tail_bytes(&p, 1024).unwrap(), "line1\nline2\nline3\n");
    }

    #[test]
    fn large_file_bounded_and_partial_first_line_dropped() {
        let mut content = String::new();
        for i in 0..1000 {
            content.push_str(&format!("event-{:04}\n", i));
        }
        let p = tmp_log("large.log", &content);
        // 100-byte budget over 10-byte lines → at most ~10 lines, first is complete
        let tail = read_log_tail_bytes(&p, 100).unwrap();
        assert!(tail.len() <= 100);
        assert!(tail.starts_with("event-"), "partial first line must be dropped: {:?}", tail);
        assert!(tail.ends_with("event-0999\n"));
    }

    #[test]
    fn budget_larger_than_file_is_fine() {
        let p = tmp_log("tiny.log", "only\n");
        assert_eq!(read_log_tail_bytes(&p, u64::MAX).unwrap(), "only\n");
    }

    #[test]
    fn missing_file_is_none() {
        assert!(read_log_tail(Path::new("/nonexistent/never/there.log")).is_none());
    }

    #[test]
    fn one_giant_line_beyond_budget_yields_empty() {
        let p = tmp_log("oneline.log", &"x".repeat(500));
        assert_eq!(read_log_tail_bytes(&p, 100).unwrap(), "");
    }
}
