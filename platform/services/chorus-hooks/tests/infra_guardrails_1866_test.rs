//! #1866 — Remove stale Docker references from infra_guardrails messages
//!
//! AC item 9: infra_guardrails.rs error messages must reference docker-compose
//! for remaining containers, not "app-state.sh deploy" or "Terraform manages Docker".
//!
//! These tests shell out to the running chorus-hooks service via the shim binary.
//! They verify the deny/ask message content after the message update.

use std::process::{Command, Stdio};
use std::io::Write;

const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim";

fn run_hook(tool_name: &str, command: &str) -> String {
    let input = serde_json::json!({
        "tool_name": tool_name,
        "tool_input": { "command": command },
        "cwd": "/Users/jeffbridwell/CascadeProjects/chorus/roles/kade",
        "session_id": "test-1866"
    });

    let mut child = Command::new(SHIM)
        .arg("pre-tool-use")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start shim");

    child.stdin.take().unwrap()
        .write_all(input.to_string().as_bytes())
        .expect("failed to write stdin");

    let output = child.wait_with_output().expect("failed to read output");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// AC 9: docker exec deny must NOT reference "redeploy using app-state.sh deploy"
#[test]
fn docker_exec_message_drops_app_state_deploy() {
    let output = run_hook("Bash", "docker exec -it mycontainer bash");
    assert!(
        !output.contains("redeploy using app-state.sh"),
        "docker exec deny still references app-state.sh deploy: {output}"
    );
}

/// AC 9: docker exec deny should reference docker-compose for remaining containers
#[test]
fn docker_exec_message_references_docker_compose() {
    let output = run_hook("Bash", "docker exec -it mycontainer bash");
    assert!(
        output.contains("docker-compose"),
        "docker exec deny should mention docker-compose: {output}"
    );
}

/// AC 9: terraform deny must NOT say "Terraform is only used for remaining Docker services"
#[test]
fn terraform_message_drops_terraform_manages_docker() {
    let output = run_hook("Bash", "terraform apply");
    assert!(
        !output.contains("Terraform is only used for"),
        "terraform deny still suggests Terraform manages remaining services: {output}"
    );
}

/// AC 9: terraform deny should reference docker-compose
#[test]
fn terraform_message_references_docker_compose() {
    let output = run_hook("Bash", "terraform apply");
    assert!(
        output.contains("docker-compose"),
        "terraform deny should mention docker-compose: {output}"
    );
}
