// @domain: cicd — placed by chorus-crawl's domain rules (#4538)
// #4771 AC4 — the land's ONE GitHub credential, shared by every werk verb that
// talks to GitHub during a land (werk-merge, werk-sync, werk-teardown).
//
// On 11-02 06:02 #4642's land stopped on `HTTP 401: Requires authentication`:
// the verbs ran `gh` and `git` with no environment, so both used Jeff's own gh
// keyring and macOS keychain, and his login had lapsed. Now: one file,
// ~/.chorus/secrets/github-deploy of the account the land runs as (the runner
// and dagu run as that account), mode 0600 and owned by it. gh gets it
// through GH_TOKEN; git through an inline credential helper that reads the file
// at the moment git asks, after every other helper is cleared. Never argv,
// never logged, never anyone's keyring. Jeff mints it and writes the file.
//
// BOUNDARY (shared/, #3513): pure, dep-free, single-file include!. Every path
// is fully qualified so the including crate's `use` lines never collide.

/// Under the running account's HOME on purpose: the file must be OWNED by the
/// account the land runs as (check_credential_file), so that account's home is
/// where it lives. Unlike dagu's shared data (#4771 AC3), nothing else reads it.
pub const DEPLOY_CREDENTIAL_REL: &str = ".chorus/secrets/github-deploy";
pub const CRED_FILE_ENV: &str = "WERK_DEPLOY_CREDENTIAL_FILE";

/// git runs this through sh for `credential fill`; it answers `get` only, with
/// the fixed token user and the file's contents, read when git asks.
pub const GIT_CRED_HELPER: &str =
    "!f() { test \"$1\" = get || exit 0; printf 'username=x-access-token\\npassword=%s\\n' \"$(cat \"$WERK_DEPLOY_CREDENTIAL_FILE\")\"; }; f";

/// Tests that set CRED_FILE_ENV take this lock: the env is process-wide and
/// these tests run in parallel inside each crate that includes this file.
pub static CRED_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn current_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

/// The credential file's path: $WERK_DEPLOY_CREDENTIAL_FILE, else
/// $HOME/.chorus/secrets/github-deploy of the running account.
pub fn deploy_credential_path() -> Result<std::path::PathBuf, String> {
    if let Some(p) = std::env::var_os(CRED_FILE_ENV).filter(|p| !p.is_empty()) {
        return Ok(std::path::PathBuf::from(p));
    }
    std::env::var_os("HOME").filter(|h| !h.is_empty())
        .map(|h| std::path::PathBuf::from(h).join(DEPLOY_CREDENTIAL_REL))
        .ok_or_else(|| format!("auth-fail: no HOME to find ~/{} in", DEPLOY_CREDENTIAL_REL))
}

/// Exists, non-empty regular file, mode 0600 (nothing for group/other), owned
/// by `uid`. Errors name the PATH only, never the contents.
pub fn check_credential_file(p: &std::path::Path, uid: u32) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(p)
        .map_err(|e| format!("auth-fail: deploy credential file {} unreadable: {}", p.display(), e))?;
    if !m.is_file() || m.len() == 0 {
        return Err(format!("auth-fail: deploy credential file {} is empty or not a file", p.display()));
    }
    if m.mode() & 0o077 != 0 {
        return Err(format!("auth-fail: deploy credential file {} is mode {:o}; it must be 0600", p.display(), m.mode() & 0o777));
    }
    if m.uid() != uid {
        return Err(format!("auth-fail: deploy credential file {} is owned by uid {}; the land runs as uid {}", p.display(), m.uid(), uid));
    }
    Ok(())
}

/// Resolve + check, then arm it for every later gh/git call of this process.
pub fn arm_deploy_credential() -> Result<std::path::PathBuf, String> {
    let p = deploy_credential_path()?;
    check_credential_file(&p, current_uid())?;
    std::env::set_var(CRED_FILE_ENV, &p);
    Ok(p)
}

/// The armed file, if any.
pub fn armed_credential() -> Option<std::ffi::OsString> {
    std::env::var_os(CRED_FILE_ENV).filter(|p| !p.is_empty())
}

