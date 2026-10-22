//! #4691 — the agent onboarding steps, one list (platform/config/agent-onboarding-steps.tsv),
//! and the census judgement over them: which steps a principal is missing.
//! Pure: the facts are measured by the caller (main.rs census), so every step
//! can be shown missing in a test.

const STEPS: &str = include_str!("../../../config/agent-onboarding-steps.tsv");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub owner: String,
    pub what: String,
}

/// The committed step list, in order. A line that is not `id<TAB>owner<TAB>what` is skipped.
pub fn steps() -> Vec<Step> {
    STEPS
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut c = l.splitn(3, '\t');
            Some(Step { id: c.next()?.to_string(), owner: c.next()?.to_string(), what: c.next()?.to_string() })
        })
        .collect()
}

/// What census measured for one agent principal.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub host_account: Option<String>,
    pub holds_role: Option<String>,
    pub agent_runtime: Option<String>,
    pub role_is_agent: bool,
    /// word-cap edges whose Property row exists
    pub word_caps: usize,
    pub in_group: bool,
    pub runs_on: Option<String>,
    pub role_home: bool,
    pub webid_card: String,
}

fn done(id: &str, f: &Facts) -> Option<bool> {
    Some(match id {
        "principal-host" => f.host_account.is_some(),
        "principal-role" => f.holds_role.is_some(),
        "principal-runtime" => f.agent_runtime.is_some(),
        "role-agent" => f.role_is_agent,
        "role-word-caps" => f.word_caps >= 2,
        "account-group" => f.in_group,
        "runtime-table" => f.runs_on.is_some(),
        "role-home" => f.role_home,
        "webid-card" => f.webid_card == "200",
        _ => return None,
    })
}

/// The steps not done, in list order. A Jeff step, or a step id census has no
/// measure for, is always named: census never reports what it cannot see as done.
pub fn gaps(f: &Facts) -> Vec<Step> {
    steps().into_iter().filter(|s| s.owner == "jeff" || done(&s.id, f) != Some(true)).collect()
}
