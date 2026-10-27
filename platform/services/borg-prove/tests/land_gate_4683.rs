// @test-type: unit
// @domain: code
// #4683 — borg-prove prod and record run inside cicd-land, before accept
// (src/main.rs, src/lib.rs). What Jeff sees: a card whose ACs are all met in prod is
// accepted with prover borg-queen on the card; any AC not met or not measured
// stops the land before accept; a card with no AC check lands as today and
// says "unproven" on the card, never green. #4760: the AC lines are read from
// the card itself (`cards view`), not from CardAC rows.
use std::path::PathBuf;
use std::process::{Command, Output};

const VIEW: &str = "#4683 Wire borg-prove into the land\n  Status:   WIP\n  Owner:    Kade\n  Desc:\n    ## AC\n";
const FIELDS: &str = "\n  Domains:  domain:chorus\n  Comments (1):\n    - [ ] a comment's checkbox is not an AC  check: http /nope.txt ok\n";

struct Fx { dir: PathBuf }

impl Fx {
    fn new(name: &str, rows: &str) -> Fx {
        let dir = std::env::temp_dir().join(format!("borg-4683-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("prod")).unwrap();
        std::fs::write(dir.join("prod/ok.txt"), "status ok\n").unwrap();
        std::fs::write(dir.join("view.txt"), format!("{VIEW}{rows}{FIELDS}")).unwrap();
        let d = dir.display();
        for (bin, body) in [
            ("chorus-log", format!("echo \"$*\" >> {d}/spine.log")),
            // view answers from view.txt; every other call is logged with who made it
            ("cards", format!("case \"$1\" in view) cat {d}/view.txt ;; *) echo \"DEPLOY_ROLE=$DEPLOY_ROLE $*\" >> {d}/cards.log ;; esac")),
        ] {
            let p = dir.join(bin);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            Command::new("chmod").arg("+x").arg(&p).status().unwrap();
        }
        Fx { dir }
    }
    fn run(&self, mode: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_borg-prove"))
            .args([mode, "4683", "kade"])
            .env("BORG_PROD_BASE", format!("file://{}", self.dir.join("prod").display()))
            .env("BORG_CHORUS_LOG", self.dir.join("chorus-log"))
            .env("BORG_CARDS", self.dir.join("cards"))
            .env("BORG_RESULTS_DIR", self.dir.join("results"))
            .env("DAG_RUN_ID", "run1")
            // these runs are her own session: record does not hop (#4760)
            .env("BORG_QUEEN_ACCOUNT", me())
            .output()
            .unwrap()
    }
    fn log(&self, name: &str) -> String { std::fs::read_to_string(self.dir.join(name)).unwrap_or_default() }
}

fn me() -> String { String::from_utf8_lossy(&Command::new("id").arg("-un").output().unwrap().stdout).trim().to_string() }
fn out(o: &Output) -> String { format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)) }
fn row(ac: &str, target: &str, expect: &str) -> String { format!("    - [ ] {ac}  check: http {target} {expect}\n") }

#[test]
fn every_ac_met_in_prod_passes_and_record_names_borg_queen() {
    let fx = Fx::new("green", &row("ac-4683-1", "/ok.txt", "ok"));
    let p = fx.run("prod");
    assert!(p.status.success(), "{}", out(&p));
    assert!(out(&p).contains("Proven"), "{}", out(&p));
    let r = fx.run("record");
    assert!(r.status.success(), "{}", out(&r));
    assert!(fx.log("spine.log").contains("borg.verdict"), "{}", fx.log("spine.log"));
    assert!(fx.log("spine.log").contains("prover=borg-queen") && fx.log("spine.log").contains("verdict=Proven"), "{}", fx.log("spine.log"));
    assert!(fx.log("cards.log").contains(" comment 4683 "), "{}", fx.log("cards.log"));
    assert_eq!(adds(&fx), 0, "an all-green card files no children: {}", fx.log("cards.log"));
}

#[test]
fn negative_proof_an_ac_not_met_in_prod_fails_the_step_and_record_refuses() {
    let fx = Fx::new("red", &format!("{}{}", row("ac-4683-1", "/ok.txt", "ok"), row("ac-4683-2", "/ok.txt", "nope")));
    let p = fx.run("prod");
    assert!(!p.status.success(), "a red AC must stop the land: {}", out(&p));
    assert!(out(&p).contains("Not met"), "{}", out(&p));
    let r = fx.run("record");
    assert!(!r.status.success(), "record must not pass a red verdict on to accept: {}", out(&r));
}

