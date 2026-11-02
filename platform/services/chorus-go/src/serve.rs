//! #4755 — the go page's request handling, moved unchanged from main.rs
//! (Silas 2026-10-27) so tests drive `handle` in process. Only `handle` and the
//! two config readers main needs are public: nothing outside this module can
//! answer a go step without `accept`'s signature check first.
use crate::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::Command;

const PAGE: &str = include_str!("page.html");

pub fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| d.to_string())
}
fn rp_id() -> String { env_or("CHORUS_GO_RP_ID", "lightlifeurbangardens.com") }
pub fn origin() -> String { env_or("CHORUS_GO_ORIGIN", "https://lightlifeurbangardens.com") }
pub fn chorus_home() -> String { env_or("CHORUS_HOME", &format!("{}/CascadeProjects/chorus", env_or("HOME", ""))) }
fn werk_base() -> String { env_or("CHORUS_WERK_BASE", &format!("{}/CascadeProjects/chorus-werk", env_or("HOME", ""))) }

struct Req { method: String, path: String, query: String, body: Vec<u8> }

fn read_req(s: &mut TcpStream) -> Result<Req, String> {
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).ok();
    let mut r = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).map_err(|e| e.to_string())? == 0 || h == "\r\n" || h == "\n" { break; }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") { len = v.trim().parse().unwrap_or(0); }
        }
    }
    if len > 64 * 1024 { return Err("body too large".into()); }
    let mut body = vec![0; len];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    let (path, query) = target.split_once('?').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((target, String::new()));
    Ok(Req { method, path, query, body })
}

fn param(q: &str, k: &str) -> String {
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(a, _)| *a == k).map(|(_, v)| v.to_string()).unwrap_or_default()
}

