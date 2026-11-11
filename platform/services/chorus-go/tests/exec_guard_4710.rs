//! #4710 AC3 (Silas 18:06–18:08) — the lander runs as chorus-lander and holds the App key, so it
//! must execute nothing jeffbridwell or a role can write: swap one such program and it runs as
//! the lander, key readable. Every process chorus-go starts must be named by an absolute path
//! under /usr/bin or /bin (root-owned on macOS) — never a bare name found on PATH, never a path
//! built from an env default (CHORUS_HOME, HOME, a werk).
use std::path::Path;

const ALLOWED: &[&str] = &["/usr/bin/", "/bin/"];

/// Every `Command::new(<arg>)` in the source text and why its program is not allowed, if not.
pub fn exec_problems(src: &str, file: &str) -> Vec<String> {
    let mut out = Vec::new();
    // what runs in the shipped binary: code lines, before the source's unit tests
    let shipped = src.split("#[cfg(test)]").next().unwrap_or(src);
    for (n, line) in shipped.lines().enumerate() {
        if line.trim_start().starts_with("//") { continue; }
        let mut rest = line;
        while let Some(i) = rest.find("Command::new(") {
            let arg = &rest[i + "Command::new(".len()..];
            let end = arg.find(')').unwrap_or(arg.len());
            let a = arg[..end].trim();
            let lit = a.strip_prefix('"').and_then(|x| x.strip_suffix('"'));
            match lit {
                Some(p) if ALLOWED.iter().any(|ok| p.starts_with(ok)) => {}
                Some(p) => out.push(format!("{file}:{}: runs \"{p}\", not an absolute /usr/bin or /bin path", n + 1)),
                None if a.starts_with("LANDER_BIN_") || ALLOWED_CONSTS.contains(&a) => {}
                None => out.push(format!("{file}:{}: runs `{a}`, a program chosen at run time (env, PATH or a path anyone may write)", n + 1)),
            }
            rest = &arg[end..];
        }
    }
    out
}

/// Constants chorus-go uses for its programs; each must itself be an allowed literal (checked below).
const ALLOWED_CONSTS: &[&str] = &["CURL", "OPENSSL", "GIT"];

#[test]
fn chorus_go_runs_only_root_owned_programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = Vec::new();
    for f in ["src/lib.rs", "src/serve.rs", "src/main.rs", "src/github.rs", "../shared/app_credential.rs", "../shared/deploy_credential.rs"] {
        let src = std::fs::read_to_string(dir.join(f)).unwrap();
        problems.extend(exec_problems(&src, f));
        for c in ALLOWED_CONSTS {
            for l in src.lines().filter(|l| l.contains(&format!("const {c}: &str"))) {
                let v = l.split('"').nth(1).unwrap_or("");
                if !ALLOWED.iter().any(|ok| v.starts_with(ok)) { problems.push(format!("{f}: const {c} = \"{v}\" is not under /usr/bin or /bin")); }
            }
        }
    }
    assert!(problems.is_empty(), "the lander would run programs others can write:\n{}", problems.join("\n"));
}

// NEGATIVE PROOFS: a bare name on PATH, an env_or default under CHORUS_HOME, a werk path.
#[test]
fn a_bare_name_an_env_default_and_a_home_path_are_all_caught() {
    assert_eq!(exec_problems(r#"Command::new("/usr/bin/curl")"#, "x").len(), 0);
    assert_eq!(exec_problems(r#"Command::new("curl")"#, "x").len(), 1);
    assert_eq!(exec_problems(r#"Command::new(env_or("CHORUS_GO_CARDS", &format!("{}/platform/scripts/cards", chorus_home())))"#, "x").len(), 1);
    assert_eq!(exec_problems(r#"let o = Command::new(&dagu)"#, "x").len(), 1);
    assert_eq!(exec_problems(r#"Command::new("/Users/jeffbridwell/.chorus/bin/x")"#, "x").len(), 1);
    // a comment or a unit test is not the shipped binary; code after them still is
    assert_eq!(exec_problems("// Command::new(tool)\n#[cfg(test)]\nCommand::new(\"git\")", "x").len(), 0);
    assert_eq!(exec_problems("let a = 1;\nCommand::new(\"git\")\n#[cfg(test)]", "x").len(), 1);
}
