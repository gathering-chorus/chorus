//! The test/prod membrane (#3615) — one enforced guard so test and build
//! contexts cannot touch production state.
//!
//! Turns the #3528 "a test brings its own world" discipline into enforcement.
//! Three prior incidents each bought this guardrail privately (#3608 nudge
//! registry, #3381 demo stores, #2491 CI spine paths); Rule of Three says
//! platformize. The canonical surface list lives in
//! `designing/schemas/membrane-surfaces.json` — a drift-guard test below
//! asserts the `Surface` enum matches it 1:1, so renaming either side fails
//! loudly instead of passing vacuously (#3734).
//!
//! Shape: pure core (`context_from`, `resolve_from`) tested without env
//! mutation, thin impure wrappers reading real env — mirrors the
//! `resolve_and_derive` precedent (#3140). Refusal is a panic with a typed
//! `MEMBRANE REFUSED` message: same asymmetric contract as state_paths
//! (#2505/#2565 — prod falls back silently, tests fail explicitly) and the
//! #3631 hook_run_dir precedent (better to refuse than silently write prod).

use std::fmt;

/// Execution context, per the env contract in membrane-surfaces.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Prod,
    Test,
    Build,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Prod => write!(f, "prod"),
            Context::Test => write!(f, "test"),
            Context::Build => write!(f, "build"),
        }
    }
}

/// Production surfaces the membrane guards. Must match the `surfaces` keys in
/// designing/schemas/membrane-surfaces.json — see `enum_matches_schema` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Spine,
    SessionsRegistry,
    IndexDb,
    Lance,
    MessagesDb,
    Fuseki,
    RoleState,
    ChorusHome,
}

impl Surface {
    /// Schema key in membrane-surfaces.json.
    pub fn key(&self) -> &'static str {
        match self {
            Surface::Spine => "spine",
            Surface::SessionsRegistry => "sessions-registry",
            Surface::IndexDb => "index-db",
            Surface::Lance => "lance",
            Surface::MessagesDb => "messages-db",
            Surface::Fuseki => "fuseki",
            Surface::RoleState => "role-state",
            Surface::ChorusHome => "chorus-home",
        }
    }

    /// The env var a non-prod context sets to bring its own world.
    pub fn override_var(&self) -> Option<&'static str> {
        match self {
            Surface::Spine => Some("CHORUS_LOG_FILE"),
            Surface::SessionsRegistry => Some("CHORUS_SESSIONS_DIR"),
            Surface::IndexDb => Some("CHORUS_DB_PATH"),
            Surface::Lance => Some("CHORUS_LANCE_DIR"),
            Surface::MessagesDb => Some("CHORUS_MESSAGES_DB"),
            Surface::Fuseki => Some("FUSEKI_URL"),
            Surface::RoleState => None,
            Surface::ChorusHome => Some("CHORUS_HOME"),
        }
    }

    pub const ALL: &'static [Surface] = &[
        Surface::Spine,
        Surface::SessionsRegistry,
        Surface::IndexDb,
        Surface::Lance,
        Surface::MessagesDb,
        Surface::Fuseki,
        Surface::RoleState,
        Surface::ChorusHome,
    ];
}

/// Typed refusal — what the membrane says when a non-prod context resolves a
/// production surface without its override. The Display form is the contract:
/// it names the surface, the marker that classified the context, and the fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub surface: &'static str,
    pub context: Context,
    pub marker: String,
    pub override_var: Option<&'static str>,
    pub prod_path: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEMBRANE REFUSED (#3615): {} context ({}) attempted to resolve production surface '{}' ({}) — a test brings its own world (#3528). {}",
            self.context,
            self.marker,
            self.surface,
            self.prod_path,
            match self.override_var {
                Some(v) => format!("Set {} to a path inside the test's tempdir.", v),
                None => "This surface has no override — do not touch it from a non-prod context.".to_string(),
            }
        )
    }
}

/// Pure context classification. `env` is a lookup fn so tests never mutate
/// process env (parallel test safety). Explicit CHORUS_CONTEXT wins; ambient
/// markers otherwise; prod is the absence of every marker.
/// Returns (context, marker-that-decided).
pub fn context_from(env: &dyn Fn(&str) -> Option<String>) -> (Context, String) {
    if let Some(v) = env("CHORUS_CONTEXT") {
        return match v.as_str() {
            "test" => (Context::Test, "CHORUS_CONTEXT=test".into()),
            "build" => (Context::Build, "CHORUS_CONTEXT=build".into()),
            // Any other value (incl. "prod") is an explicit prod declaration:
            // the escape hatch for prod-adjacent runners (nightly, werk verbs)
            // that inherit ambient CI-ish env.
            _ => (Context::Prod, format!("CHORUS_CONTEXT={}", v)),
        };
    }
    if env("BATS_TEST_TMPDIR").is_some() {
        return (Context::Test, "BATS_TEST_TMPDIR set".into());
    }
    if env("NODE_ENV").as_deref() == Some("test") {
        return (Context::Test, "NODE_ENV=test".into());
    }
    if env("CARGO").is_some() {
        return (Context::Build, "CARGO set (cargo test/run child)".into());
    }
    if env("CI").is_some() || env("GITHUB_ACTIONS").is_some() {
        return (Context::Build, "CI set".into());
    }
    (Context::Prod, "no non-prod marker".into())
}

