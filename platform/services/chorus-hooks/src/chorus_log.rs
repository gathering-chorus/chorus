//! chorus-log — Emit structured JSON events to chorus.log (spine).
//! Subcommand: `chorus-hook-shim log <event> <role> [key=value ...]`
//! Replaces chorus-log.sh (95 lines bash).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const LOG_FILE: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log";
const SCHEMA_FILE: &str = "/Users/jeffbridwell/CascadeProjects/chorus/designing/schemas/spine-events.json";

pub fn run(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim log <event> <role> [key=value ...]");
        return ExitCode::from(1);
    }

    let mut event = args[0].clone();
    let role = &args[1];

    // Alias translation from schema
    if let Ok(schema) = fs::read_to_string(SCHEMA_FILE) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&schema) {
            if let Some(aliases) = parsed.get("aliases").and_then(|a| a.as_object()) {
                if let Some(new_name) = aliases.get(&event).and_then(|v| v.as_str()) {
                    event = new_name.to_string();
                }
            }
        }
    }

    // Timestamp
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

    // Build extra fields
    let mut extras = String::new();
    let mut display = String::new();
    for kv in args.iter().skip(2) {
        if let Some((key, val)) = kv.split_once('=') {
            let escaped = val.replace('"', "\\\"");
            extras.push_str(&format!(r#","{}":"{}""#, key, escaped));
            display.push_str(&format!(" {}={}", key, val));
        }
    }

    // Write to log
    let line = format!(
        r#"{{"timestamp":"{}","level":"info","appName":"chorus-events","component":"lifecycle","event":"{}","role":"{}"{}}}"#,
        ts, event, role, extras
    );

    let path = PathBuf::from(LOG_FILE);
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}", line);
            println!("{} | {}{}", event, role, display);
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
        let content = fs::read_to_string(LOG_FILE).ok()?;
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
        assert!(line.contains("Z\""));
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
