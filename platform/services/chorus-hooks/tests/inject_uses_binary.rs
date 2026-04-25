use chorus_hooks::shared::state_paths::chorus_root;
#[test]
fn inject_binary_exists() {
    let path = &format!("{}/platform/services/chorus-inject/target/release/chorus-inject", chorus_root());
    assert!(std::path::Path::new(path).exists(), "chorus-inject binary must exist");
}
