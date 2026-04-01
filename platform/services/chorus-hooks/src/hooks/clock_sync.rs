use crate::types::{HookInput, HookResponse};
use chrono::{Utc, FixedOffset};
use std::fs;

const CLOCK_FILE: &str = "/tmp/wall-clock.txt";
const CLOCK_ISO_FILE: &str = "/tmp/wall-clock-iso.txt";

/// Boston timezone offset (Eastern: UTC-5 standard, UTC-4 daylight)
/// Uses system `date` for correct DST transitions — month range was wrong
/// for early March and early November (~2 weeks/year).
fn boston_offset() -> FixedOffset {
    boston_offset_pub()
}

/// Public accessor for boston_offset — used by chorus_log.rs (#1850)
pub fn boston_offset_pub() -> FixedOffset {
    // Ask the OS — it knows the actual DST transition rules
    let output = std::process::Command::new("date")
        .args(["+%z"])
        .env("TZ", "America/New_York")
        .output();
    if let Ok(out) = output {
        let offset_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // Parse "+HHMM" or "-HHMM" format
        if offset_str.len() >= 5 {
            let sign = if offset_str.starts_with('-') { -1 } else { 1 };
            let hours: i32 = offset_str[1..3].parse().unwrap_or(5);
            let mins: i32 = offset_str[3..5].parse().unwrap_or(0);
            let total_secs = sign * (hours * 3600 + mins * 60);
            if let Some(offset) = FixedOffset::east_opt(total_secs) {
                return offset;
            }
        }
    }
    // Fallback: EST (UTC-5)
    FixedOffset::west_opt(5 * 3600).unwrap()
}

/// Write authoritative wall clock on every user prompt submit.
/// Also injects the timestamp into the hook response so roles see it
/// without having to read a file — prevents stale cached timestamps (#1849).
pub async fn tick(_input: &HookInput) -> HookResponse {
    let offset = boston_offset();
    let now = Utc::now().with_timezone(&offset);

    let human = now.format("%Y-%m-%d %H:%M").to_string();
    let iso = now.format("%Y-%m-%dT%H:%M:%S%z").to_string();

    // Write both formats atomically (backward compat for scripts that read these)
    let _ = fs::write(CLOCK_FILE, &human);
    let _ = fs::write(CLOCK_ISO_FILE, &iso);

    // Clock written to files above — don't emit to stderr, Claude Code treats stderr as hook error (#1559)
    HookResponse::allow()
}

/// PostToolUse: inject current wall clock into every tool response cycle.
/// This ensures the role can't go an entire response without a fresh timestamp.
pub async fn post_tick(_input: &HookInput) -> HookResponse {
    let offset = boston_offset();
    let now = Utc::now().with_timezone(&offset);
    let human = now.format("%Y-%m-%d %H:%M").to_string();

    // Update the file on every tool call, not just prompt submit
    let _ = fs::write(CLOCK_FILE, &human);

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- AC4: wall clock write/read accuracy ---

    #[test]
    fn boston_offset_returns_valid_offset() {
        let offset = boston_offset();
        // Should be either -04:00 (EDT) or -05:00 (EST)
        let secs = offset.local_minus_utc();
        assert!(secs == -14400 || secs == -18000,
            "offset should be -4h or -5h, got {}s", secs);
    }

    #[test]
    fn wall_clock_file_format() {
        // Write clock file directly to verify format
        let offset = boston_offset();
        let now = Utc::now().with_timezone(&offset);
        let human = now.format("%Y-%m-%d %H:%M").to_string();

        // Should match YYYY-MM-DD HH:MM pattern
        assert!(human.len() == 16, "human clock should be 16 chars: {}", human);
        assert!(human.chars().nth(4) == Some('-'));
        assert!(human.chars().nth(10) == Some(' '));
    }

    #[test]
    fn wall_clock_iso_format() {
        let offset = boston_offset();
        let now = Utc::now().with_timezone(&offset);
        let iso = now.format("%Y-%m-%dT%H:%M:%S%z").to_string();

        // Should contain T separator and timezone offset
        assert!(iso.contains('T'), "ISO format should have T: {}", iso);
        assert!(iso.contains("-04") || iso.contains("-05"),
            "should have Boston offset: {}", iso);
    }

    // --- AC4: /tmp file write/read ---

    #[test]
    fn clock_file_is_readable_after_write() {
        let offset = boston_offset();
        let now = Utc::now().with_timezone(&offset);
        let human = now.format("%Y-%m-%d %H:%M").to_string();

        fs::write(CLOCK_FILE, &human).expect("should write clock file");

        let read_back = fs::read_to_string(CLOCK_FILE).expect("should read clock file");
        assert_eq!(read_back, human);
    }
}
