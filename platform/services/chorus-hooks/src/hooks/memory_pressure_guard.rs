//! Memory-pressure guard (#3625 AC2)
//!
//! PreToolUse on Task/Agent: refuses to spawn a subagent while the box is
//! already past the memory floor. Born from the 2026-07-07 Library OOM —
//! swap went 4.7→20GB in 15 minutes while a 4-agent Explore fanout, four
//! headless gate claudes, and werk env-up overlapped on 16GB; Jeff hard
//! powered off. Each subagent is a 0.5–2GB process; spawning into a
//! spiraling box is what turns pressure into a seizure.
//!
//! Floors mirror werk-demo's gate-spawn floor and the LibrarySwapPressure
//! alert (both #3625): swap > 8GB or free < 10% ⇒ refuse, with a message
//! telling the role to wait for pressure to drain and retry. Unmeasurable
//! metrics never block (non-macOS / hosted CI fail open). The parse/verdict
//! logic is intentionally mirrored from werk-demo/src/lib.rs — separate
//! crates, same threshold contract; change them together.

use crate::types::{HookInput, HookResponse};
use std::process::Command;

const GB: u64 = 1024 * 1024 * 1024;

/// Parse `sysctl vm.swapusage` "used" into bytes (K/M/G suffixes).
pub fn parse_swap_used_bytes(sysctl_out: &str) -> Option<u64> {
    let rest = sysctl_out.split("used =").nth(1)?;
    let tok = rest.split_whitespace().next()?;
    let (num, unit) = tok.split_at(tok.len().saturating_sub(1));
    let val: f64 = num.parse().ok()?;
    let mult = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((val * mult) as u64)
}

/// Parse `memory_pressure -Q` "System-wide memory free percentage: NN%".
pub fn parse_free_pct(out: &str) -> Option<u8> {
    let rest = out.split("free percentage:").nth(1)?;
    rest.trim().trim_end_matches('%').split('%').next()?.trim().parse().ok()
}

/// true = proceed. Unmeasurable never blocks; a measurable floor breach does.
pub fn memory_floor_verdict(
    swap_used: Option<u64>,
    free_pct: Option<u8>,
    swap_floor: u64,
    free_floor: u8,
) -> bool {
    if let Some(swap) = swap_used {
        if swap > swap_floor {
            return false;
        }
    }
    if let Some(free) = free_pct {
        if free < free_floor {
            return false;
        }
    }
    true
}

fn floors() -> (u64, u8) {
    let swap_floor = std::env::var("CHORUS_MEM_SWAP_FLOOR_GB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(8)
        * GB;
    let free_floor: u8 = std::env::var("CHORUS_MEM_FREE_FLOOR_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    (swap_floor, free_floor)
}

fn sample_memory() -> (Option<u64>, Option<u8>) {
    let swap = Command::new("sysctl")
        .arg("vm.swapusage")
        .output()
        .ok()
        .and_then(|o| parse_swap_used_bytes(&String::from_utf8_lossy(&o.stdout)));
    let free = Command::new("memory_pressure")
        .arg("-Q")
        .output()
        .ok()
        .and_then(|o| parse_free_pct(&String::from_utf8_lossy(&o.stdout)));
    (swap, free)
}

/// Pure decision for tests: should this tool call be refused, and with what message?
pub fn decide(tool: &str, swap: Option<u64>, free: Option<u8>, swap_floor: u64, free_floor: u8) -> Option<String> {
    if !matches!(tool, "Task" | "Agent") {
        return None;
    }
    if memory_floor_verdict(swap, free, swap_floor, free_floor) {
        return None;
    }
    let swap_gb = swap.map(|b| b as f64 / GB as f64).unwrap_or(0.0);
    Some(format!(
        "BLOCKED: memory pressure (#3625) — swap {:.1}GB / free {}%, floor is {}GB swap / {}% free. \
         A subagent is a 0.5-2GB process; spawning now risks the 2026-07-07 OOM spiral (4.7→20GB swap in 15 min, hard power-off). \
         Do the work inline in this session, or wait for pressure to drain (check: sysctl vm.swapusage) and retry. \
         Do not fan out until swap is back under the floor.",
        swap_gb,
        free.map(|f| f.to_string()).unwrap_or_else(|| "?".into()),
        swap_floor / GB,
        free_floor
    ))
}

pub async fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if !matches!(tool, "Task" | "Agent") {
        return HookResponse::allow();
    }
    if std::env::var("CHORUS_MEM_FLOOR_DISABLE").is_ok() {
        return HookResponse::allow();
    }
    let (swap_floor, free_floor) = floors();
    let (swap, free) = sample_memory();
    match decide(tool, swap, free, swap_floor, free_floor) {
        Some(msg) => HookResponse::block_with_stderr(&msg),
        None => HookResponse::allow(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_spawn_tools_are_never_touched() {
        assert!(decide("Bash", Some(20 * GB), Some(2), 8 * GB, 10).is_none());
        assert!(decide("Edit", Some(20 * GB), Some(2), 8 * GB, 10).is_none());
    }

    #[test]
    fn spawn_allowed_when_headroom_exists() {
        assert!(decide("Task", Some(3 * GB), Some(40), 8 * GB, 10).is_none());
        assert!(decide("Agent", Some(3 * GB), Some(40), 8 * GB, 10).is_none());
    }

    #[test]
    fn spawn_refused_when_swap_over_floor() {
        let msg = decide("Task", Some(9 * GB), Some(30), 8 * GB, 10).unwrap();
        assert!(msg.contains("memory pressure"));
        assert!(msg.contains("9.0GB"));
    }

    #[test]
    fn spawn_refused_when_free_pct_under_floor() {
        assert!(decide("Agent", Some(2 * GB), Some(5), 8 * GB, 10).is_some());
    }

    #[test]
    fn unmeasurable_fails_open() {
        assert!(decide("Task", None, None, 8 * GB, 10).is_none());
    }

    #[test]
    fn parses_real_sysctl_shape() {
        let out = "vm.swapusage: total = 2048.00M  used = 1021.88M  free = 1026.12M  (encrypted)";
        assert_eq!(parse_swap_used_bytes(out), Some((1021.88f64 * 1024.0 * 1024.0) as u64));
    }

    #[test]
    fn parses_real_memory_pressure_shape() {
        let out = "The system has 17179869184 (1048576 pages with a page size of 16384).\nSystem-wide memory free percentage: 47%\n";
        assert_eq!(parse_free_pct(out), Some(47));
    }
}
