//! Unified file classification for chorus-hooks (#2076).
//! Replaces 4 separate is_code_file() definitions across hooks.

/// Source code files that carry cross-domain risk and trigger gates.
/// Used by: pair_gate, log_first_gate, tdd_gate
pub fn is_source_code(path: &str) -> bool {
    let source_exts = [".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".sh"];
    source_exts.iter().any(|ext| path.ends_with(ext))
        // HTML outside public dirs is source (templates, app pages)
        || (path.ends_with(".html") && !path.contains("/public/") && !path.starts_with("public/") && !path.contains("gathering-docs") && !path.contains("/artifacts/"))
}

/// Presentation/config files — editable without pair, but still tracked.
/// Used by: memory_gate (broader scope)
pub fn is_project_file(path: &str) -> bool {
    is_source_code(path)
        || is_style_file(path)
        || is_config_file(path)
        || is_template_file(path)
}

/// CSS/SCSS — exempt from pair gate (#2062)
pub fn is_style_file(path: &str) -> bool {
    path.ends_with(".css") || path.ends_with(".scss")
}

/// Templates — exempt from pair gate
pub fn is_template_file(path: &str) -> bool {
    path.ends_with(".ejs")
}

/// Config files
pub fn is_config_file(path: &str) -> bool {
    let exts = [".json", ".toml", ".yaml", ".yml"];
    exts.iter().any(|ext| path.ends_with(ext))
}

/// Static assets in public directories — exempt from pair gate (#2062)
pub fn is_static_asset(path: &str) -> bool {
    path.contains("/public/") && (path.ends_with(".html") || is_style_file(path))
}

/// Test files — excluded from TDD production code check
pub fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("/tests/") || lower.contains("/test/")
        || lower.starts_with("tests/") || lower.starts_with("test/")
        || lower.contains(".test.") || lower.contains(".spec.")
        || lower.contains("_test.") || lower.contains("test_")
        || lower.ends_with("_test.rs") || lower.ends_with("_test.ts")
        || lower.contains(".feature")
}

/// Production code — source code minus test files and build artifacts.
/// Used by: tdd_gate
pub fn is_production_code(path: &str) -> bool {
    let lower = path.to_lowercase();
    is_source_code(path) && !is_test_file(path)
        && !lower.contains("/target/") && !lower.starts_with("target/")
        && !lower.contains("/node_modules/") && !lower.starts_with("node_modules/")
        && !lower.contains("/dist/")
}

/// Skip generated/build paths — used by pair_gate and others
pub fn is_generated_path(path: &str) -> bool {
    path.contains("/target/") || path.contains("/node_modules/") || path.contains("/dist/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_code_detection() {
        assert!(is_source_code("src/main.rs"));
        assert!(is_source_code("handlers/music.handler.ts"));
        assert!(is_source_code("scripts/deploy.sh"));
        assert!(!is_source_code("styles/main.css"));
        assert!(!is_source_code("views/layout.ejs"));
    }

    #[test]
    fn static_assets_exempt() {
        assert!(is_static_asset("clearing/public/index.html"));
        assert!(is_static_asset("app/public/styles.css"));
        assert!(!is_static_asset("src/views/index.html"));
    }

    #[test]
    fn html_classification() {
        // HTML in public = static asset, not source
        assert!(!is_source_code("clearing/public/index.html"));
        // HTML outside public = source (app template)
        assert!(is_source_code("src/views/index.html"));
    }

    #[test]
    fn production_code_excludes_tests() {
        assert!(is_production_code("src/nudge.rs"));
        assert!(!is_production_code("tests/nudge_test.rs"));
        assert!(!is_production_code("src/handlers/music.handler.test.ts"));
        assert!(!is_production_code("target/release/chorus-hooks"));
    }

    #[test]
    fn gathering_docs_html_excluded() {
        // #1695: generated docs in public/gathering-docs/ are not production code
        assert!(!is_production_code("public/gathering-docs/chorus-standards.html"));
        assert!(!is_production_code("/Users/jeff/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/domain-chorus.html"));
        assert!(!is_source_code("public/gathering-docs/pulse-service-design.html"));
        // artifacts/ HTML is also generated, not production code
        assert!(!is_production_code("roles/wren/artifacts/chorus-domain-map-v2.html"));
        assert!(!is_source_code("/Users/jeff/CascadeProjects/chorus/roles/wren/artifacts/team-analysis.html"));
    }

    #[test]
    fn project_file_is_superset() {
        // Source code is a project file
        assert!(is_project_file("src/main.rs"));
        // CSS is a project file but not source
        assert!(is_project_file("styles/main.css"));
        assert!(!is_source_code("styles/main.css"));
        // Config is a project file
        assert!(is_project_file("Cargo.toml"));
    }
}