/// Call right after `Command::new(tool)`, before any argument: git never
/// prompts and never reaches a keyring (every helper cleared); once armed it
/// gets the deploy credential. gh gets GH_TOKEN once armed.
pub fn with_deploy_credential(c: &mut std::process::Command, tool: &str) {
    match tool {
        "git" => {
            c.env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "never").env_remove("GIT_ASKPASS").env_remove("SSH_ASKPASS");
            c.args(["-c", "credential.helper="]);
            if let Some(p) = armed_credential() {
                c.env(CRED_FILE_ENV, p).args(["-c", &format!("credential.helper={}", GIT_CRED_HELPER)]);
            }
        }
        "gh" => {
            c.env("GH_PROMPT_DISABLED", "1");
            if let Some(t) = armed_credential().and_then(|p| std::fs::read_to_string(p).ok()) {
                c.env("GH_TOKEN", t.trim());
            }
        }
        _ => {}
    }
}

/// A 401 or a missing login from gh or git.
pub fn is_auth_error(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    ["http 401", "requires authentication", "authentication failed", "bad credentials",
     "could not read username", "could not read password", "gh auth login"]
        .iter().any(|k| s.contains(k))
}

/// The refusal for an auth failure: names the armed file's path, never its contents.
pub fn auth_fail_message(stderr: &str) -> String {
    let file = armed_credential().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "(none armed)".to_string());
    format!("auth-fail: GitHub refused the deploy credential in {}: {}", file, stderr.lines().next().unwrap_or(""))
}

#[cfg(test)]
mod deploy_credential_4771 {
    use super::*;

    fn cred_file(tag: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let f = std::env::temp_dir().join(format!("dc4771-{}-{}-{}", tag, std::process::id(), line!()));
        std::fs::write(&f, body).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        f
    }

    /// What git itself would send GitHub, asked through `git credential fill`.
    fn git_fill(armed: Option<&std::path::Path>) -> (bool, String) {
        use std::io::Write;
        let _g = CRED_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut c = std::process::Command::new("git");
        match armed {
            Some(p) => { std::env::set_var(CRED_FILE_ENV, p); }
            None => { std::env::remove_var(CRED_FILE_ENV); }
        }
        with_deploy_credential(&mut c, "git");
        std::env::remove_var(CRED_FILE_ENV);
        let mut child = c.args(["credential", "fill"])
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .spawn().unwrap();
        child.stdin.take().unwrap().write_all(b"protocol=https\nhost=github.com\n\n").unwrap();
        let out = child.wait_with_output().unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[test]
    fn armed_git_answers_github_with_the_deploy_credential() {
        let f = cred_file("fill", "tok-4771\n");
        let (ok, out) = git_fill(Some(&f));
        assert!(ok, "{out}");
        assert!(out.contains("username=x-access-token") && out.contains("password=tok-4771"), "{out}");
        let _ = std::fs::remove_file(&f);
    }

    // NEGATIVE PROOF: unarmed, git has no credential at all: no keyring, no
    // prompt, so it fails instead of using Jeff's login.
    #[test]
    fn unarmed_git_has_no_credential_and_never_prompts() {
        let (ok, out) = git_fill(None);
        assert!(!ok, "git found a credential with every helper cleared: {out}");
        assert!(!out.contains("password="), "{out}");
    }

    #[test]
    fn the_0602_401_is_an_auth_failure_and_a_conflict_is_not() {
        assert!(is_auth_error("HTTP 401: Requires authentication (https://api.github.com/graphql)"));
        assert!(is_auth_error("fatal: Authentication failed for 'https://github.com/x/y.git/'"));
        assert!(is_auth_error("fatal: could not read Username for 'https://github.com': terminal prompts disabled"));
        assert!(!is_auth_error("Pull request is not mergeable: merge conflict"));
    }

    #[test]
    fn a_credential_file_open_or_owned_by_another_account_is_refused_by_path() {
        use std::os::unix::fs::PermissionsExt;
        let f = cred_file("own", "tok");
        let me = current_uid();
        assert!(check_credential_file(&f, me).is_ok());
        let err = check_credential_file(&f, me + 1).unwrap_err();
        assert!(err.contains("owned by uid") && err.contains(f.to_str().unwrap()), "{err}");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(check_credential_file(&f, me).unwrap_err().contains("must be 0600"));
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&f, "").unwrap();
        assert!(check_credential_file(&f, me).unwrap_err().contains("empty"));
        let _ = std::fs::remove_file(&f);
    }
}
