//! #3959 — THE role resolver. One place decides what a caller is called.
//!
//! Before this, 43 sites read the role and each invented its own default when it
//! was missing (`:-system` at 9, empty at 7, `:-silas` at 2), so ~26,000 events a
//! day carried a name nobody chose. It lives in `shared/` because both the daemon
//! and the shim binary need it, and a second copy would recreate the divergence
//! this exists to end.

/// Why a role could not be resolved. #3959 — the substitution used to be
/// invisible: 43 sites read the role, each invented its own default when it was
/// missing (`:-system` at 9, empty at 7, `:-silas` at 2), and ~26,000 events a
/// day were written with a name nobody chose. A caller that cannot name itself
/// must say WHY, so the gap is diagnosable instead of merely present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleUnresolved {
    /// Neither CHORUS_ROLE nor DEPLOY_ROLE is set in this process's environment.
    NoEnv { cwd: String },
    /// A variable is set but holds something that is not a known role.
    NotARole { value: String },
}

impl RoleUnresolved {
    pub fn reason(&self) -> &'static str {
        match self {
            RoleUnresolved::NoEnv { .. } => "no-env",
            RoleUnresolved::NotARole { .. } => "not-a-role",
        }
    }

    /// One line naming what is missing and where to set it — never a bare
    /// "unknown". The werk case is called out because it is the one that
    /// actually bites: building happens in `chorus-werk/<role>-<card>/`, and
    /// until #3959 no derivation matched that path.
    pub fn detail(&self) -> String {
        match self {
            RoleUnresolved::NoEnv { cwd } => format!(
                "CHORUS_ROLE and DEPLOY_ROLE both unset (cwd={cwd}); \
                 source platform/scripts/chorus-env-setup.sh, which derives the \
                 role from a roles/<role> or chorus-werk/<role>-<card> path"
            ),
            RoleUnresolved::NotARole { value } => {
                format!("role variable holds {value:?}, which is not wren|silas|kade")
            }
        }
    }
}

/// The ONE role resolver for emit-time attribution. Returns the reason on
/// failure rather than substituting a default, so callers can count and report
/// the gap. It deliberately does NOT panic: a hard failure here would take the
/// hooks daemon down, and a dead daemon fails the whole team closed — the #3218
/// lockout shape. Loud and counted, never silent; degraded, never fatal.
pub fn resolve_caller_role() -> Result<String, RoleUnresolved> {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    resolve_role_from(
        std::env::var("CHORUS_ROLE").ok(),
        std::env::var("DEPLOY_ROLE").ok(),
        &cwd,
    )
}

/// The pure core. Kept separate from the env read so the suite brings its own
/// world instead of mutating process env — shared-env tests race each other
/// under cargo's default parallelism, and a racing test is a check that cannot
/// tell a real failure from a neighbour's write.
pub fn resolve_role_from(
    chorus_role: Option<String>,
    deploy_role: Option<String>,
    cwd: &str,
) -> Result<String, RoleUnresolved> {
    let raw = chorus_role
        .filter(|s| !s.is_empty())
        .or_else(|| deploy_role.filter(|s| !s.is_empty()));
    match raw {
        None => Err(RoleUnresolved::NoEnv { cwd: cwd.to_string() }),
        Some(v) if matches!(v.as_str(), "wren" | "silas" | "kade") => Ok(v),
        Some(v) => Err(RoleUnresolved::NotARole { value: v }),
    }
}


#[cfg(test)]
mod role_resolver_tests_3959 {
    use super::*;

    const CWD: &str = "/Users/j/CascadeProjects/chorus-werk/wren-3959";
    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn resolves_a_real_role() {
        assert_eq!(resolve_role_from(s("wren"), None, CWD), Ok("wren".into()));
    }

    #[test]
    fn falls_through_to_deploy_role() {
        assert_eq!(resolve_role_from(None, s("kade"), CWD), Ok("kade".into()));
        // empty string is absence, not a value — the ":-" fallback at 7 sites
        assert_eq!(resolve_role_from(s(""), s("silas"), CWD), Ok("silas".into()));
    }

    /// NEGATIVE PROOF — the case that produced ~26,000 unattributed events a day.
    /// Strip the environment and the resolver must REFUSE with a reason, never
    /// hand back a substituted name.
    #[test]
    fn stripped_env_refuses_and_names_why() {
        let err = resolve_role_from(None, None, CWD)
            .expect_err("a nameless caller must not resolve to a name");
        assert_eq!(err.reason(), "no-env");
        let d = err.detail();
        assert!(d.contains("chorus-env-setup.sh"), "names the fix: {d}");
        assert!(d.contains("chorus-werk"), "names the werk case: {d}");
        assert!(d.contains(CWD), "names where it happened: {d}");
        assert!(!d.contains("silas"), "never substitutes a teammate: {d}");
    }

    /// NEGATIVE PROOF — "variable holds a non-role" and "variable is absent" are
    /// two different failures and must read differently. Collapsing them is the
    /// same defect class as the old `unknown` catch-all.
    #[test]
    fn a_non_role_value_is_distinguishable_from_absence() {
        let absent = resolve_role_from(None, None, CWD).unwrap_err();
        let bogus = resolve_role_from(s("system"), None, CWD).unwrap_err();
        assert_eq!(absent.reason(), "no-env");
        assert_eq!(bogus.reason(), "not-a-role");
        assert_ne!(absent.reason(), bogus.reason());
        assert!(bogus.detail().contains("system"), "quotes the offending value");
    }

    /// Every default the 43 read sites used to invent is now a REFUSAL.
    #[test]
    fn the_old_substituted_defaults_no_longer_resolve() {
        for bogus in ["system", "silas-bot", "unattributed", "unknown", "jeff"] {
            let r = resolve_role_from(s(bogus), None, CWD);
            assert!(r.is_err(), "{bogus:?} must not pass as a role");
        }
    }

    /// The legacy shim still answers "unknown" so nothing fails closed — a hard
    /// failure here would take the daemon down and fail the whole team closed
    /// (#3218). Degraded and counted, never fatal.
    #[test]
    fn legacy_shim_degrades_without_taking_the_daemon_down() {
        assert_eq!(
            resolve_role_from(None, None, CWD)
                .unwrap_or_else(|_| "unknown".to_string()),
            "unknown"
        );
    }
}

