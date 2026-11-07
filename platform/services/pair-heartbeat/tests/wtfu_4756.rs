// @test-type: unit
// @domain: roles
// #4756 — pair-heartbeat/src/wtfu (and its wiring in src/lib): an idle actor
// with work waiting on it is woken once; a busy one, or one with nothing
// waiting, is not. Fixtures are 10-26's real shapes: dagu's waiting pair run
// on #4753 (borg-queen answers), 4749's pair still "waiting" after it landed,
// a go step (Jeff's), and the 16:44 renudge borg-queen never saw.
use pair_heartbeat::wtfu::{answerer, human_tasks, idle_secs, should_wake, turn_ends, unread_nudges, wake_message, Work, IDLE_SECS, REWAKE_SECS, WINDOW_SECS};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

#[test]
fn the_answerer_is_named_in_the_prompt_and_jeff_is_never_one() {
    assert_eq!(answerer("every AC green for #4749? (kade answers)").as_deref(), Some("kade"));
    assert_eq!(answerer("Borg Queen proves it in prod for #4753? (borg-queen answers)").as_deref(), Some("borg-queen"));
    assert_eq!(answerer("post-land decision for #4751? ( answers)"), None);
    assert_eq!(answerer("Jeff's go for #4751?"), None);
    assert_eq!(answerer("ok? (jeff answers)"), None);
}

fn run(name: &str, id: &str, card: &str, status: &str) -> Value {
    json!({"name": name, "dagRunId": id, "statusLabel": status, "params": format!("CARD={card} ROLE=kade NAVIGATOR=borg-queen")})
}

fn detail(step: &str, prompt: &str) -> Value {
    json!({"dagRunDetails": {"nodes": [
        {"statusLabel": "succeeded", "step": {"name": "skill-pair-start"}},
        {"statusLabel": "waiting", "step": {"name": step, "humanTask": {"prompt": prompt}}}
    ]}})
}

#[test]
fn a_waiting_task_on_a_wip_card_is_its_answerers_work() {
    let runs = json!({"dagRuns": [
        run("pair", "034mnXten", "4753", "waiting"),
        run("cicd", "034miMuox", "4753", "waiting"),
        run("pair", "034mfAAEu", "4749", "waiting"),
        run("cicd", "034mzzzzz", "4753", "running"),
    ]});
    let w = human_tasks(&runs, &["4753".into()], |name, _| {
        Some(match name {
            "pair" => detail("skill-pair-green", "every AC green for #4753? (borg-queen answers)"),
            _ => detail("skill-werk-go", "Jeff's go for #4753?"),
        })
    });
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].actor, "borg-queen");
    assert!(w[0].what.contains("#4753") && w[0].what.contains("skill-pair-green"), "{}", w[0].what);
}

/// NEGATIVE PROOF: 4749 landed, its pair run still reads "waiting"; no card
/// in WIP means no work, and Jeff's go step is never anyone's work.
#[test]
fn negative_proof_a_stale_run_and_the_go_are_nobodys_work() {
    let runs = json!({"dagRuns": [run("pair", "034mfAAEu", "4749", "waiting"), run("cicd", "034miMuox", "4753", "waiting")]});
    let w = human_tasks(&runs, &["4753".into()], |name, _| {
        Some(if name == "pair" { detail("skill-pair-green", "(kade answers)") } else { detail("skill-werk-go", "Jeff's go for #4753?") })
    });
    assert!(w.is_empty(), "{w:?}");
}

fn loki(lines: &[(i64, Value)]) -> Value {
    let values: Vec<Value> = lines.iter().map(|(at, l)| json!([(at * 1_000_000_000).to_string(), l.to_string()])).collect();
    json!({"data": {"result": [{"stream": {"job": "platform-chorus"}, "values": values}]}})
}

fn emitted(to: &str, trace: &str, content: &str) -> Value {
    json!({"event": "nudge.emitted", "role": "system", "payload": format!("from=system,to={to},chars=1,trace={trace},origin=mcp,content={content}")})
}

#[test]
fn an_unsurfaced_nudge_older_than_a_minute_is_work_and_a_surfaced_one_is_not() {
    let t = now();
    let l = loki(&[
        (t - 300, emitted("borg-queen", "tr-unread", "pair #4753: you have been quiet 120s")),
        (t - 300, emitted("kade", "tr-read", "hello")),
        (t - 290, json!({"event": "nudge.surfaced", "role": "pulse", "to": "kade", "trace_id": "tr-read"})),
        (t - 10, emitted("wren", "tr-fresh", "just sent")),
        (t - 300, emitted("jeff", "tr-jeff", "go?")),
        (t - 300, emitted("abby-normal", "tr-wake", "wtfu abby-normal: 1 unread nudge(s) waits on you")),
    ]);
    let w = unread_nudges(&l, t);
    let actors: Vec<&str> = w.iter().map(|w| w.actor.as_str()).collect();
    assert_eq!(actors, vec!["borg-queen"], "fresh, surfaced, Jeff's and wake nudges are not work: {w:?}");
}

