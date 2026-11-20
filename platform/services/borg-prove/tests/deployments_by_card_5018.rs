// @test-type: unit
// @domain: builds
//! #5018 — the land's prove reads the card's Deployment rows by card. The door's bare
//! list is one page of 100 rows; after #4555's land added 36, #5018's own row was the
//! 102nd, so its land failed "missing deployment manifest" twice (11-20 11:20, 11:21).
//! A fake door here pages like the real one: the bare list stops at 100 rows.
use borg_prove::{deployments_url, parse_deployments_for_card_checked};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;

fn row(card: u32, unit: &str) -> String {
    format!(r#"{{"name":"dep-{card}-{unit}","cardId":"{card}","deployedUnit":"{unit}","cdhash":"ab"}}"#)
}

/// Serves one request: the bare list returns the first 100 of 101 rows (100 other
/// card rows, then 5018's as the 101st); `?cardId=5018` returns 5018's row.
fn fake_door() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Some(Ok(mut s)) = l.incoming().next() {
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).unwrap();
            let mut all: Vec<String> = (0..100).map(|i| row(4555, &format!("u{i}"))).collect();
            all.push(row(5018, "werk-deploy"));
            let rows: Vec<String> = if line.contains("cardId=5018") {
                all.into_iter().filter(|r| r.contains("\"cardId\":\"5018\"")).collect()
            } else {
                all.into_iter().take(100).collect()
            };
            let body = format!(r#"{{"data":[{}]}}"#, rows.join(","));
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    addr
}

fn get(url: &str) -> String {
    let o = Command::new("curl").args(["-s", "--max-time", "5", url]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn the_prove_finds_the_cards_row_past_the_first_page() {
    let door = fake_door();
    let recs = parse_deployments_for_card_checked(&get(&deployments_url(&door, 5018)), 5018).unwrap();
    assert_eq!(recs.len(), 1, "{recs:?}");
    assert_eq!(recs[0].deployed_unit, "werk-deploy");
}

#[test]
fn negative_proof_the_bare_list_misses_it_the_1120_state() {
    let door = fake_door();
    let recs = parse_deployments_for_card_checked(&get(&format!("{door}/v1/cicd/deployments")), 5018).unwrap();
    assert!(recs.is_empty(), "the bare page must not reach row 101: {recs:?}");
}

#[test]
fn the_url_names_the_card_and_tolerates_a_trailing_slash() {
    assert_eq!(deployments_url("http://127.0.0.1:3360/", 5018), "http://127.0.0.1:3360/v1/cicd/deployments?cardId=5018");
}