#[test]
fn negative_proof_an_ac_that_cannot_be_measured_reads_red() {
    let fx = Fx::new("unmeasured", &row("ac-4683-1", "/missing.txt", "ok"));
    let p = fx.run("prod");
    assert!(!p.status.success(), "unmeasured is never green: {}", out(&p));
    assert!(out(&p).contains("unmeasured"), "{}", out(&p));
}

#[test]
fn a_card_with_no_cardac_rows_lands_as_today_and_says_unproven() {
    let fx = Fx::new("norows", "");
    let p = fx.run("prod");
    assert!(p.status.success(), "{}", out(&p));
    assert!(out(&p).contains("unproven") && out(&p).contains("no AC has a check"), "{}", out(&p));
    let r = fx.run("record");
    assert!(r.status.success(), "{}", out(&r));
    assert!(fx.log("spine.log").contains("verdict=unproven"), "{}", fx.log("spine.log"));
    assert!(!fx.log("spine.log").contains("verdict=Proven"), "never green: {}", fx.log("spine.log"));
    assert!(fx.log("cards.log").contains("unproven"), "{}", fx.log("cards.log"));
}

fn adds(fx: &Fx) -> usize { fx.log("cards.log").lines().filter(|l| l.starts_with("DEPLOY_ROLE=borg-queen add ")).count() }

// AC3 — each red AC files one child under the parent, owned by the parent's
// owner, filed by borg-queen (no bouncer: not an agent role), with the evidence.
#[test]
fn one_red_ac_files_exactly_one_child_under_the_parent_and_the_parent_is_held() {
    let fx = Fx::new("child", &format!("{}{}", row("ac-4683-1", "/ok.txt", "ok"), row("ac-4683-2", "/ok.txt", "nope")));
    let _ = fx.run("prod");
    let r = fx.run("record");
    assert!(!r.status.success(), "the parent is held: accept must not run: {}", out(&r));
    assert_eq!(adds(&fx), 1, "{}", fx.log("cards.log"));
    let log = fx.log("cards.log");
    let add = log.lines().find(|l| l.starts_with("DEPLOY_ROLE=borg-queen add ")).unwrap();
    assert!(add.contains("ac-4683-2") && !add.contains("ac-4683-1 "), "{add}");
    assert!(add.contains("--parent 4683") && add.contains("--owner kade"), "{add}");
    assert!(log.contains("## Experience") && log.contains("## AC") && log.contains("nope"), "evidence and the AC floor in the desc: {log}");
}

#[test]
fn an_unmeasured_ac_files_a_child_too() {
    let fx = Fx::new("child-unmeasured", &row("ac-4683-1", "/missing.txt", "ok"));
    let _ = fx.run("prod");
    let r = fx.run("record");
    assert!(!r.status.success(), "{}", out(&r));
    assert_eq!(adds(&fx), 1, "{}", fx.log("cards.log"));
}

#[test]
fn negative_proof_a_rerun_does_not_file_a_second_child_for_the_same_ac() {
    let fx = Fx::new("rerun", &row("ac-4683-2", "/ok.txt", "nope"));
    let _ = fx.run("prod");
    let _ = fx.run("record");
    let title = fx.log("cards.log").lines().find(|l| l.starts_with("DEPLOY_ROLE=borg-queen add ")).unwrap()
        .trim_start_matches("DEPLOY_ROLE=borg-queen add ").split(" --").next().unwrap().to_string();
    std::fs::write(fx.dir.join("view.txt"), format!(
        "#4683 Wire borg-prove into the land\n  Status:   WIP\n  Owner:    Kade\n  Children (1):\n    #4700 [Later] {title}\n")).unwrap();
    let _ = fx.run("record");
    assert_eq!(adds(&fx), 1, "the child exists already: {}", fx.log("cards.log"));
}

