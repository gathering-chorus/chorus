// @test-type: unit
// @domain: code
// #4683 — borg-prove prod and record run inside cicd-land, before accept
// (src/main.rs, src/lib.rs). What Jeff sees: a card whose ACs are all met in prod is
// accepted with prover borg-queen on the card; any AC not met or not measured
// stops the land before accept; a card with no CardAC rows lands as today and
// says "unproven" on the card, never green.
use std::path::PathBuf;
use std::process::{Command, Output};

const HEADER: &str = "?ac\t?statement\t?kind\t?target\t?expect\n";

struct Fx { dir: PathBuf }

impl Fx {
    fn new(name: &str, rows: &str) -> Fx {
        let dir = std::env::temp_dir().join(format!("borg-4683-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("prod")).unwrap();
        std::fs::write(dir.join("checks.tsv"), format!("{HEADER}{rows}")).unwrap();
        std::fs::write(dir.join("prod/ok.txt"), "status ok\n").unwrap();
        std::fs::write(dir.join("view.txt"), "#4683 Wire borg-prove into the land\n  Status:   WIP\n  Owner:    Kade\n").unwrap();
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
            .env("BORG_CHECKS_TSV", self.dir.join("checks.tsv"))
            .env("BORG_PROD_BASE", format!("file://{}", self.dir.join("prod").display()))
            .env("BORG_CHORUS_LOG", self.dir.join("chorus-log"))
            .env("BORG_CARDS", self.dir.join("cards"))
            .output()
            .unwrap()
    }
    fn log(&self, name: &str) -> String { std::fs::read_to_string(self.dir.join(name)).unwrap_or_default() }
}

fn out(o: &Output) -> String { format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)) }
fn row(ac: &str, target: &str, expect: &str) -> String { format!("<https://x#{ac}>\t\"{ac}\"\t\"http\"\t\"{target}\"\t\"{expect}\"\n") }

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
    assert!(out(&p).contains("unproven") && out(&p).contains("no CardAC rows"), "{}", out(&p));
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
    let r = fx.run("record");
    assert!(!r.status.success(), "{}", out(&r));
    assert_eq!(adds(&fx), 1, "{}", fx.log("cards.log"));
}

#[test]
fn negative_proof_a_rerun_does_not_file_a_second_child_for_the_same_ac() {
    let fx = Fx::new("rerun", &row("ac-4683-2", "/ok.txt", "nope"));
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
    std::fs::write(fx.dir.join("view.txt"), "#4683 x\n  Status:   WIP\n").unwrap();
    let r = fx.run("record");
    assert!(!r.status.success());
    assert_eq!(adds(&fx), 0, "{}", fx.log("cards.log"));
    assert!(out(&r).contains("#4683 has no owner"), "{}", out(&r));
}