/// Pure resolver. Override set → the test's own world, always allowed.
/// No override + prod → production path. No override + test/build → Violation.
pub fn resolve_from(
    surface: Surface,
    prod_path: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, Violation> {
    if let Some(var) = surface.override_var() {
        if let Some(p) = env(var) {
            if !p.is_empty() {
                return Ok(p);
            }
        }
    }
    let (ctx, marker) = context_from(env);
    match ctx {
        Context::Prod => Ok(prod_path.to_string()),
        _ => Err(Violation {
            surface: surface.key(),
            context: ctx,
            marker,
            override_var: surface.override_var(),
            prod_path: prod_path.to_string(),
        }),
    }
}

fn real_env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|s| !s.is_empty())
}

/// Impure shell: resolve a surface path against real env. On violation,
/// best-effort emit `membrane.violation` to the LIVE spine (the one sanctioned
/// crossing — the guard's own controlled emission, so drift is visible, not
/// silent — AC5), then panic with the typed message. Panicking in a test/build
/// context fails the offending test, which is exactly the signal.
pub fn resolve(surface: Surface, prod_path: &str) -> String {
    match resolve_from(surface, prod_path, &real_env) {
        Ok(p) => p,
        Err(v) => {
            emit_violation_event(&v);
            panic!("{}", v);
        }
    }
}

/// Eastern offset, self-contained (third copy after clock_sync + chorus_log
/// #1850 — necessary: shared/ compiles into lib, main, AND shim, and no one
/// module with an offset fn is present in all three trees).
fn eastern_offset() -> chrono::FixedOffset {
    let output = std::process::Command::new("date")
        .args(["+%z"])
        .env("TZ", "America/New_York")
        .output();
    if let Ok(out) = output {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.len() >= 5 {
            let sign = if s.starts_with('-') { -1 } else { 1 };
            let h: i32 = s[1..3].parse().unwrap_or(5);
            let m: i32 = s[3..5].parse().unwrap_or(0);
            if let Some(offset) = chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60)) {
                return offset;
            }
        }
    }
    chrono::FixedOffset::west_opt(5 * 3600).unwrap()
}

