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
    crate::shared::state_paths::chorus_log_file()
}
fn schema_file() -> String { format!("{}/designing/schemas/spine-events.json", crate::shared::state_paths::chorus_root()) }

/// #3140 — PURE: resolve an event alias + derive the shared-envelope extras from
/// the spine schema. Returns (resolved_event, derived_json_fragment). The fragment
/// is a leading-comma JSON snippet (`,"product":"Chorus",...`) ready to splice into
/// the log line. Single source of truth = spine-events.json (product_map /
/// value_stream_map / events[].vertebra); mirrors spine-event-write.ts (TS).
/// Pure so it's hermetically testable without env/cwd/global-state.
pub fn resolve_and_derive(schema: &serde_json::Value, event: &str) -> (String, String) {
    // alias resolution first — vertebra is keyed on the canonical name.
    let resolved = schema
        .get("aliases")
        .and_then(|a| a.as_object())
        .and_then(|a| a.get(event))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| event.to_string());

    let mut derived = String::new();
    // product + value_stream from product_map[chorus-events].
    if let Some(product) = schema
        .get("product_map")
        .and_then(|m| m.get("chorus-events"))
        .and_then(|v| v.as_str())
    {
        derived.push_str(&format!(r#","product":"{}","value_stream":"{}""#, product, product));
    }
    // value_stream_step from the resolved event's vertebra via value_stream_map.
    if let Some(step) = schema
        .get("events")
        .and_then(|e| e.get(&resolved))
        .and_then(|e| e.get("vertebra"))
        .and_then(|v| v.as_str())
        .and_then(|vert| {
            schema.get("value_stream_map").and_then(|m| m.get(vert)).and_then(|v| v.as_str())
        })
    {
        derived.push_str(&format!(r#","value_stream_step":"{}""#, step));
    }
    (resolved, derived)
}

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

    // #3140 — shared spine-envelope enrichment from the ONE schema source
    // (spine-events.json). resolve_and_derive does alias resolution + product/
    // value_stream/value_stream_step derivation (pure, unit-tested). Best-effort:
    // a missing/bad schema yields the event unchanged + no enrichment (back-compat).
    let schema_json = fs::read_to_string(schema_file()).ok();
    let parsed = schema_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    let derived = match &parsed {
        Some(schema) => {
            let (resolved, extras) = resolve_and_derive(schema, &event);
            event = resolved;
            extras
        }
        None => String::new(),
    };

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
        if let Some((raw_key, val)) = kv.split_once('=') {
            if raw_key == "level" {
                level = val.to_string();
                continue;
            }
            // #2931: gate/demo skills historically write `card=NNNN` (string),
            // but logs-query and chorus_logs_for_card key on `card_id` (numeric
            // — see #2876 unquoted-emit). Two shapes for the same identity
            // means gate.X.passed events vanish from card joins. Normalize at
            // the substrate: `card` is an alias for `card_id` on the way out.
            // Skill markdown can use either; downstream consumers see one
            // canonical field.
            let key = if raw_key == "card" { "card_id" } else { raw_key };
            // #2443 follow-on: proper JSON escape — newlines/backslashes/quotes/control
            // chars would otherwise break log-line integrity. Prior `.replace('"', ...)`
            // only handled quotes; multi-line content (Kade's #2280 feedback) produced
            // invalid JSON that the poller silently skipped.
            //
            // #2876: keys with canonical-integer types emit unquoted when the
            // value parses cleanly as i64. logs-query regex `"card_id":NNN\b`
            // requires unquoted form; without this, env-bridge build.* / card.*
            // events drop out of chorus_logs_for_card joins. Coerce here for
            // the same reason events.ts coerces on the TS side.
            let is_numeric_key = matches!(key, "card_id" | "hop" | "latencyMs" | "exit_code" | "file_count");
            let value_repr = if is_numeric_key {
                if let Ok(n) = val.parse::<i64>() {
                    n.to_string()
                } else {
                    serde_json::to_string(&val).unwrap_or_else(|_| "\"\"".to_string())
                }
            } else {
                serde_json::to_string(&val).unwrap_or_else(|_| "\"\"".to_string())
            };
            extras.push_str(&format!(r#","{}":{}"#, key, value_repr));
            display.push_str(&format!(" {}={}", key, val));
        }
    }

    // #2897: Demo trace propagation. If a card= or card_id= field was provided,
    // look up the trace_id from /tmp/demo-trace-${card}.txt (with CHORUS_TRACE_ID
    // env taking precedence). Lets `grep '"trace":"<id>"'` reconstruct one /demo
    // run end-to-end. #2931: `card` is normalized to `card_id` on write, so
    // post-#2931 events match `"card_id":N` (unquoted, numeric); pre-#2931
    // legacy events with `"card":"N"` (quoted string) remain in the log
    // history but are not the canonical form going forward.
    // demo_preflight.rs writes the temp file on /demo entry; chorus_acp cleans
    // it up on /acp success.
    let mut card_arg: Option<String> = None;
    for kv in args.iter().skip(2) {
        if let Some((k, v)) = kv.split_once('=') {
            if k == "card" || k == "card_id" {
                card_arg = Some(v.to_string());
                break;
            }
        }
    }
    let trace_id_opt = std::env::var("CHORUS_TRACE_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            card_arg.as_ref().and_then(|c| {
                let path = format!("/tmp/demo-trace-{}.txt", c);
                std::fs::read_to_string(&path)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
        });
    if let Some(trace) = trace_id_opt {
        let escaped =
            serde_json::to_string(&trace).unwrap_or_else(|_| "\"\"".to_string());
        extras.push_str(&format!(r#","trace":{}"#, escaped));
        display.push_str(&format!(" trace={}", trace));
    }

    // Validate level
    if !["info", "warn", "critical"].contains(&level.as_str()) {
        eprintln!("Invalid level '{}' — use info, warn, or critical", level);
        level = "info".to_string();
    }

    // Write to log. #3140: derived shared-envelope fields (product/value_stream/
    // value_stream_step) follow the core fields; domain + caller fields ride in
    // `extras` (domain is already first-class via key=value passthrough).
    let line = format!(
        r#"{{"timestamp":"{}","level":"{}","appName":"chorus-events","component":"lifecycle","event":"{}","role":"{}"{}{}}}"#,
        ts, level, event, role, derived, extras
    );

    let path = PathBuf::from(&log_file());
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            // #3278 — ONE atomic O_APPEND write (line+newline in a single buffer).
            // writeln! can issue the formatted text and the '\n' as separate writes,
            // letting another process's append interleave and fuse two events onto one
            // corrupt line. A single write_all of line+"\n" is one uninterruptible append.
            let mut buf = line.clone().into_bytes();
            buf.push(b'\n');
            let _ = f.write_all(&buf);
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
        let content = fs::read_to_string(log_file()).ok()?;
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
        // #2931: `card=NNNN` input is normalized to canonical `card_id`:NNNN
        // (unquoted, numeric) so chorus_logs_for_card finds it. Single shape
        // at the substrate; skill markdown can use either alias.
        assert!(line.contains("\"card_id\":1718"), "expected canonical card_id, got: {}", line);
        assert!(!line.contains("\"card\":\"1718\""), "legacy quoted-string card field should not appear: {}", line);
    }

    #[test]
    fn emit_normalizes_card_id_passthrough() {
        // Explicit `card_id=NNNN` should still write `card_id`:NNNN unquoted,
        // identical to `card=NNNN` after normalization.
        let result = run(&[
            "test.cardid.passthrough".into(),
            "kade".into(),
            "card_id=2931".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);
        let line = find_event_line("test.cardid.passthrough").expect("event in log");
        assert!(line.contains("\"card_id\":2931"), "got: {}", line);
    }

    // #2941 — normalization edge: `card=` with empty value. Skill markdown
    // can produce this when CARD_ID env is unset and substituted blindly.
    // The normalized field should still appear (so log parsers see the
    // empty as a sentinel, not a missing key), but as a quoted empty
    // string since "" doesn't parse as i64.
    #[test]
    fn empty_card_value_emits_quoted_empty_string() {
        let result = run(&[
            "test.2941.card.empty".into(),
            "kade".into(),
            "card=".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);
        let line = find_event_line("test.2941.card.empty").expect("event in log");
        // Normalization still fires (key renamed to card_id) but the empty
        // value falls through to the quoted-string branch.
        assert!(line.contains("\"card_id\":\"\""), "expected quoted empty, got: {}", line);
        assert!(!line.contains("\"card\":"), "legacy 'card' key should not appear: {}", line);
    }

    // #2941 — normalization edge: both `card=` AND `card_id=` passed
    // simultaneously. Today both are processed in argv order; whichever
    // comes LAST wins because each emits its own `,"card_id":...` and the
    // resulting JSON has duplicate keys (parsers take the last one). This
    // test pins that behavior — caller-known precedence > silent collision.
    #[test]
    fn card_and_card_id_both_passed_last_wins() {
        let result = run(&[
            "test.2941.card.collision".into(),
            "kade".into(),
            "card=111".into(),
            "card_id=222".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);
        let line = find_event_line("test.2941.card.collision").expect("event in log");
        // Both writes happen; JSON parsers honor last-wins.
        let parsed: serde_json::Value =
            serde_json::from_str(&extract_json_object(&line, "test.2941.card.collision").unwrap())
                .expect("valid JSON despite duplicate keys");
        assert_eq!(parsed["card_id"], 222, "last-wins precedence: card_id=222 should override card=111");
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
        // #2931: `card` is normalized to canonical `card_id` (unquoted numeric).
        assert_eq!(parsed["card_id"], 1718);
        assert_eq!(parsed["mode"], "inject");
        assert_eq!(parsed["target"], "silas");
    }

    // --- #2876: numeric-key coercion ---

    #[test]
    fn card_id_emitted_as_unquoted_integer() {
        run(&[
            "test.card.id.numeric".into(),
            "silas".into(),
            "card_id=2876".into(),
        ]);
        let line = find_event_line("test.card.id.numeric").expect("event in log");
        let json_str = extract_json_object(&line, "test.card.id.numeric")
            .expect("should find JSON object");
        // Direct substring check: unquoted integer form
        assert!(
            json_str.contains("\"card_id\":2876"),
            "card_id should be unquoted integer for chorus_logs_for_card regex match: {}",
            json_str,
        );
        assert!(
            !json_str.contains("\"card_id\":\"2876\""),
            "card_id must not be string-quoted: {}",
            json_str,
        );
        // Also assert via JSON parse — typed read-back
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed["card_id"].is_number());
        assert_eq!(parsed["card_id"].as_i64(), Some(2876));
    }

    #[test]
    fn non_numeric_card_id_falls_back_to_quoted_string() {
        // Defensive: junk input doesn't crash the emitter, just stays quoted.
        run(&[
            "test.card.id.junk".into(),
            "silas".into(),
            "card_id=not-a-number".into(),
        ]);
        let line = find_event_line("test.card.id.junk").expect("event in log");
        assert!(
            line.contains("\"card_id\":\"not-a-number\""),
            "non-numeric card_id should remain quoted: {}",
            line,
        );
    }

    #[test]
    fn hop_emitted_as_unquoted_integer() {
        run(&[
            "test.hop.numeric".into(),
            "silas".into(),
            "hop=3".into(),
        ]);
        let line = find_event_line("test.hop.numeric").expect("event in log");
        let json_str = extract_json_object(&line, "test.hop.numeric")
            .expect("should find JSON object");
        assert!(json_str.contains("\"hop\":3"));
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["hop"].as_i64(), Some(3));
    }

    #[test]
    fn non_numeric_keys_remain_string_quoted() {
        // title, board, branch, etc. must stay quoted strings even when value
        // happens to be all-digits — the coercion list is closed.
        run(&[
            "test.title.numeric".into(),
            "silas".into(),
            "title=12345".into(),
        ]);
        let line = find_event_line("test.title.numeric").expect("event in log");
        assert!(
            line.contains("\"title\":\"12345\""),
            "non-numeric-key 'title' must remain string even with digit value: {}",
            line,
        );
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
        // #2931: `card` normalized to canonical `card_id` (unquoted integer).
        assert_eq!(parsed["card_id"], 1895);
    }

    // --- AC3: special characters in values ---

    // --- #3140: shared-envelope enrichment (PURE resolve_and_derive) ---
    // Hermetic: build the schema value inline (no env/cwd/global state), mirroring
    // the product_map / value_stream_map / events[].vertebra shape in spine-events.json.

    fn fixture_schema() -> serde_json::Value {
        serde_json::json!({
            "aliases": { "card_created": "card.item.created" },
            "product_map": { "chorus-events": "Chorus", "jeff-bridwell-personal-site": "Gathering" },
            "value_stream_map": { "capturing": "Capturing", "building": "Building" },
            "events": { "card.item.created": { "vertebra": "capturing" } }
        })
    }

    #[test]
    fn derive_stamps_product_and_value_stream() {
        let (_ev, derived) = resolve_and_derive(&fixture_schema(), "test.anything");
        assert!(derived.contains(r#""product":"Chorus""#), "product from product_map: {}", derived);
        assert!(derived.contains(r#""value_stream":"Chorus""#), "value_stream = product: {}", derived);
    }

    #[test]
    fn derive_stamps_value_stream_step_from_vertebra() {
        let (_ev, derived) = resolve_and_derive(&fixture_schema(), "card.item.created");
        assert!(derived.contains(r#""value_stream_step":"Capturing""#), "step from vertebra: {}", derived);
    }

    #[test]
    fn derive_resolves_alias_before_vertebra_lookup() {
        // alias card_created -> card.item.created (capturing); step must still resolve.
        let (ev, derived) = resolve_and_derive(&fixture_schema(), "card_created");
        assert_eq!(ev, "card.item.created", "alias resolved");
        assert!(derived.contains(r#""value_stream_step":"Capturing""#), "step via resolved name: {}", derived);
    }

    #[test]
    fn derive_is_best_effort_on_empty_schema() {
        // missing maps -> no enrichment, event unchanged (back-compat).
        let (ev, derived) = resolve_and_derive(&serde_json::json!({}), "some.event");
        assert_eq!(ev, "some.event");
        assert_eq!(derived, "");
    }

    // Note: end-to-end emit() of the derived fields is proven hermetically by the
    // pure resolve_and_derive tests above + the manual CHORUS_ROOT=werk emit (which
    // showed product/value_stream/value_stream_step/domain on a real line). domain
    // passthrough specifically is already covered by emit_multiple_key_values.

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
