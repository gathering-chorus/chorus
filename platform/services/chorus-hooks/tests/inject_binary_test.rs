#[test]
fn inject_binary_exists() {
    let path = "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-hooks/target/release/chorus-inject";
    assert!(std::path::Path::new(path).exists(), "chorus-inject binary must exist at {}", path);
}

#[test]
fn inject_binary_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let path = "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-hooks/target/release/chorus-inject";
    let meta = std::fs::metadata(path).expect("chorus-inject binary must exist");
    assert!(meta.permissions().mode() & 0o111 != 0, "chorus-inject must be executable");
}