/// Direct synchronous append to the real spine path — deliberately NOT via
/// chorus_log()/append_log, which resolve through the membrane and would
/// recurse. Best-effort: a refusal must never be masked by a failed emit.
fn emit_violation_event(v: &Violation) {
    use std::io::Write;
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let path = format!("{}/.chorus/chorus.log", home);
    let ts = chrono::Utc::now()
        .with_timezone(&eastern_offset())
        .format("%Y-%m-%dT%H:%M:%S%.3f%z")
        .to_string();
    // Resolve once, keeping the failure REASON alongside the value.
    let role_for_emit: (String, &'static str) = match super::role::resolve_caller_role() {
        Ok(r) => (r, ""),
        Err(e) => ("unattributed".to_string(), e.reason()),
    };
    let line = serde_json::json!({
        "timestamp": ts,
        "level": "warn",
        "appName": "chorus-events",
        "event": "membrane.violation",
        // #3959 — when the role cannot be resolved, say SO and say WHY, rather
        // than writing a name nobody chose. "unattributed" read as a role for
        // months; role_unresolved names the reason so the gap is diagnosable.
        "role": role_for_emit.0,
        "role_unresolved": role_for_emit.1,
        "surface": v.surface,
        "context": v.context.to_string(),
        "marker": v.marker,
        "override_var": v.override_var.unwrap_or(""),
    });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k: &str| map.get(k).cloned()
    }

    #[test]
    fn prod_is_absence_of_markers() {
        let env = env_of(&[("HOME", "/Users/jeffbridwell")]);
        assert_eq!(context_from(&env).0, Context::Prod);
    }

    #[test]
    fn ambient_markers_classify_test_and_build() {
        assert_eq!(context_from(&env_of(&[("BATS_TEST_TMPDIR", "/tmp/x")])).0, Context::Test);
        assert_eq!(context_from(&env_of(&[("NODE_ENV", "test")])).0, Context::Test);
        assert_eq!(context_from(&env_of(&[("CARGO", "/usr/bin/cargo")])).0, Context::Build);
        assert_eq!(context_from(&env_of(&[("CI", "true")])).0, Context::Build);
    }

    #[test]
    fn explicit_chorus_context_wins_over_ambient() {
        // The nightly runner escape hatch: CI-ish env, explicitly prod.
        let env = env_of(&[("CI", "true"), ("CHORUS_CONTEXT", "prod")]);
        assert_eq!(context_from(&env).0, Context::Prod);
        // And the reverse: explicit test with no ambient marker.
        let env = env_of(&[("CHORUS_CONTEXT", "test")]);
        assert_eq!(context_from(&env).0, Context::Test);
    }

    #[test]
    fn prod_context_resolves_prod_path() {
        let env = env_of(&[]);
        let got = resolve_from(Surface::Spine, "/Users/j/.chorus/chorus.log", &env).unwrap();
        assert_eq!(got, "/Users/j/.chorus/chorus.log");
    }

    #[test]
    fn override_is_the_tests_own_world() {
        let env = env_of(&[("BATS_TEST_TMPDIR", "/tmp/x"), ("CHORUS_LOG_FILE", "/tmp/x/spine.log")]);
        let got = resolve_from(Surface::Spine, "/Users/j/.chorus/chorus.log", &env).unwrap();
        assert_eq!(got, "/tmp/x/spine.log");
    }

    /// NEGATIVE PROOF (#3734): the state the membrane exists to catch — a test
    /// context resolving a production surface with NO override — must REFUSE.
    #[test]
    fn test_context_without_override_refuses_with_typed_error() {
        let env = env_of(&[("BATS_TEST_TMPDIR", "/tmp/x")]);
        let err = resolve_from(Surface::Spine, "/Users/j/.chorus/chorus.log", &env).unwrap_err();
        assert_eq!(err.surface, "spine");
        assert_eq!(err.context, Context::Test);
        let msg = err.to_string();
        assert!(msg.contains("MEMBRANE REFUSED"), "typed prefix missing: {msg}");
        assert!(msg.contains("'spine'"), "must name the surface: {msg}");
        assert!(msg.contains("CHORUS_LOG_FILE"), "must name the fix: {msg}");
        assert!(msg.contains("BATS_TEST_TMPDIR"), "must name the marker: {msg}");
    }

    /// The #3608 shape: sessions registry from a test without CHORUS_SESSIONS_DIR.
    #[test]
    fn sessions_registry_poisoning_shape_refuses() {
        let env = env_of(&[("NODE_ENV", "test")]);
        let err = resolve_from(Surface::SessionsRegistry, "/Users/j/.chorus/sessions", &env).unwrap_err();
        assert!(err.to_string().contains("CHORUS_SESSIONS_DIR"));
    }

    #[test]
    fn build_context_refuses_messages_db() {
        let env = env_of(&[("CARGO", "/usr/bin/cargo")]);
        let err = resolve_from(Surface::MessagesDb, "/x/platform/pulse/messages.db", &env).unwrap_err();
        assert_eq!(err.context, Context::Build);
        assert!(err.to_string().contains("CHORUS_MESSAGES_DB"));
    }

    /// Drift guard (#3734): the enum and membrane-surfaces.json must match 1:1.
    /// If the schema file is deleted, renamed, or a surface diverges, this
    /// fails loudly — it must never pass vacuously.
    #[test]
    fn enum_matches_schema() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = format!("{}/../../../designing/schemas/membrane-surfaces.json", manifest);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("membrane-surfaces.json unreadable at {path}: {e} — the drift guard's target moved; fix the path or the schema, do not delete this test"));
        let schema: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let surfaces = schema.get("surfaces").and_then(|s| s.as_object())
            .expect("membrane-surfaces.json must have a 'surfaces' object");

        let mut schema_keys: Vec<&str> = surfaces.keys().map(|s| s.as_str()).collect();
        let mut enum_keys: Vec<&str> = Surface::ALL.iter().map(|s| s.key()).collect();
        schema_keys.sort();
        enum_keys.sort();
        assert_eq!(schema_keys, enum_keys, "Surface enum and membrane-surfaces.json diverged");

        // Override vars must agree too — the refusal message's "fix" line is
        // only correct if the schema and enum name the same env var.
        for s in Surface::ALL {
            let json_override = surfaces[s.key()].get("override").and_then(|v| v.as_str());
            assert_eq!(json_override, s.override_var(), "override drift on '{}'", s.key());
        }
    }
}
