#[test]
fn inject_binary_exists() {
    let path = "/Users/jeffbridwell/CascadeProjects/platform/services/chorus-hooks/target/release/chorus-inject";
    assert!(std::path::Path::new(path).exists(), "chorus-inject binary must exist");
}
