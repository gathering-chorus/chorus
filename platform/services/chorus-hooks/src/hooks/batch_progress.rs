//! Batch Progress Emission (#1656)
//!
//! PostToolUse hook on Bash when run_in_background=true.
//! Spawns a monitor that tails the output file every 30 seconds,
//! emitting progress to chorus log + Bridge.
//! Silent background jobs become impossible.

use crate::types::{HookInput, HookResponse};
use std::process::Command;
use tracing::{info, warn};

const CHORUS_LOG: &str = "/Users/jeffbridwell/CascadeProjects/messages/scripts/chorus-log.sh";

/// PreToolUse: detect run_in_background=true and spawn a monitor.
/// The monitor waits for the output file to appear, then tails it every 30s.
pub fn check_pre_bg(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    let run_bg = input
        .tool_input
        .as_ref()
        .and_then(|v| v.get("run_in_background"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !run_bg {
        return HookResponse::allow();
    }

    let command = input.get_tool_input_str("command");
    let cmd_short: String = command.lines().next().unwrap_or("").chars().take(80).collect();
    let role = input.role();

    info!(role = role.as_str(), cmd = %cmd_short, "batch-progress-pre: run_in_background detected");

    let monitor_script = format!(
        r#"#!/bin/bash
ROLE="{role}"
CMD="{cmd_escaped}"
START=$(date +%s)
INTERVAL=30
MARKER="/tmp/batch-pre-marker-{rust_pid}.txt"

# Wait for a new .output file to appear
OUTPUT_FILE=""
for attempt in $(seq 1 10); do
    sleep 3
    NEWEST=$(find /private/tmp/claude-501/ -path "*/tasks/*.output" -newer "$MARKER" -type f 2>/dev/null | head -1)
    if [ -n "$NEWEST" ]; then
        OUTPUT_FILE="$NEWEST"
        break
    fi
done

if [ -z "$OUTPUT_FILE" ]; then
    exit 0
fi

# First tick at 5 seconds, then every 30s
FIRST=1
PREV_LINES=0
STALE_COUNT=0
while true; do
    if [ "$FIRST" = "1" ]; then
        sleep 5
        FIRST=0
    else
        sleep $INTERVAL
    fi

    if [ ! -f "$OUTPUT_FILE" ]; then
        break
    fi

    ELAPSED=$(( $(date +%s) - START ))
    MINS=$(( ELAPSED / 60 ))
    SECS=$(( ELAPSED % 60 ))

    LAST_LINE=$(tail -1 "$OUTPUT_FILE" 2>/dev/null | head -c 120)
    LINES=$(wc -l < "$OUTPUT_FILE" 2>/dev/null | tr -d ' ')

    # Stagnation detection — stop if output unchanged for 2 consecutive checks
    if [ "$LINES" = "$PREV_LINES" ]; then
        STALE_COUNT=$((STALE_COUNT + 1))
        if [ "$STALE_COUNT" -ge 2 ]; then
            SAFE_CMD=$(echo "$CMD" | tr '"' "'" | tr '\\' '/' | head -c 40)
            printf '{{"from":"system","text":"[batch-complete] %s: %s lines in %sm%ss"}}' "$SAFE_CMD" "$LINES" "$MINS" "$SECS" > /tmp/batch-bridge-msg-$$.json
            /usr/bin/curl -s -X POST http://localhost:3470/api/message \
              -H 'Content-Type: application/json' \
              -d @/tmp/batch-bridge-msg-$$.json \
              --connect-timeout 2 > /dev/null 2>&1
            /bin/bash {chorus_log} batch.progress.complete "$ROLE" cmd="$CMD" elapsed="${{MINS}}m${{SECS}}s" lines="$LINES" 2>/dev/null
            break
        fi
    else
        STALE_COUNT=0
    fi
    PREV_LINES=$LINES

    # Parse progress from output — look for N/M patterns, percentages, item counts
    SAFE_CMD=$(echo "$CMD" | tr '"' "'" | tr '\\' '/' | head -c 40)
    SAFE_LAST=$(echo "$LAST_LINE" | tr '"' "'" | tr '\\' '/' | head -c 60)

    # Try to extract progress numbers from output
    TOTAL=$(grep -oE '[0-9]+/[0-9]+' "$OUTPUT_FILE" 2>/dev/null | tail -1)
    PCT=$(grep -oE '[0-9]+%' "$OUTPUT_FILE" 2>/dev/null | tail -1)
    ITEMS=$(grep -cE 'processed|generated|done|complete|item|thumbnail|IMG_|sips' "$OUTPUT_FILE" 2>/dev/null)

    # Build a useful progress message
    if [ -n "$TOTAL" ]; then
        PROGRESS="$TOTAL"
    elif [ -n "$PCT" ]; then
        PROGRESS="$PCT"
    elif [ "$ITEMS" -gt 0 ] 2>/dev/null; then
        PROGRESS="$ITEMS items"
    else
        PROGRESS="$LINES lines"
    fi

    printf '{{"from":"system","text":"[progress] %s: %s, elapsed %sm%ss -- %s"}}' "$SAFE_CMD" "$PROGRESS" "$MINS" "$SECS" "$SAFE_LAST" > /tmp/batch-bridge-msg-$$.json
    /usr/bin/curl -s -X POST http://localhost:3470/api/message \
      -H 'Content-Type: application/json' \
      -d @/tmp/batch-bridge-msg-$$.json \
      --connect-timeout 2 > /dev/null 2>&1

    /bin/bash {chorus_log} batch.progress "$ROLE" cmd="$CMD" elapsed="${{MINS}}m${{SECS}}s" lines="$LINES" 2>/dev/null

    # Completion keyword detection
    if grep -qE 'completed|finished|error|failed' "$OUTPUT_FILE" 2>/dev/null; then
        printf '{{"from":"system","text":"[batch-complete] %s: %s lines in %sm%ss"}}' "$SAFE_CMD" "$LINES" "$MINS" "$SECS" > /tmp/batch-bridge-msg-$$.json
        /usr/bin/curl -s -X POST http://localhost:3470/api/message \
          -H 'Content-Type: application/json' \
          -d @/tmp/batch-bridge-msg-$$.json \
          --connect-timeout 2 > /dev/null 2>&1
        /bin/bash {chorus_log} batch.progress.complete "$ROLE" cmd="$CMD" elapsed="${{MINS}}m${{SECS}}s" lines="$LINES" 2>/dev/null
        break
    fi

    if [ $ELAPSED -gt 1800 ]; then
        break
    fi
done
rm -f "$0" /tmp/batch-pre-marker-{rust_pid}.txt /tmp/batch-bridge-msg-$$.json
"#,
        role = role.as_str(),
        cmd_escaped = cmd_short.replace('"', r#"\""#).replace('$', r"\$"),
        chorus_log = CHORUS_LOG,
        rust_pid = std::process::id(),
    );

    // Create a marker file so the monitor can find output files newer than this moment
    let marker = format!("/tmp/batch-pre-marker-{}.txt", std::process::id());
    let _ = std::fs::write(&marker, "");

    let monitor_path = format!("/tmp/batch-monitor-pre-{}.sh", std::process::id());
    match std::fs::write(&monitor_path, &monitor_script) {
        Ok(_) => {
            match Command::new("/bin/bash")
                .arg(&monitor_path)
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    info!(pid = child.id(), "batch-progress-pre: monitor spawned");
                }
                Err(e) => {
                    warn!("batch-progress-pre: spawn failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("batch-progress-pre: script write failed: {}", e);
        }
    }

    // Return allow — don't block the command, just track it
    HookResponse::allow()
}

/// PostToolUse: handle completed background tasks (fires when task finishes)
pub async fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }

    // Only trigger on run_in_background=true
    let run_bg = input
        .tool_input
        .as_ref()
        .and_then(|v| v.get("run_in_background"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !run_bg {
        return HookResponse::allow();
    }

    // Debug: dump what PostToolUse actually receives for run_in_background
    let debug_response = input.tool_response_str();
    let debug_input = format!("{:?}", input.tool_input);
    let _ = std::fs::write("/tmp/batch-posttool-debug.txt",
        format!("run_bg={}\nresponse_len={}\nresponse={}\ninput={}\n",
            run_bg, debug_response.len(),
            &debug_response[..debug_response.len().min(500)],
            &debug_input[..debug_input.len().min(500)]));

    let command = input.get_tool_input_str("command");
    let cmd_short: String = command
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();

    let role = input.role();

    // Extract output file and task ID from tool response
    let response = input.tool_response_str();
    let output_file = extract_output_file(&response);
    let task_id = extract_task_id(&response);

    if output_file.is_empty() {
        info!("batch-progress: background job detected but no output file found");
        return HookResponse::warn_stderr(&format!(
            "<team-scan>\n[batch-progress] Background job started: {}. No output file — progress unavailable.\n</team-scan>",
            cmd_short
        ));
    }

    info!(
        role = role.as_str(),
        task = %task_id,
        cmd = %cmd_short,
        output = %output_file,
        "batch-progress: spawning monitor"
    );

    // Monitor script: tails output file every 30s, posts to Bridge + chorus log
    let monitor_script = format!(
        r#"#!/bin/bash
OUTPUT_FILE="{output_file}"
TASK_ID="{task_id}"
CMD="{cmd_escaped}"
ROLE="{role}"
START=$(date +%s)
INTERVAL=30

while true; do
    sleep $INTERVAL

    if [ ! -f "$OUTPUT_FILE" ]; then
        break
    fi

    ELAPSED=$(( $(date +%s) - START ))
    MINS=$(( ELAPSED / 60 ))
    SECS=$(( ELAPSED % 60 ))

    LAST_LINE=$(tail -1 "$OUTPUT_FILE" 2>/dev/null | head -c 120)
    LINES=$(wc -l < "$OUTPUT_FILE" 2>/dev/null | tr -d ' ')

    # Post to Bridge so Jeff sees it
    /usr/bin/curl -s -X POST http://localhost:3470/api/message \
      -H 'Content-Type: application/json' \
      -d "{{\\"from\\": \\"system\\", \\"text\\": \\"[batch] $CMD -- $LINES lines, ${{MINS}}m${{SECS}}s -- $LAST_LINE\\"}}" \
      --connect-timeout 2 > /dev/null 2>&1

    # Emit to chorus log
    /bin/bash {chorus_log} batch.progress "$ROLE" task="$TASK_ID" elapsed="${{MINS}}m${{SECS}}s" lines="$LINES" 2>/dev/null

    # Check for completion markers
    if grep -q "completed\|finished\|done\|error\|failed" "$OUTPUT_FILE" 2>/dev/null; then
        /bin/bash {chorus_log} batch.progress.complete "$ROLE" task="$TASK_ID" elapsed="${{MINS}}m${{SECS}}s" lines="$LINES" 2>/dev/null
        /usr/bin/curl -s -X POST http://localhost:3470/api/message \
          -H 'Content-Type: application/json' \
          -d "{{\\"from\\": \\"system\\", \\"text\\": \\"[batch-complete] $CMD -- $LINES lines in ${{MINS}}m${{SECS}}s\\"}}" \
          --connect-timeout 2 > /dev/null 2>&1
        break
    fi

    # Safety: stop after 30 minutes
    if [ $ELAPSED -gt 1800 ]; then
        /bin/bash {chorus_log} batch.progress.timeout "$ROLE" task="$TASK_ID" elapsed="${{MINS}}m${{SECS}}s" 2>/dev/null
        break
    fi
done
rm -f "$0" /tmp/batch-bridge-msg-$$.json
"#,
        output_file = output_file,
        task_id = task_id,
        cmd_escaped = cmd_short.replace('"', r#"\""#).replace('$', r"\$"),
        role = role.as_str(),
        chorus_log = CHORUS_LOG,
    );

    let monitor_path = format!("/tmp/batch-monitor-{}.sh", task_id);

    // Dedup guard: skip if a monitor for this task is already running
    let already_running = Command::new("pgrep")
        .args(["-f", &format!("batch-monitor-{}", task_id)])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if already_running {
        info!(task = %task_id, "batch-progress: monitor already running, skipping spawn");
        return HookResponse::allow();
    }

    match std::fs::write(&monitor_path, &monitor_script) {
        Ok(_) => {
            match Command::new("/bin/bash")
                .arg(&monitor_path)
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    info!(pid = child.id(), path = %monitor_path, "batch-progress: monitor spawned");
                }
                Err(e) => {
                    warn!("batch-progress: spawn failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("batch-progress: script write failed: {}", e);
        }
    }

    HookResponse::warn_stderr(&format!(
        "<team-scan>\n[batch-progress] Background job tracked: {} (task {}). Progress every 30s on Bridge.\n</team-scan>",
        cmd_short, task_id
    ))
}

fn extract_output_file(response: &str) -> String {
    response
        .lines()
        .find(|l| l.contains("Output is being written to:") || l.contains("output-file"))
        .and_then(|l| {
            if l.contains("Output is being written to:") {
                l.split("Output is being written to:").nth(1).map(|s| s.trim().to_string())
            } else {
                l.split("<output-file>")
                    .nth(1)
                    .and_then(|s| s.split("</output-file>").next())
                    .map(|s| s.trim().to_string())
            }
        })
        .unwrap_or_default()
}

fn extract_task_id(response: &str) -> String {
    response
        .lines()
        .find(|l| l.contains("ID:") || l.contains("task-id"))
        .and_then(|l| {
            if l.contains("task-id") {
                l.split("<task-id>")
                    .nth(1)
                    .and_then(|s| s.split("</task-id>").next())
                    .map(|s| s.trim().to_string())
            } else {
                l.split("ID:")
                    .nth(1)
                    .map(|s| s.trim().split_whitespace().next().unwrap_or("unknown").to_string())
            }
        })
        .unwrap_or_else(|| format!("{}", std::process::id()))
}
