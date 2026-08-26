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


/// #4004 — INHERITANCE. A subagent is spawned by a role but gets none of its
/// environment, so `resolve_role_from` correctly answers `no-env` and the beat
/// is stamped unattributed at birth. Measured 2026-08-25: 122 of the last 300
/// `agent.activity` events were unknown, all subagent sessions — 41% of our own
/// activity both unattributable AND ungoverned, because every guard that decides
/// what an agent may do keys on this same role.
///
/// A subagent is not anonymous, it acts FOR whoever spawned it. So when the
/// environment is silent, walk the process ancestry and take the role of the
/// nearest ancestor that holds a registered session. The answer is marked
/// INHERITED so it can never be read as a first-hand declaration.
///
/// Pure: the caller supplies the ancestry and the registry lookup, so the suite
/// brings its own world (#3528) — no processes, no ~/.chorus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRole {
    pub role: String,
    pub inherited_from_pid: Option<u32>,
}

pub fn resolve_role_with_ancestry(
    chorus_role: Option<String>,
    deploy_role: Option<String>,
    cwd: &str,
    ancestry: &[u32],
    session_role_of: &dyn Fn(u32) -> Option<String>,
) -> Result<ResolvedRole, RoleUnresolved> {
    match resolve_role_from(chorus_role, deploy_role, cwd) {
        Ok(role) => Ok(ResolvedRole { role, inherited_from_pid: None }),
        // A variable holding a NON-role is a caller bug, not an absent spawner:
        // inheriting there would paper over a typo with someone else's identity.
        Err(e @ RoleUnresolved::NotARole { .. }) => Err(e),
        Err(no_env) => {
            for pid in ancestry {
                if let Some(role) = session_role_of(*pid) {
                    if matches!(role.as_str(), "wren" | "silas" | "kade") {
                        return Ok(ResolvedRole { role, inherited_from_pid: Some(*pid) });
                    }
                }
            }
            Err(no_env)
        }
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


#[cfg(test)]
mod subagent_inheritance_tests_4004 {
    use super::*;

    const CWD: &str = "/private/tmp/agent-scratch";
    fn s(v: &str) -> Option<String> { Some(v.to_string()) }
    /// pid 900 is a registered silas session; 901 is its child; 700 is unregistered.
    fn registry(pid: u32) -> Option<String> {
        match pid { 900 => Some("silas".into()), 950 => Some("wren".into()), _ => None }
    }

    /// NEGATIVE PROOF — the measured case: a subagent with NO env, in no werk,
    /// in no session of its own. Before this it resolved to no-env and every beat
    /// it emitted was unattributable (122 of 300 events on 2026-08-25).
    #[test]
    fn a_subagent_inherits_the_role_that_spawned_it() {
        let r = resolve_role_with_ancestry(None, None, CWD, &[701, 900, 1], &registry)
            .expect("an ancestor holds a session; the child is not anonymous");
        assert_eq!(r.role, "silas");
        assert_eq!(r.inherited_from_pid, Some(900), "inheritance must name its source");
    }

    #[test]
    fn the_nearest_registered_ancestor_wins() {
        // walking outward, silas is closer than wren — a nested spawn belongs to
        // the role that actually launched it, not the outermost session.
        let r = resolve_role_with_ancestry(None, None, CWD, &[701, 900, 950, 1], &registry).unwrap();
        assert_eq!(r.role, "silas");
    }

    /// NEGATIVE PROOF — inheritance must not paper over the OTHER failure. A
    /// variable holding a non-role is a caller bug; silently adopting a parent's
    /// identity there would hand a typo someone else's authority.
    #[test]
    fn a_bogus_role_value_still_refuses_even_with_a_valid_ancestor() {
        let err = resolve_role_with_ancestry(s("system"), None, CWD, &[900], &registry)
            .expect_err("a named-but-invalid role is not an absent one");
        assert_eq!(err.reason(), "not-a-role");
    }

    /// NEGATIVE PROOF — no ancestor holds a session: still REFUSE with a reason.
    /// Inheritance adds a source of truth; it must not become a new way to invent
    /// a name (the exact failure #3959 ended).
    #[test]
    fn no_registered_ancestor_still_refuses_and_names_why() {
        let err = resolve_role_with_ancestry(None, None, CWD, &[701, 702, 1], &registry)
            .expect_err("nothing in the ancestry can name this caller");
        assert_eq!(err.reason(), "no-env");
    }

    #[test]
    fn a_direct_declaration_is_never_marked_inherited() {
        let r = resolve_role_with_ancestry(s("kade"), None, CWD, &[900], &registry).unwrap();
        assert_eq!(r.role, "kade");
        assert_eq!(r.inherited_from_pid, None);
    }
}


/// #4004 — the live wiring for `resolve_role_with_ancestry`: walk this process's
/// real ancestry and look each pid up in the session registry
/// (`~/.chorus/sessions/<role>-<pid>.json`). Best-effort by construction — a
/// missing registry or an unreadable `ps` returns None, which leaves the caller
/// exactly where it was rather than inventing a name.
pub fn inherited_role_from_ancestry() -> Option<String> {
    let ancestry = process_ancestry(std::process::id(), 12);
    let r = resolve_role_with_ancestry(None, None, "", &ancestry, &session_role_of_pid).ok()?;
    r.inherited_from_pid.map(|_| r.role)
}

/// Registered session for this pid, if any. The registry names the owner in the
/// filename, so no parse of the body is needed to answer "whose session is this".
pub fn session_role_of_pid(pid: u32) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::Path::new(&home).join(".chorus").join("sessions");
    for role in ["wren", "silas", "kade"] {
        if dir.join(format!("{role}-{pid}.json")).exists() {
            return Some(role.to_string());
        }
    }
    None
}

/// Parent chain, nearest first, bounded so a cycle or a deep tree cannot hang a
/// hook call (a stalled hook fails the whole team closed — the #3218 shape).
pub fn process_ancestry(start: u32, max_depth: usize) -> Vec<u32> {
    let mut out = Vec::new();
    let mut pid = start;
    for _ in 0..max_depth {
        let Ok(o) = std::process::Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output() else { break };
        let Ok(txt) = String::from_utf8(o.stdout) else { break };
        let Ok(ppid) = txt.trim().parse::<u32>() else { break };
        if ppid <= 1 { break; }
        out.push(ppid);
        pid = ppid;
    }
    out
}
