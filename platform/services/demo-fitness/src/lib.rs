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

/// The services a demo must run its own copy of.
pub const TARGET: [&str; 6] = [
    "chorus-api",
    "chorus-mcp",
    "athena-make",
    "athena-model",
    "chorus-hooks",
    "clearing",
];

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
pub fn measure(launchctl_list: &str, role: &str) -> Fitness {
    let running = running_labels(launchctl_list);
    let mut own = Vec::new();
    let mut missing = Vec::new();
    for s in TARGET {
        if running.iter().any(|l| l == &variant_label(s, role)) {
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
        "demo-fitness {}: {} of {} target services running{}\n  own     : {}\n  MISSING : {}\n  shared  : {} prod service(s), incl. the store\n",
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