#[test]
fn negative_proof_a_parent_with_no_owner_refuses_rather_than_filing_an_orphan() {
    let fx = Fx::new("noowner", &row("ac-4683-2", "/ok.txt", "nope"));
    std::fs::write(fx.dir.join("view.txt"), "#4683 x\n  Status:   WIP\n  Desc:\n    ## AC\n    - [ ] ac-4683-2  check: http /ok.txt nope\n").unwrap();
    let _ = fx.run("prod");
    let r = fx.run("record");
    assert!(!r.status.success());
    assert_eq!(adds(&fx), 0, "{}", fx.log("cards.log"));
    assert!(out(&r).contains("#4683 has no owner"), "{}", out(&r));
}

// Silas + Wren at the demo: record judged a second measurement, so a check
// that changed between the two steps filed the wrong children.
#[test]
fn negative_proof_record_judges_what_prod_measured_not_a_second_look() {
    let fx = Fx::new("toctou", &row("ac-4683-2", "/ok.txt", "nope"));
    let p = fx.run("prod");
    assert!(!p.status.success());
    // prod changes between the two steps: a second look would read met
    std::fs::write(fx.dir.join("view.txt"), format!("{VIEW}{}{FIELDS}", row("ac-4683-2", "/ok.txt", "ok"))).unwrap();
    let r = fx.run("record");
    assert!(!r.status.success(), "record must hold accept on prod's red: {}", out(&r));
    assert!(fx.log("spine.log").contains("verdict=Not met"), "{}", fx.log("spine.log"));
    assert_eq!(adds(&fx), 1, "{}", fx.log("cards.log"));
}

#[test]
fn negative_proof_record_with_no_prod_results_records_nothing_and_holds_accept() {
    let fx = Fx::new("noprod", &row("ac-4683-1", "/ok.txt", "ok"));
    let r = fx.run("record");
    assert!(!r.status.success(), "{}", out(&r));
    assert!(out(&r).contains("prod's results are not readable"), "{}", out(&r));
    assert!(fx.log("spine.log").is_empty() && fx.log("cards.log").is_empty());
}

#[test]
fn results_round_trip_with_tabs_and_newlines_in_the_evidence() {
    use borg_prove::*;
    let c = Check { ac: "ac-1".into(), statement: "a\tb".into(), kind: "http".into(), target: "/x".into(), expect: "ok\\n".into() };
    let r = vec![(c, Outcome::NotMet("line1\nline2\twith tab".into()))];
    assert_eq!(results_from_tsv(&results_to_tsv(&r)).unwrap(), r);
    assert!(results_from_tsv("garbage\n").is_err());
}

// Silas + Abby at the demo: the children parse stopped at the first line it
// did not know, so a format change would silently file a second child.
#[test]
fn negative_proof_children_the_board_states_but_cannot_be_read_refuse_rather_than_duplicate() {
    let fx = Fx::new("children-unread", &row("ac-4683-2", "/ok.txt", "nope"));
    std::fs::write(fx.dir.join("view.txt"),
        "#4683 x\n  Status:   WIP\n  Owner:    Kade\n  Desc:\n    ## AC\n    - [ ] ac-4683-2  check: http /ok.txt nope\n  Children (2):\n    #4700 [Later] one\n    4701 Later two\n").unwrap();
    let _ = fx.run("prod");
    let r = fx.run("record");
    assert!(!r.status.success());
    assert_eq!(adds(&fx), 0, "{}", fx.log("cards.log"));
    assert!(out(&r).contains("could not be read"), "{}", out(&r));
}

#[test]
fn child_titles_reads_the_stated_children_and_none_is_an_empty_list() {
    use borg_prove::child_titles;
    assert_eq!(child_titles("#1 x\n  Owner: Kade\n"), Some(vec![]));
    assert_eq!(child_titles("  Children (1):\n    #4700 [Later] a title\n  Comments (0):\n"), Some(vec!["a title".to_string()]));
    assert_eq!(child_titles("  Children (2):\n    #4700 [Later] a\n"), None);
}

