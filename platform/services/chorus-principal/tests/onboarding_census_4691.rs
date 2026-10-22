// @test-type: unit
// @domain: identity
//! #4691 — every agent principal on its own Mac account onboards the same way
//! (Jeff 2026-10-22 08:35: "i dont need 2 or 3 snowflakes"). The steps are one
//! list, platform/config/agent-onboarding-steps.tsv, read by census (and by
//! create, #4694). On 2026-10-22 census said "14 users, all complete" while
//! Borg Queen lacked seven steps; these cases hold census to naming each one.
use chorus_principal::onboarding::{gaps, steps, Facts};

fn complete() -> Facts {
    Facts {
        host_account: Some("chorus-x".into()),
        holds_role: Some("role-x".into()),
        agent_runtime: Some("runtime-agy".into()),
        role_is_agent: true,
        word_caps: 2,
        in_group: true,
        runs_on: Some("agy".into()),
        role_home: true,
        webid_card: "200".into(),
    }
}

#[test]
fn the_step_list_is_the_committed_file_with_one_jeff_step() {
    let s = steps();
    assert_eq!(s.len(), 10, "{s:?}");
    assert_eq!(s.iter().filter(|x| x.owner == "jeff").map(|x| x.id.as_str()).collect::<Vec<_>>(), ["runtime-sign-in"]);
    assert!(s.iter().all(|x| x.owner == "create" || x.owner == "jeff"), "{s:?}");
}

#[test]
fn a_principal_with_every_step_has_only_jeffs_step_left() {
    let g = gaps(&complete());
    assert_eq!(g.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["runtime-sign-in"], "census cannot see a sign-in, so it is always named as Jeff's");
}

/// NEGATIVE: remove any one create step and census names exactly that step.
#[test]
fn negative_proof_each_missing_step_is_named() {
    let cases: Vec<(&str, Box<dyn Fn(&mut Facts)>)> = vec![
        ("principal-host", Box::new(|f| f.host_account = None)),
        ("principal-role", Box::new(|f| f.holds_role = None)),
        ("principal-runtime", Box::new(|f| f.agent_runtime = None)),
        ("role-agent", Box::new(|f| f.role_is_agent = false)),
        ("role-word-caps", Box::new(|f| f.word_caps = 1)),
        ("account-group", Box::new(|f| f.in_group = false)),
        ("runtime-table", Box::new(|f| f.runs_on = None)),
        ("role-home", Box::new(|f| f.role_home = false)),
        ("webid-card", Box::new(|f| f.webid_card = "404".into())),
    ];
    assert_eq!(cases.len(), steps().iter().filter(|s| s.owner == "create").count(), "every create step has a negative");
    for (id, break_it) in cases {
        let mut f = complete();
        break_it(&mut f);
        let g: Vec<String> = gaps(&f).into_iter().filter(|x| x.owner == "create").map(|x| x.id).collect();
        assert_eq!(g, [id], "breaking {id}");
    }
}

/// Borg Queen as measured live 2026-10-22 08:40, before #4691 lands: census
/// names the seven, not "complete".
#[test]
fn borg_queen_as_measured_this_morning_is_seven_steps_short() {
    let f = Facts { host_account: None, holds_role: None, agent_runtime: None, role_is_agent: false, word_caps: 0,
        in_group: true, runs_on: None, role_home: false, webid_card: "200".into() };
    let g: Vec<String> = gaps(&f).into_iter().filter(|x| x.owner == "create").map(|x| x.id).collect();
    assert_eq!(g, ["principal-host", "principal-role", "principal-runtime", "role-agent", "role-word-caps", "runtime-table", "role-home"]);
}
