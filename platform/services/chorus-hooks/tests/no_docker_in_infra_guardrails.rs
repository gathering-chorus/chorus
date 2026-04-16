//! #2119 — verify Docker guards are fully purged from infra_guardrails.rs.
//!
//! Post #2020, no service runs Docker. The guards were defensive rails
//! against commands that have no target. Keeping them signals "Docker
//! is still a thing." This test locks in the deletion.

#[test]
fn infra_guardrails_source_has_no_docker_refs() {
    let src = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/hooks/infra_guardrails.rs"
    ).expect("infra_guardrails.rs should exist");

    // Case-insensitive scan — catches DOCKER_*_RE declarations, "docker " command patterns,
    // and "Docker is not used" messages alike.
    let lower = src.to_lowercase();
    assert!(
        !lower.contains("docker"),
        "infra_guardrails.rs must not contain any 'docker' references after #2119.\n\
         Found references — delete the DOCKER_* regex declarations, usage blocks, and test cases."
    );
}