fn reply(s: &mut TcpStream, code: u16, ctype: &str, body: &str) -> Result<(), String> {
    let reason = match code { 200 => "OK", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found", _ => "Error" };
    write!(s, "HTTP/1.1 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len())
        .map_err(|e| e.to_string())
}

fn redirect(s: &mut TcpStream, to: &str) -> Result<(), String> {
    write!(s, "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n")
        .map_err(|e| e.to_string())
}

pub fn handle(store: &Store, mut s: TcpStream) -> Result<(), String> {
    let req = read_req(&mut s)?;
    let path = req.path.trim_end_matches('/');
    let json_body = || serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
    let field = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let out = match (req.method.as_str(), path) {
        // #4642: one go path. The old per-run link (/go?card=&run=) lands on the one list.
        ("GET", "/go") if req.query.contains("card=") => return redirect(&mut s, "/go/approvals"),
        ("GET", "/go") | ("GET", "/go/register") | ("GET", "/go/approvals") => return reply(&mut s, 200, "text/html; charset=utf-8", PAGE),
        // #4505: Jeff's one list of asks, decided by his passkey like a go.
        ("GET", "/go/api/approvals") => pending_approvals().map(offered).and_then(|mut rows| {
            rows.extend(signed_not_sent()?);
            Ok(json!({"ok": true, "approvals": rows}))
        }),
        ("POST", "/go/api/approvals/challenge") => approval_challenge_for(store, &field(&json_body(), "row")),
        ("POST", "/go/api/approvals/decide") => {
            let v = json_body();
            decide(store, &field(&v, "row"), &field(&v, "nonce"), &field(&v, "decision"),
                &field(&v, "authData"), &field(&v, "clientDataJSON"), &field(&v, "signature"))
        }
        ("GET", "/go/api/register-options") => Ok(json!({
            "challenge": b64url(&register_challenge(&param(&req.query, "token"))), "rpId": rp_id(), "registered": store.passkey().ok().flatten().is_some()})),
        ("POST", "/go/api/register") => {
            let v = json_body();
            register(store, &field(&v, "token"), &field(&v, "credId"), &field(&v, "publicKey"),
                &unb64(&field(&v, "clientDataJSON")).unwrap_or_default(), &rp_id(), &origin())
                .map(|_| json!({"ok": true, "text": "Your phone is registered. From now on only a go it signs lands a card."}))
        }
        // #4642 recovery: Jeff's go is recorded but its run never took it (dagu busy or down).
        ("POST", "/go/api/approvals/resend") => resend_go(store, &field(&json_body(), "row")),
        _ => return reply(&mut s, 404, "text/plain", "not found"),
    };
    match out {
        Ok(v) => reply(&mut s, 200, "application/json", &v.to_string()),
        Err(e) => reply(&mut s, 403, "application/json", &json!({"ok": false, "text": e}).to_string()),
    }
}

/// The round is the werk's HEAD: the commit the demo showed.
fn round_of(role: &str, card: u64) -> Result<String, String> {
    let werk = format!("{}/{role}-{card}", werk_base());
    werk_head(&werk).map_err(|_| format!("no werk for #{card} ({role})"))
}

/// #4758 — the dag and run that hold the go of the link's run (lib go_target).
fn go_run(run: &str) -> (&'static str, String) {
    go_target(run, &run_status("cicd-go", &format!("go-{run}")))
}

/// The first line of `dagu status <dag> --run-id <run>`; empty when dagu cannot say.
fn run_status(dag: &str, run: &str) -> String {
    let dagu = env_or("CHORUS_GO_DAGU", "/opt/homebrew/bin/dagu");
    let cfg = format!("{}/platform/config/dagu.yaml", chorus_home());
    Command::new(&dagu)
        .args(["--config", &cfg, "status", dag, "--run-id", run])
        .env_remove("CHORUS_ROLE").env_remove("ROLE")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Answer the run's go step with what Jeff's phone signed. `accepter=jeff` here is
/// not a role typing Jeff's name (DEC-048): handle reaches this only after
/// verify_assertion has checked Jeff's passkey signature over this round.
/// Answer the run's go step: `go` or `no`, as Jeff (only after his passkey signature checked).
fn complete_go(run: &str, decision: &str) -> Result<(), String> {
    let dagu = env_or("CHORUS_GO_DAGU", "/opt/homebrew/bin/dagu");
    let cfg = format!("{}/platform/config/dagu.yaml", chorus_home());
    let (dag, id) = go_run(run);
    let o = Command::new(&dagu)
        .args(["--config", &cfg, "human-task", "complete", dag, "--run-id", &id, "--step", "skill_werk_go",
               "--input", "accepter=jeff", "--input", &format!("decision={decision}")])
        .env_remove("CHORUS_ROLE").env_remove("ROLE")
        .output().map_err(|e| format!("dagu: {e}"))?;
    if !o.status.success() {
        return Err(format!("signed, but the run did not take it: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(())
}

// ---- #4505: approvals. Rows live on the door (decisions domain); this process only reads them,
// verifies Jeff's passkey, writes his ApprovalDecision as principal-jeff, and replays an approved
// `cards add` argv as data. Nothing here PATCHes a row.

fn door() -> String { env_or("CHORUS_GO_DOOR", "http://localhost:3360") }
fn curl_bin() -> String { env_or("CHORUS_GO_CURL", "/usr/bin/curl") }

fn door_get(route: &str) -> Result<Vec<Value>, String> {
    let o = Command::new(curl_bin()).args(["-sf", "--max-time", "8", &format!("{}{route}?limit=10000", door())])
        .output().map_err(|e| format!("door: {e}"))?;
    if !o.status.success() { return Err(format!("the door did not answer {route}")); }
    let v: Value = serde_json::from_slice(&o.stdout).map_err(|_| format!("the door's {route} is not JSON"))?;
    Ok(v.get("data").and_then(Value::as_array).cloned().unwrap_or_default())
}

fn s_of(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o.get("@id").or_else(|| o.get("id")).and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}
/// A link may come back as a full IRI or a local name; compare on the local name.
fn local(s: &str) -> &str { s.rsplit(|c| c == '#' || c == '/' || c == ':').next().unwrap_or(s) }

/// Approvals with no decision by principal-jeff pointing at them, oldest first.
/// A decision row owned by anyone else is ignored: it decides nothing.
fn pending_approvals() -> Result<Vec<Value>, String> {
    let decided: std::collections::HashSet<String> = door_get("/decisions/approvaldecisions")?.iter()
        .filter(|d| local(&s_of(d, "ownedBy")) == "principal-jeff")
        .map(|d| local(&s_of(d, "decides")).to_string()).collect();
    let mut rows: Vec<Value> = door_get("/decisions/approvals")?.into_iter()
        .filter(|a| !decided.contains(local(&s_of(a, "name"))))
        .map(|a| json!({"row": s_of(&a, "name"), "asker": local(&s_of(&a, "ownedBy")),
            "kind": s_of(&a, "approvalKind"), "title": s_of(&a, "approvalSubject"),
            "action": s_of(&a, "approvalAction"), "at": s_of(&a, "createdAt")}))
        .collect();
    rows.sort_by(|a, b| s_of(a, "at").cmp(&s_of(b, "at")).then(s_of(a, "row").cmp(&s_of(b, "row"))));
    Ok(rows)
}

/// The row as it stands now: pending only while no jeff decision points at it.
fn row_now(row: &str) -> Result<ApprovalRow, String> {
    let all = pending_approvals()?;
    if let Some(r) = all.iter().find(|r| s_of(r, "row") == row) {
        let (kind, action) = (s_of(r, "kind"), s_of(r, "action"));
        // #4642: a go row is checked against the werk's HEAD now (the asker's werk for that card).
        let round_now = if kind == "go" {
            go_action(&action).ok().and_then(|g| round_of(s_of(r, "asker").trim_start_matches("principal-"), g.card).ok())
        } else { None };
        return Ok(ApprovalRow { id: row.to_string(), state: "pending".into(), action, kind, round_now });
    }
    let exists = door_get("/decisions/approvals")?.iter().any(|a| s_of(a, "name") == row);
    if exists { Ok(ApprovalRow { id: row.to_string(), state: "decided".into(), action: String::new(), kind: String::new(), round_now: None }) }
    else { Err(format!("no approval '{row}'")) }
}

/// #4771 AC2 — the first line of `dagu status` for a go row's run.
fn go_row_status(action: &str) -> Result<String, String> {
    let g = go_action(action)?;
    let (dag, id) = go_run(&g.run);
    Ok(run_status(dag, &id))
}

/// Whether a go row's run still waits at the go. A run cicd-supersede stopped
/// (a newer run of the card started) reads Aborted: that go is stale.
fn go_row_open(action: &str) -> Result<(), String> {
    go_run_open(&go_row_status(action)?)
}

/// The list drops a go row only when its run was superseded (Aborted). Any
/// other state that is not waiting (dagu unreadable, not there yet) stays on
/// the list with its reason, so Jeff never sees an empty list and no why.
fn offered(rows: Vec<Value>) -> Vec<Value> {
    let rows = newest_go_per_card(rows);
    rows.into_iter().filter_map(|mut r| {
        if s_of(&r, "kind") != "go" { return Some(r); }
        let status = go_row_status(&s_of(&r, "action"));
        if status.as_deref().map(superseded_status).unwrap_or(false) { return None; }
        if let Err(why) = status.and_then(|st| go_run_open(&st)) { r["state"] = json!(why); }
        Some(r)
    }).collect()
}

/// #4771 AC2 — a stale go is refused before Jeff signs and before any decision
/// is written or any run answered, pointing to the card's newest open go.
fn refuse_stale_go(row: &str, action: &str) -> Result<(), String> {
    // #4771 — one go per card: a newer go row of the card supersedes this one even while
    // dagu still has this run waiting (a run that reached its go after the newer run started).
    let card = go_action(action).map(|g| g.card).unwrap_or(0);
    let pending = pending_approvals().unwrap_or_default();
    let mine = pending.iter().find(|r| s_of(r, "row") == row).map(|r| s_of(r, "at")).unwrap_or_default();
    if let Some(newer) = pending.iter()
        .filter(|r| s_of(r, "kind") == "go" && s_of(r, "row") != row && s_of(r, "at") > mine)
        .filter(|r| go_action(&s_of(r, "action")).map(|g| g.card == card).unwrap_or(false))
        .max_by(|a, b| s_of(a, "at").cmp(&s_of(b, "at"))) {
        return Err(stale_go_refusal(card, "this run was superseded by a newer run of the card", Some(&s_of(newer, "row"))));
    }
    let status = go_row_status(action);
    let Err(why) = status.clone().and_then(|st| go_run_open(&st)) else { return Ok(()) };
    // #4771 — a go answered no by cicd-supersede reads Succeeded; to Jeff it is superseded
    let why = if status.as_deref().map(superseded_status).unwrap_or(false) {
        "this run was superseded by a newer run of the card".to_string()
    } else { why };
    let newest = pending_approvals().unwrap_or_default().into_iter()
        .filter(|r| s_of(r, "kind") == "go" && s_of(r, "row") != row)
        .filter(|r| go_action(&s_of(r, "action")).map(|g| g.card == card).unwrap_or(false))
        .filter(|r| go_row_open(&s_of(r, "action")).is_ok())
        .max_by(|a, b| s_of(a, "at").cmp(&s_of(b, "at")))
        .map(|r| s_of(&r, "row"));
    Err(stale_go_refusal(card, &why, newest.as_deref()))
}

fn approval_challenge_for(store: &Store, row: &str) -> Result<Value, String> {
    let pk = store.passkey()?.ok_or("no passkey registered yet: run `chorus-go register-link` on the Library")?;
    let r = row_now(row)?;
    if r.state != "pending" { return Err("this ask was already decided".into()); }
    if r.kind == "go" {
        refuse_stale_go(row, &r.action)?;
        let g = go_action(&r.action)?;
        if r.round_now.as_deref() != Some(g.round.as_str()) {
            return Err(format!("this go is for round {}; the demo moved on, so wait for the new one", short(&g.round)));
        }
    }
    let sha = action_sha(&r.action);
    let nonce = store.mint_approval_challenge(row, &sha)?;
    Ok(json!({"ok": true, "row": row, "nonce": nonce, "rpId": pk.rp_id, "credId": pk.cred_id,
        "challengeApprove": b64url(&approval_challenge(row, &sha, "approve", &nonce)),
        "challengeDecline": b64url(&approval_challenge(row, &sha, "decline", &nonce))}))
}

fn decide(store: &Store, row: &str, nonce: &str, decision: &str, ad: &str, cd: &str, sig: &str) -> Result<Value, String> {
    let r = row_now(row)?;
    // #4771 AC2 — the run may have been superseded between challenge and decide.
    if r.kind == "go" { refuse_stale_go(row, &r.action)?; }
    let d = decide_approval(store, nonce, decision, &r, ad, cd, sig)?;
    let sha = action_sha(&r.action);
    if let Some(g) = &d.go {
        // A go: Jeff's decision row (with the round it signed), then the run's go step.
        // The door row can fail (no identity minted for jeff): the go still counts — the land reads
        // the signed record chorus-go already wrote — and the failure is alerted, never silent.
        if let Err(why) = write_decision(&d, &sha, ad, cd, sig, Some(&g.round)) {
            alert_unrecorded(&d.row, &why);
        }
        let verb = if d.decision == "approve" { "go" } else { "no" };
        complete_go(&g.run, verb)?;
        return Ok(json!({"ok": true, "text": if verb == "go" { format!("Go signed for #{}. It's landing.", g.card) } else { format!("No signed for #{}. It's held.", g.card) }}));
    }
    // A card: Jeff's signed decision is written first, the record that must never be lost (Silas
    // 18:48); a door refusal files nothing. Then an approve replays the argv as data.
    write_decision(&d, &sha, ad, cd, sig, None)?;
    if d.decision != "approve" {
        return Ok(json!({"ok": true, "text": "Declined. Nothing was filed."}));
    }
    let o = Command::new(env_or("CHORUS_GO_CARDS", &format!("{}/platform/scripts/cards", chorus_home())))
        .args(&d.argv).env("DEPLOY_ROLE", "jeff").env_remove("CHORUS_ROLE").env_remove("ROLE")
        .output().map_err(|e| format!("cards: {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    if !o.status.success() {
        return Err(format!("approved and recorded, but cards add failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(json!({"ok": true, "text": format!("Approved. {}", out.lines().find(|l| l.contains('#')).unwrap_or("Filed.").trim())}))
}

/// Jeff's decision is his own row, written as principal-jeff, only after verify_assertion passed.
fn write_decision(d: &ApprovalDecision, sha: &str, ad: &str, cd: &str, sig: &str, round: Option<&str>) -> Result<(), String> {
    let mint = env_or("CHORUS_GO_IDENTITY_TOKEN_BIN", &format!("{}/platform/scripts/chorus-identity-token", chorus_home()));
    let t = Command::new(&mint).arg("jeff").output().map_err(|e| format!("identity mint: {e}"))?;
    let token = String::from_utf8_lossy(&t.stdout).trim().to_string();
    if !t.status.success() || token.is_empty() {
        return Err("signed, but there is no identity for jeff to record it with (Jeff's step: seed principal-jeff)".into());
    }
    let mut body = json!({"name": format!("decision-{}", d.row), "label": format!("{} {}", d.decision, d.row),
        "decides": d.row, "approvalVerdict": d.decision, "decidedAt": iso(d.at), "actionSha": sha,
        // the nonce rides with the signature so the land can re-verify a go (#4642)
        "passkeyProof": json!({"nonce": d.nonce, "authData": ad, "clientDataJSON": cd, "signature": sig}).to_string()});
    if let Some(r) = round { body["signedRound"] = json!(r); }
    // The token goes on curl's stdin (`-H @-`), never its argv: argv is readable by any account via `ps`.
    let mut child = Command::new(curl_bin()).args(["-sf", "--max-time", "8", "-X", "POST",
            "-H", "Content-Type: application/json", "-H", "@-",
            "--data-binary", &body.to_string(), &format!("{}/decisions/approvaldecisions", door())])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("door: {e}"))?;
    {
        use std::io::Write as _;
        let mut stdin = child.stdin.take().ok_or("door: no stdin")?;
        writeln!(stdin, "Authorization: Bearer {token}").map_err(|e| format!("door: {e}"))?;
    }
    let o = child.wait_with_output().map_err(|e| format!("door: {e}"))?;
    // A second decision for one ask is refused by the door (the name `decision-<row>` exists),
    // so two racing decides file at most once.
    if !o.status.success() { return Err("signed, but the door refused Jeff's decision row (already decided?)".into()); }
    Ok(())
}

fn iso(secs: u64) -> String {
    let days = (secs / 86_400) as i64; let rem = secs % 86_400;
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468; let era = z.div_euclid(146_097); let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// #4642 — what the land asks (`chorus-go check <card> <round>`): Jeff's go for this card at this
/// round, read from the door's rows and re-verified against his passkey. One go path.
pub fn land_go(store: &Store, card: u64, round: &str) -> Result<LandGo, String> {
    let Some(pk) = store.passkey()? else { return Ok(LandGo::NoPasskey) };
    let door = door_get("/decisions/approvals")
        .and_then(|a| door_get("/decisions/approvaldecisions").map(|d| (a, d)))
        .and_then(|(a, d)| land_go_from_rows(&pk, &a, &d, card, round));
    match door {
        Ok(()) => Ok(LandGo::Signed),
        // break-glass (Silas 19:27): the door has no row (a failed mint) or cannot answer; the
        // signed record chorus-go wrote before the door POST still lands, re-verified.
        Err(door_why) => land_go_from_local(&pk, store, card, round)
            .map(|_| LandGo::Signed)
            .map_err(|local_why| format!("{door_why}; {local_why} (#4642)")),
    }
}

/// Go rows Jeff approved whose run still waits at its go step: his decision is recorded, the run
/// never took it. They stay on his list until it does (Silas, Kade 19:17).
fn signed_not_sent() -> Result<Vec<Value>, String> {
    let decided_go: std::collections::HashSet<String> = door_get("/decisions/approvaldecisions")?.iter()
        .filter(|d| local(&s_of(d, "ownedBy")) == "principal-jeff" && s_of(d, "approvalVerdict") == "approve")
        .map(|d| local(&s_of(d, "decides")).to_string()).collect();
    let mut out = Vec::new();
    for a in door_get("/decisions/approvals")?.into_iter().filter(|a| s_of(a, "approvalKind") == "go") {
        let row = local(&s_of(&a, "name")).to_string();
        if !decided_go.contains(&row) { continue; }
        let Ok(g) = go_action(&s_of(&a, "approvalAction")) else { continue };
        let (dag, id) = go_run(&g.run);
        if go_run_open(&run_status(dag, &id)).is_ok() {
            out.push(json!({"row": row, "asker": local(&s_of(&a, "ownedBy")), "kind": "go", "state": "signed-not-sent",
                "title": s_of(&a, "approvalSubject"), "action": s_of(&a, "approvalAction"), "at": s_of(&a, "createdAt")}));
        }
    }
    Ok(out)
}

/// Re-send a recorded go to its run. No new signature: the recorded one is re-verified, exactly as
/// the land will, before the run's go step is answered.
fn resend_go(store: &Store, row: &str) -> Result<Value, String> {
    let pk = store.passkey()?.ok_or("no passkey registered yet")?;
    let approvals = door_get("/decisions/approvals")?;
    let a = approvals.iter().find(|a| local(&s_of(a, "name")) == row && s_of(a, "approvalKind") == "go")
        .ok_or(format!("no go approval '{row}'"))?;
    let g = go_action(&s_of(a, "approvalAction"))?;
    land_go_from_rows(&pk, &approvals, &door_get("/decisions/approvaldecisions")?, g.card, &g.round)?;
    complete_go(&g.run, "go")?;
    Ok(json!({"ok": true, "text": format!("Go sent for #{}. It's landing.", g.card)}))
}

/// A go decision the door did not record: loud on the spine and in the log (alert rules read the
/// spine); the land still has the signed local record.
fn alert_unrecorded(row: &str, why: &str) {
    eprintln!("chorus-go: approval.decision.unrecorded row={row}: {why}");
    let log = format!("{}/platform/scripts/chorus-log", chorus_home());
    let _ = Command::new(log).args(["approval.decision.unrecorded", "system", &format!("row={row}"), &format!("reason={}", why.replace(' ', "_"))]).output();
}
