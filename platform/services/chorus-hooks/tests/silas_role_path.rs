//! #1820: Verify Silas role path is roles/silas, not platform/roles/silas.
//! After DEC-1816 namespace move.

use chorus_hooks::shared::state_paths::chorus_root;
#[test]
fn shim_uses_roles_silas_not_platform_roles_silas() {
    let source = std::fs::read_to_string(
        format!("{}/platform/services/chorus-hooks/src/shim.rs", chorus_root())
    ).expect("shim.rs should exist");

    assert!(
        source.contains("\"silas\" => \"roles/silas\""),
        "shim.rs must map silas to 'roles/silas', not 'platform/roles/silas'"
    );
    assert!(
        !source.contains("\"silas\" => \"platform/roles/silas\""),
        "shim.rs must NOT reference old path 'platform/roles/silas'"
    );
}

#[test]
fn sensitive_paths_uses_roles_silas() {
    let source = std::fs::read_to_string(
        format!("{}/platform/services/chorus-hooks/src/hooks/sensitive_paths.rs", chorus_root())
    ).expect("sensitive_paths.rs should exist");

    assert!(
        !source.contains("platform/roles/silas"),
        "sensitive_paths.rs must NOT reference old path 'platform/roles/silas'"
    );
}

#[test]
fn health_command_uses_roles_silas() {
    let source = std::fs::read_to_string(
        format!("{}/platform/services/chorus-hooks/src/commands/health.rs", chorus_root())
    ).expect("health.rs should exist");

    assert!(
        !source.contains("platform/roles/silas"),
        "health.rs must NOT reference old path 'platform/roles/silas'"
    );
}
