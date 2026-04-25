//! chorus-log — Emit structured JSON events to chorus.log (spine).
//! Subcommand: `chorus-hook-shim log <event> <role> [key=value ...]`
//! Replaces chorus-log.sh (95 lines bash).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

fn log_file() -> String {
    // #2475 — CHORUS_LOG_FILE override for hermetic tests. Production never
    // sets it; tests point at a tempdir to isolate spine writes.
    if let Ok(p) = std::env::var("CHORUS_LOG_FILE") {
        return p;
    }
    format!("{}/platform/logs/chorus.log", crate::shared::state_paths::chorus_root())
}
fn schema_file() -> String { format!("{}/designing/schemas/spine-events.json", crate::shared::state_paths::chorus_root()) }

/// Eastern timezone offset — standalone for shim binary (#1850)
fn eastern_offset() -> chrono::FixedOffset {
    let output = std::process::Command::new("date")
        .args(["+%z"])
        .env("TZ", "America/New_York")
        .output();
    if let Ok(out) = output {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.len() >= 5 {
            let sign = if s.starts_with('-') { -1 } else { 1 };
            let h: i32 = s[1..3].parse().unwrap_or(5);
            let m: i32 = s[3..5].parse().unwrap_or(0);
            if let Some(offset) = chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60)) {
                return offset;
            }
        }
    }
    chrono::FixedOffset::west_opt(5 * 3600).unwrap()
}

pub fn run(args: &[String]) -> ExitCode {
    emit(args, /* silent */ false)
}

/// #2311: silent emit — writes the event to chorus.log but skips the stdout
/// mirror. Used by session-start which owns stdout for the SessionStart
/// hookSpecificOutput envelope.
pub fn run_silent(args: &[String]) -> ExitCode {
    emit(args, /* silent */ true)
}

