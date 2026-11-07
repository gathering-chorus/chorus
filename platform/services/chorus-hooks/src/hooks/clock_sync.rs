// @domain: spine — placed by chorus-crawl's domain rules (#4538)
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

/// #4835 — every chorus_log line called this, and it forked `date` each time: on
/// the async workers, several times per prompt, so 40 prompts at once queued on
/// process spawns. The OS answer is kept for a minute (DST moves the offset at
/// most twice a year, at a known hour).
type OffsetSlot = std::sync::Mutex<Option<(std::time::Instant, FixedOffset)>>;
static OFFSET_CACHE: OffsetSlot = std::sync::Mutex::new(None);
const OFFSET_TTL: std::time::Duration = std::time::Duration::from_secs(60);
// per test thread, so tests running side by side cannot count each other's asks
#[cfg(test)]
thread_local!(static OFFSET_ASKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });

/// Public accessor for boston_offset — used by chorus_log.rs (#1850)
pub fn boston_offset_pub() -> FixedOffset {
    let mut slot = OFFSET_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((at, off)) = *slot {
        if at.elapsed() < OFFSET_TTL {
            return off;
        }
    }
    let off = ask_os_offset();
    *slot = Some((std::time::Instant::now(), off));
    off
}

fn ask_os_offset() -> FixedOffset {
    #[cfg(test)]
    OFFSET_ASKS.with(|n| n.set(n.get() + 1));
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

        let test_clock = std::env::temp_dir().join(format!("wall-clock-test-{}.txt", std::process::id()));
        fs::write(&test_clock, &human).expect("should write clock file");

        let read_back = fs::read_to_string(&test_clock).expect("should read clock file");
        let _ = fs::remove_file(&test_clock);
        assert_eq!(read_back, human);
    }
}

#[cfg(test)]
mod offset_cache_4835 {
    use super::*;

    #[test]
    fn a_thousand_log_lines_ask_the_os_once() {
        let before = OFFSET_ASKS.with(|n| n.get());
        let first = boston_offset_pub();
        for _ in 0..1000 {
            assert_eq!(boston_offset_pub(), first);
        }
        let asked = OFFSET_ASKS.with(|n| n.get()) - before;
        assert!(asked <= 1, "asked the OS {asked} times for 1001 calls");
    }

    // NEGATIVE PROOF: the uncached path really asks the OS each time
    #[test]
    fn the_os_path_asks_every_call() {
        let before = OFFSET_ASKS.with(|n| n.get());
        for _ in 0..3 {
            let off = ask_os_offset();
            assert!([-4 * 3600, -5 * 3600].contains(&off.local_minus_utc()), "Boston is UTC-4 or UTC-5: {off}");
        }
        assert!(OFFSET_ASKS.with(|n| n.get()) - before >= 3);
    }
}
