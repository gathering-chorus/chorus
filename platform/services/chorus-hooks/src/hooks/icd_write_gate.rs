//! ICD Write Gate — Athena pattern (#1648)
//!
//! PostToolUse hook on writes to icd-instance-*.ttl or icd-ontology.ttl.
//! After any ICD file write: validate TTL → reload Fuseki → run linter.
//! The write IS the validation. No human memory in the loop.
//!
//! Prior art: Staples Athena — save triggered validation automatically.

use crate::state::AppState;
use crate::types::HookInput;
use std::process::Command;
use tracing::{info, warn};

/// Check if the written file is an ICD file and run validation pipeline
pub async fn check(input: &HookInput, _state: &AppState) {
    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return;
    }

    // Only trigger on ICD files
    let is_icd = file_path.contains("icd-instance-") || file_path.contains("icd-ontology");
    let is_ttl = file_path.ends_with(".ttl");
    if !is_icd || !is_ttl {
        return;
    }

    info!(file = %file_path, "ICD write detected — running validation pipeline");

    // Step 1: Validate TTL
    let validate = Command::new("npx")
        .args(["-y", "turtle-validator", &file_path])
        .output();

    match validate {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(file = %file_path, "ICD TTL validation FAILED: {}", stderr);
            eprintln!("\n<team-scan>\n[icd-gate] TTL validation FAILED for {}: {}\n</team-scan>\n",
                file_path.rsplit('/').next().unwrap_or(&file_path), stderr.trim());
            return;
        }
        Err(e) => {
            warn!("turtle-validator not available: {}", e);
            // Continue — don't block on missing validator
        }
        _ => {
            info!("TTL validation passed");
        }
    }

    // Step 2: Reload Fuseki ICD graph
    let app_dir = "/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site";
    let ontology_dir = format!("{}/src/ontology", app_dir);

    // Drop current graph
    let _ = Command::new("curl")
        .args(["-s", "-X", "DELETE",
            "http://localhost:3030/pods/data?graph=urn:gathering:icd/current"])
        .output();

    // Load ontology
    let ontology_path = format!("{}/icd-ontology.ttl", ontology_dir);
    let _ = Command::new("curl")
        .args(["-s", "-X", "POST",
            "http://localhost:3030/pods/data?graph=urn:gathering:icd/current",
            "-H", "Content-Type: text/turtle",
            "--data-binary", &format!("@{}", ontology_path)])
        .output();

    // Load all instance files
    if let Ok(entries) = std::fs::read_dir(&ontology_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with("icd-instance-") && name.ends_with(".ttl") {
                let _ = Command::new("curl")
                    .args(["-s", "-X", "POST",
                        "http://localhost:3030/pods/data?graph=urn:gathering:icd/current",
                        "-H", "Content-Type: text/turtle",
                        "--data-binary", &format!("@{}", path.display())])
                    .output();
            }
        }
    }

    info!("Fuseki ICD graph reloaded");

    // Step 3: Run linter
    let lint = Command::new("python3")
        .args(["scripts/icd-lint-sparql.py"])
        .current_dir(app_dir)
        .output();

    match lint {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Strip ANSI codes for clean output
            let clean: String = stdout.chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .collect();

            if output.status.success() {
                info!("ICD lint passed");
                eprintln!("\n<team-scan>\n[icd-gate] ICD validated + reloaded + lint PASSED\n</team-scan>\n");
            } else {
                warn!("ICD lint FAILED");
                // Extract just the error lines
                let errors: Vec<&str> = clean.lines()
                    .filter(|l| l.contains("ERROR") || l.contains("FAIL") || l.contains("RESULT"))
                    .collect();
                eprintln!("\n<team-scan>\n[icd-gate] ICD lint FAILED after reload:\n{}\n</team-scan>\n",
                    errors.join("\n"));
            }
        }
        Err(e) => {
            warn!("ICD linter not available: {}", e);
        }
    }
}