#[test]
fn idle_is_measured_from_the_last_turn_end_and_a_running_turn_is_never_idle() {
    let t = now();
    let l = loki(&[(t - 500, json!({"event": "agent.turn.ended", "role": "kade"})), (t - 90, json!({"event": "agent.turn.ended", "role": "kade"}))]);
    let ends = turn_ends(&l);
    assert_eq!(ends.get("kade"), Some(&(t - 90)));
    assert_eq!(idle_secs(Some(t - 120), Some(t - 90), t), Some(90));
    // NEGATIVE PROOF: a turn that started after the last end is running
    assert_eq!(idle_secs(Some(t - 30), Some(t - 90), t), None);
    assert_eq!(idle_secs(Some(t - 600), None, t), None, "a turn started inside the window with no end is running");
    assert_eq!(idle_secs(None, None, t), Some(WINDOW_SECS));
}

#[test]
fn a_wake_fires_once_per_work_and_again_only_after_the_cooldown() {
    let t = now();
    assert!(should_wake(Some(IDLE_SECS), "pair/x", "", 0, t));
    assert!(!should_wake(Some(IDLE_SECS - 1), "pair/x", "", 0, t), "idle under a minute");
    assert!(!should_wake(None, "pair/x", "", 0, t), "a running turn");
    assert!(!should_wake(Some(600), "", "", 0, t), "nothing waiting");
    assert!(!should_wake(Some(600), "pair/x", "pair/x", t - 60, t), "already woken for this");
    assert!(should_wake(Some(600), "pair/x nudge/y", "pair/x", t - 60, t), "new work");
    assert!(should_wake(Some(600), "pair/x", "pair/x", t - REWAKE_SECS, t), "cooldown passed");
}

#[test]
fn the_wake_names_the_actor_and_the_work_first() {
    let a = Work { actor: "borg-queen".into(), key: "pair/1/skill-pair-green".into(), what: "#4753 pair skill-pair-green (run 1)".into() };
    let b = Work { actor: "borg-queen".into(), key: "nudge/t".into(), what: "an unread nudge from system".into() };
    let m = wake_message("borg-queen", 240, &[&a, &b]);
    assert!(m.starts_with("wtfu borg-queen: #4753 pair skill-pair-green"), "{m}");
    assert!(m.contains("1 unread nudge(s)") && m.contains("idle 4m"), "{m}");
}

// --- the real tick, against a fake CHORUS_HOME and file:// dagu, Loki and board ---

struct Fixture {
    home: std::path::PathBuf,
}

fn fixture(turn_end_ago: i64, turn_started_ago: i64) -> Fixture {
    let t = now();
    let home = std::env::temp_dir().join(format!("wtfu-4756-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let api = home.join("dagu/api/v1/dag-runs");
    fs::create_dir_all(api.join("pair")).unwrap();
    fs::write(home.join("dagu/api/v1/dag-runs.json"), json!({"dagRuns": [run("pair", "034mnXten", "4753", "waiting")]}).to_string()).unwrap();
    fs::write(api.join("pair/034mnXten.json"), detail("skill-pair-green", "every AC green for #4753? (borg-queen answers)").to_string()).unwrap();
    let lk = home.join("loki/loki/api/v1");
    fs::create_dir_all(&lk).unwrap();
    fs::write(lk.join("query_range.json"), loki(&[(t - turn_end_ago, json!({"event": "agent.turn.ended", "role": "borg-queen"}))]).to_string()).unwrap();
    let run_dir = home.join("run");
    fs::create_dir_all(&run_dir).unwrap();
    fs::write(run_dir.join("turn-borg-queen.json"), json!({"at": t - turn_started_ago}).to_string()).unwrap();
    fs::write(home.join("board.json"), r#"{"data":{"cards":[{"id":4753}]}}"#).unwrap();
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("ops-nudge"), format!("echo \"$1|$2\" >> {}\n", home.join("nudges.log").display())).unwrap();
    fs::write(scripts.join("chorus-log"), "true\n").unwrap();
    Fixture { home }
}

impl Fixture {
    fn tick(&self) -> String {
        let h = &self.home;
        let out = Command::new(env!("CARGO_BIN_EXE_pair-heartbeat"))
            .env("CHORUS_HOME", h)
            .env("PAIR_BOARD_URL", format!("file://{}", h.join("board.json").display()))
            .env("WTFU_DAGU", format!("file://{}", h.join("dagu").display()))
            .env("WTFU_LOKI", format!("file://{}", h.join("loki").display()))
            .env("WTFU_RUN_DIR", h.join("run"))
            .env_remove("WTFU_DRY")
            .env_remove("CHORUS_ROLE")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        fs::read_to_string(h.join("nudges.log")).unwrap_or_default()
    }
}

#[test]
fn the_tick_wakes_an_idle_navigator_once() {
    let f = fixture(300, 400);
    let log = f.tick();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.starts_with("borg-queen|wtfu borg-queen: #4753 pair skill-pair-green"), "{log}");
    assert_eq!(f.tick().lines().count(), 1, "the same work does not wake her twice");
}

/// NEGATIVE PROOF: the same work, but her turn is running (started after it
/// last ended) or ended under a minute ago: 0 wake turns.
#[test]
fn negative_proof_a_busy_or_just_idle_actor_is_not_woken() {
    assert_eq!(fixture(300, 20).tick(), "", "a running turn is never interrupted");
    assert_eq!(fixture(30, 40).tick(), "", "idle under a minute");
}
