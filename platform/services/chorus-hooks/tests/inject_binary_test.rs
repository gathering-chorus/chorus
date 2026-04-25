use chorus_hooks::shared::state_paths::chorus_root;
#[test]
fn inject_binary_exists() {
    let path = &format!("{}/platform/services/chorus-inject/target/release/chorus-inject", chorus_root());
    assert!(std::path::Path::new(path).exists(), "chorus-inject binary must exist at {}", path);
}

#[test]
fn inject_binary_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let path = &format!("{}/platform/services/chorus-inject/target/release/chorus-inject", chorus_root());
    let meta = std::fs::metadata(path).expect("chorus-inject binary must exist");
    assert!(meta.permissions().mode() & 0o111 != 0, "chorus-inject must be executable");
}