// #4687 (Wren at #4683's demo) — a child that cannot be filed is named on the
// parent, not only on stderr: Jeff reads the card, never the step log.
#[test]
fn a_child_card_that_cannot_be_filed_is_named_on_the_parent() {
    let fx = Fx::new("child-refused", &row("ac-4683-2", "/ok.txt", "nope"));
    // the board refuses the add: cards answers view, then exits 1 on add
    let d = fx.dir.display().to_string();
    std::fs::write(fx.dir.join("cards"), format!(
        "#!/bin/sh\ncase \"$1\" in view) cat {d}/view.txt ;; add) exit 1 ;; *) echo \"DEPLOY_ROLE=$DEPLOY_ROLE $*\" >> {d}/cards.log ;; esac\n")).unwrap();
    let _ = fx.run("prod");
    let r = fx.run("record");
    assert!(!r.status.success(), "{}", out(&r));
    let log = fx.log("cards.log");
    assert!(log.lines().any(|l| l.contains(" comment 4683 ") && l.contains("ac-4683-1") && l.contains("could not be filed")),
        "the parent names the AC whose child was not filed: {log}");
}

#[test]
fn negative_proof_a_parent_with_no_owner_says_so_on_the_parent() {
    let fx = Fx::new("noowner-comment", &row("ac-4683-2", "/ok.txt", "nope"));
    std::fs::write(fx.dir.join("view.txt"), "#4683 x\n  Status:   WIP\n  Desc:\n    ## AC\n    - [ ] ac-4683-2  check: http /ok.txt nope\n").unwrap();
    let _ = fx.run("prod");
    let _ = fx.run("record");
    let log = fx.log("cards.log");
    assert!(log.lines().any(|l| l.contains(" comment 4683 ") && l.contains("no owner")), "{log}");
}

// #4760 — the land runs record (src/main.rs): run by the pipeline's account, it
// hands itself to Borg Queen's through sudo, the run and home in its args.
fn record_as(fx: &Fx, sudo_exit: u8) -> Output {
    let d = fx.dir.display().to_string();
    let sudo = fx.dir.join("sudo");
    std::fs::write(&sudo, format!("#!/bin/sh\necho \"$*\" > {d}/sudo.args\nexit {sudo_exit}\n")).unwrap();
    Command::new("chmod").arg("+x").arg(&sudo).status().unwrap();
    Command::new(env!("CARGO_BIN_EXE_borg-prove"))
        .args(["record", "4683", "kade"])
        .env("BORG_SUDO", &sudo)
        .env("BORG_QUEEN_ACCOUNT", "chorus-borg-queen")
        .env("DAG_RUN_ID", "run7")
        .env("CHORUS_HOME", "/h")
        .output()
        .unwrap()
}

#[test]
fn record_by_the_pipeline_runs_as_borg_queen_through_sudo() {
    let fx = Fx::new("hop", &row("ac-4683-1", "/ok.txt", "ok"));
    let r = record_as(&fx, 0);
    assert!(r.status.success(), "{}", out(&r));
    let args = fx.log("sudo.args");
    assert!(args.starts_with("-n -u chorus-borg-queen ") && args.contains(" record 4683 kade run7 /h"), "{args}");
}

// NEGATIVE PROOF: sudo refused (no sudoers line) holds accept, never records as the pipeline
#[test]
fn negative_proof_record_that_cannot_become_borg_queen_holds_accept() {
    let fx = Fx::new("hop-refused", &row("ac-4683-1", "/ok.txt", "ok"));
    let r = record_as(&fx, 1);
    assert!(!r.status.success(), "{}", out(&r));
    assert!(out(&r).contains("accept held") && out(&r).contains("sudoers"), "{}", out(&r));
    assert!(fx.log("spine.log").is_empty(), "nothing recorded: {}", fx.log("spine.log"));
}

// #4760 review (Abby): a card text from a file reads AC the same way as the
// board's view: a comment's checkbox after `  Domains:` is never an AC
#[test]
fn a_card_text_file_reads_ac_like_the_board_view() {
    let fx = Fx::new("descfile", &row("ac-4683-1", "/ok.txt", "ok"));
    let p = Command::new(env!("CARGO_BIN_EXE_borg-prove"))
        .args(["prod", "4683", "kade"])
        .env("BORG_CARD_DESC", fx.dir.join("view.txt"))
        .env("BORG_PROD_BASE", format!("file://{}", fx.dir.join("prod").display()))
        .env("BORG_RESULTS_DIR", fx.dir.join("results"))
        .env("DAG_RUN_ID", "run9")
        .output().unwrap();
    assert!(p.status.success() && out(&p).contains("Proven"), "the comment's /nope.txt check was not read: {}", out(&p));
}