fn emit(args: &[String], silent: bool) -> ExitCode {
    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim log <event> <role> [key=value ...]");
        return ExitCode::from(1);
    }

    let mut event = args[0].clone();
    let role = &args[1];

    // Alias translation from schema
    if let Ok(schema) = fs::read_to_string(&schema_file()) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&schema) {
            if let Some(aliases) = parsed.get("aliases").and_then(|a| a.as_object()) {
                if let Some(new_name) = aliases.get(&event).and_then(|v| v.as_str()) {
                    event = new_name.to_string();
                }
            }
        }
    }

    // Timestamp — Eastern time (#1850), matching wall-clock.txt and gemba output
    let offset = eastern_offset();
    let ts = chrono::Utc::now().with_timezone(&offset).format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

    // Parse --level flag, default to info
    let mut level = "info".to_string();
    let mut extras = String::new();
    let mut display = String::new();
    for kv in args.iter().skip(2) {
        if kv == "--level" || kv.starts_with("--level=") {
            // Handle --level=critical or --level critical (next arg)
            if let Some(val) = kv.strip_prefix("--level=") {
                level = val.to_string();
            }
            continue;
        }
        // Handle --level <value> (previous was --level, this is the value)
        if ["info", "warn", "critical"].contains(&kv.as_str())
            && args.iter().skip(2).any(|a| a == "--level")
            && !kv.contains('=')
        {
            level = kv.clone();
            continue;
        }
        if let Some((key, val)) = kv.split_once('=') {
            if key == "level" {
                level = val.to_string();
                continue;
            }
            // #2443 follow-on: proper JSON escape — newlines/backslashes/quotes/control
            // chars would otherwise break log-line integrity. Prior `.replace('"', ...)`
            // only handled quotes; multi-line content (Kade's #2280 feedback) produced
            // invalid JSON that the poller silently skipped.
            let escaped_value = serde_json::to_string(&val).unwrap_or_else(|_| "\"\"".to_string());
            extras.push_str(&format!(r#","{}":{}"#, key, escaped_value));
            display.push_str(&format!(" {}={}", key, val));
        }
    }

    // Validate level
    if !["info", "warn", "critical"].contains(&level.as_str()) {
        eprintln!("Invalid level '{}' — use info, warn, or critical", level);
        level = "info".to_string();
    }

    // Write to log
    let line = format!(
        r#"{{"timestamp":"{}","level":"{}","appName":"chorus-events","component":"lifecycle","event":"{}","role":"{}"{}}}"#,
        ts, level, event, role, extras
    );

    let path = PathBuf::from(&log_file());
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}", line);
            if !silent {
                println!("{} | {}{}", event, role, display);
            }
        }
        Err(_) => {
            eprintln!("{} | {} — FAILED to write", event, role);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Find the log line containing the given event name (tests run in parallel,
    /// so we can't rely on "last line").
    fn find_event_line(event: &str) -> Option<String> {
        let content = fs::read_to_string(&log_file()).ok()?;
        let needle = format!("\"event\":\"{}\"", event);
        // Search from the end — our event is recent
        content.lines().rev()
            .find(|l| l.contains(&needle))
            .map(|s| s.to_string())
    }

    /// Extract a single JSON object containing the event from a line that may
    /// have multiple concatenated JSON objects (parallel write race).
    fn extract_json_object(line: &str, event: &str) -> Option<String> {
        let needle = format!("\"event\":\"{}\"", event);
        // Split on }{ boundaries — each segment is a JSON object (minus braces)
        let mut depth = 0i32;
        let mut start = 0;
        for (i, ch) in line.char_indices() {
            match ch {
                '{' => {
                    if depth == 0 { start = i; }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let obj = &line[start..=i];
                        if obj.contains(&needle) {
                            return Some(obj.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    // --- AC3: emit/read-back ---

    #[test]
    fn emit_writes_json_to_log() {
        let result = run(&[
            "test.emit.writes".into(),
            "kade".into(),
            "card=1718".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let line = find_event_line("test.emit.writes").expect("event should be in log");
        assert!(line.contains("\"role\":\"kade\""));
        assert!(line.contains("\"card\":\"1718\""));
    }

    #[test]
    fn emit_produces_valid_json() {
        run(&["test.json.valid2".into(), "silas".into()]);

        let line = find_event_line("test.json.valid2").expect("event in log");
        // Line may contain multiple concatenated JSON objects from parallel writes.
        // Extract just the one containing our event.
        let json_str = extract_json_object(&line, "test.json.valid2")
            .expect("should find JSON object for our event");
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("extracted object should be valid JSON");
        assert_eq!(parsed["event"], "test.json.valid2");
        assert_eq!(parsed["role"], "silas");
        assert!(parsed["timestamp"].is_string());
    }

    #[test]
    fn emit_includes_timestamp() {
        run(&["test.timestamp2".into(), "wren".into()]);

        let line = find_event_line("test.timestamp2").expect("event in log");
        assert!(line.contains("\"timestamp\":\"20"));
        // Eastern time: -0400 or -0500 instead of Z
        assert!(line.contains("-04") || line.contains("-05"), "should have Eastern offset, not Z: {}", line);
    }

    #[test]
    fn emit_multiple_key_values() {
        run(&[
            "test.multi.kv2".into(),
            "kade".into(),
            "card=1718".into(),
            "mode=inject".into(),
            "target=silas".into(),
        ]);

        let line = find_event_line("test.multi.kv2").expect("event in log");
        let json_str = extract_json_object(&line, "test.multi.kv2")
            .expect("should find JSON object");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["card"], "1718");
        assert_eq!(parsed["mode"], "inject");
        assert_eq!(parsed["target"], "silas");
    }

    // --- AC3: alias resolution ---

    #[test]
    fn alias_resolves_to_canonical_event() {
        // "card_created" resolves to "card.item.created" per spine-events.json
        run(&["card_created".into(), "wren".into()]);

        let line = find_event_line("card.item.created").expect("alias should resolve");
        assert!(line.contains("\"role\":\"wren\""));
    }

    #[test]
    fn non_alias_passes_through() {
        run(&["custom.passthrough.event".into(), "kade".into()]);

        let line = find_event_line("custom.passthrough.event")
            .expect("non-alias event should appear as-is");
        assert!(line.contains("\"role\":\"kade\""));
    }

    // --- AC3: error handling ---

    #[test]
    fn no_args_returns_error() {
        let result = run(&[]);
        assert_eq!(result, ExitCode::from(1));
    }

    #[test]
    fn single_arg_returns_error() {
        let result = run(&["event.only".into()]);
        assert_eq!(result, ExitCode::from(1));
    }

    // --- Level flag (#1895) ---

    #[test]
    fn default_level_is_info() {
        run(&["test.level.default".into(), "silas".into()]);
        let line = find_event_line("test.level.default").expect("event in log");
        let json = extract_json_object(&line, "test.level.default").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["level"], "info");
    }

    #[test]
    fn level_equals_syntax() {
        run(&["test.level.equals".into(), "silas".into(), "--level=critical".into()]);
        let line = find_event_line("test.level.equals").expect("event in log");
        let json = extract_json_object(&line, "test.level.equals").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["level"], "critical");
    }

    #[test]
    fn level_kv_syntax() {
        run(&["test.level.kv".into(), "kade".into(), "level=warn".into()]);
        let line = find_event_line("test.level.kv").expect("event in log");
        let json = extract_json_object(&line, "test.level.kv").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["level"], "warn");
    }

    #[test]
    fn level_with_other_fields() {
        run(&["test.level.combo".into(), "wren".into(), "--level=critical".into(), "card=1895".into()]);
        let line = find_event_line("test.level.combo").expect("event in log");
        let json = extract_json_object(&line, "test.level.combo").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["level"], "critical");
        assert_eq!(parsed["card"], "1895");
    }

    // --- AC3: special characters in values ---

    #[test]
    fn handles_quotes_in_values() {
        let result = run(&[
            "test.quotes2".into(),
            "kade".into(),
            r#"detail=said "hello" today"#.into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let line = find_event_line("test.quotes2");
        assert!(line.is_some(), "event with quotes should still be logged");
    }
}
