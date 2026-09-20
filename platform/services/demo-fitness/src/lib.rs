//! #4225 — HOW MUCH OF CHORUS DOES A DEMO ACTUALLY RUN?
//!
//! Jeff, 2026-09-20: "if prod is 100% chorus running and demo is 25% chorus
//! running u can see how that contributes to quality issues" — and the plain
//! name for the alternative, "deploy and hope".
//!
//! THE TARGET (his, 08:20, "that sounds like enough to start"):
//!   own copy per demo : api · mcp · athena-make · athena-model · hooks · clearing
//!   shared with prod  : the rows in Fuseki (his 09-16 rule — demo uses prod data)
//!   pinned per demo   : the shape the variant generates from (not built yet)
//!   never in a demo   : timers, harvesters, exporters, the nightly
//!
//! Today werk-deploy swaps the first four. athena-model and chorus-hooks have
//! no variant at all, which is why a binary deployed for one is live for both —
//! measured twice on 2026-09-19/20 when a model move took every write down.
//!
//! The point is a number that moves: 4 of 6 today, and the missing two named,
//! so werk-deploy has something to improve against instead of an argument.

/// How a demo can own a piece of Chorus. The two are not the same question,
/// and asking the wrong one is how this check first reported athena-model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// A long-running service. Owned when the role's variant is running.
    Daemon,
    /// A verb: runs, does one thing, exits. athena-model is one —
    /// `add | add-batch | delete | set | link | unlink`, called by athena-make
    /// for every write. It is never "running", so a process check can only ever
    /// report it missing. Owned when the werk has built its own copy of the
    /// binary, because that is the copy the variant's athena-make will call.
    Verb,
}

/// The pieces of Chorus a demo must own its own copy of.
pub const TARGET: [(&str, Piece); 6] = [
    ("chorus-api", Piece::Daemon),
    ("chorus-mcp", Piece::Daemon),
    ("athena-make", Piece::Daemon),
    ("athena-model", Piece::Verb),
    ("chorus-hooks", Piece::Daemon),
    ("clearing", Piece::Daemon),
];

/// The piece kind for a target name, or None if it is not a target.
pub fn piece_of(service: &str) -> Option<Piece> {
    TARGET.iter().find(|(n, _)| *n == service).map(|(_, k)| *k)
}

/// Where deploy-werk installs a werk's built binaries — the same slot
/// werk-deploy's demo_env::werk_bin_dir writes to. A verb is owned when its
/// binary is here, because CHORUS_MODEL_BIN on the variant points at this copy.
pub fn verb_bin_path(service: &str, role: &str, werk_base: &str) -> String {
    format!("{}/{}-bin/{}", werk_base, role, service)
}

/// The launchd label a variant copy carries: com.chorus.<short>.werk.<role>.
/// The "chorus-" prefix is stripped because that is what werk-deploy writes
/// (demo_env.rs strip_chorus_prefix).
pub fn variant_label(service: &str, role: &str) -> String {
    let short = service.strip_prefix("chorus-").unwrap_or(service);
    format!("com.chorus.{}.werk.{}", short, role)
}

/// Running labels, from `launchctl list` output: a row whose first column is a
/// pid. A row with "-" is scheduled, not running, and must not be counted — the
/// difference between "a demo has its own mcp" and "one is configured".
pub fn running_labels(launchctl_list: &str) -> Vec<String> {
    launchctl_list
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let pid = f.next()?;
            let _status = f.next()?;
            let label = f.next()?;
            if pid == "-" || pid == "PID" {
                None
            } else {
                Some(label.to_string())
            }
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Fitness {
    pub role: String,
    pub own: Vec<String>,
    pub missing: Vec<String>,
    pub shared: Vec<String>,
}

impl Fitness {
    pub fn target_total(&self) -> usize {
        self.own.len() + self.missing.len()
    }
}

/// Pure: given what is running and a role, say which target services this demo
/// owns, which it lacks, and which prod services it is therefore using instead.
///
/// "shared" counts only prod services that are actually up and are NOT variant
/// copies — the things a demo borrows. A demo borrowing the store is by design;
/// a demo borrowing athena-model is the gap this number exists to show.
/// `own_verbs` is the verb binaries the werk has built its own copy of — the
/// caller looks at the filesystem, this stays pure.
pub fn measure(launchctl_list: &str, role: &str, own_verbs: &[String]) -> Fitness {
    let running = running_labels(launchctl_list);
    let mut own = Vec::new();
    let mut missing = Vec::new();
    for (s, kind) in TARGET {
        let owned = match kind {
            Piece::Daemon => running.iter().any(|l| l == &variant_label(s, role)),
            Piece::Verb => own_verbs.iter().any(|v| v == s),
        };
        if owned {
            own.push(s.to_string());
        } else {
            missing.push(s.to_string());
        }
    }
    let mut shared: Vec<String> = running
        .into_iter()
        .filter(|l| !l.contains(".werk."))
        .filter(|l| {
            l.starts_with("com.chorus.")
                || l.starts_with("com.gathering.")
                || l.starts_with("com.security.")
        })
        .collect();
    shared.sort();
    Fitness { role: role.to_string(), own, missing, shared }
}

/// The line Jeff reads. `prev` is the previous run's own-count, or None.
pub fn report(f: &Fitness, prev: Option<usize>) -> String {
    let delta = match prev {
        Some(p) => format!(" (prev {})", p),
        None => String::new(),
    };
    format!(
        "demo-fitness {}: {} of {} target pieces owned{}\n  own     : {}\n  MISSING : {}\n  shared  : {} prod service(s), incl. the store\n",
        f.role,
        f.own.len(),
        f.target_total(),
        delta,
        if f.own.is_empty() { "none".into() } else { f.own.join(" ") },
        if f.missing.is_empty() { "none".into() } else { f.missing.join(" ") },
        f.shared.len()
    )
}

/// One append-only JSON line. Appended, never rewritten — same rule as the
/// spine: a trend that is overwritten each run is not a trend.
pub fn series_line(f: &Fitness, ts: &str) -> String {
    let shared: Vec<String> = f.shared.iter().map(|s| format!("\"{}\"", s)).collect();
    format!(
        "{{\"ts\":\"{}\",\"role\":\"{}\",\"own\":{},\"target\":{},\"missing\":[{}],\"shared\":{},\"shared_services\":[{}]}}",
        ts,
        f.role,
        f.own.len(),
        f.target_total(),
        f.missing.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
        f.shared.len(),
        shared.join(",")
    )
}

/// The previous run's own-count, read from the last line of the series.
pub fn previous_own(series: &str) -> Option<usize> {
    let last = series.lines().filter(|l| !l.trim().is_empty()).next_back()?;
    let i = last.find("\"own\":")? + 6;
    let rest = &last[i..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}
