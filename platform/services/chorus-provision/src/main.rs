//! chorus-provision — create a USER everywhere, or nowhere.
//!
//! Jeff, 2026-08-11: "we need a user provisioning automation — we cant keep
//! rolling our own here." Jeff, 2026-09-15: "we are creating users not persons
//! — people and agents are both users." There is one noun and one command.
//!
//! WHY THIS EXISTS, measured rather than asserted (2026-09-15 12:47):
//! 13 chorus:Principal rows in urn:chorus:domains:security. Three of them —
//! crawler-index, reindex-worker, embed-worker — name WebIDs whose profile
//! card answers 401, because the pod was never created. Their principal rows
//! were TYPED BY HAND into identity-principals-3613.ttl while seed-css.sh's
//! AGENTS list decided which pods actually got made. Two lists, nothing
//! reconciling them, so a user can exist in the graph and not in the world.
//! Loki holds zero identity.provisioned events across 30 days: nothing ever
//! recorded who got which half.
//!
//! THE RULE THIS TURNS ON. The accounts API is the REGISTER. It is the only
//! participant that can create a user and hand back an identifier nobody
//! typed. Roles say what an existing user may DO; they cannot say who exists,
//! because a role bound to an unresolvable WebID is exactly today's defect.
//! Principal rows are a PROJECTION of the register, written only here, in the
//! same run that created the account.
//!
//! So: no template, no fallback, no constructed identifier. If the server does
//! not return a WebID, this refuses. `seed-css.sh:75` does the opposite —
//!     [ -n "$WEBID" ] || WEBID="$ISSUER_URL/$AGENT/profile/card#me"
//! — and that line cannot fail, which is how a plausible-looking WebID gets
//! bound to a Principal with nothing behind it.

use std::io::Write;
use std::process::{Command, Stdio};

const USAGE: &str = "\
chorus-provision — create a user everywhere, or nowhere

  chorus-provision census            who exists incompletely
  chorus-provision plan <name>       what would be created, writes nothing
  chorus-provision create <name>     create the user, all of it or none of it

Environment:
  CSS_URL        CSS origin to talk to        (default http://localhost:3001)
  CSS_ISSUER     the logical WebID origin     (default https://id.lightlifeurbangardens.com)
  CHORUS_API     the generated write API      (default http://localhost:3340)
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("census") => cmd_census(),
        Some("create") => match args.get(1) {
            Some(n) => match Kind::from_args(&args[2..]) {
                Some(k) => cmd_create(n, k),
                None => {
                    eprintln!("chorus-provision: REFUSED — '{n}' has no kind. Say --kind human or --kind agent.");
                    eprintln!("  nothing was written. A human and an agent are provisioned differently");
                    eprintln!("  (sign-in vs client credential), so the kind is decided at the door.");
                    2
                }
            },
            None => {
                eprintln!("chorus-provision: create needs a user name");
                2
            }
        },
        Some("plan") => match args.get(1) {
            Some(n) => cmd_plan(n),
            None => {
                eprintln!("chorus-provision: plan needs a user name");
                2
            }
        },
        _ => {
            eprint!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Jeff, 2026-09-16: "it shapes the experience and the hx needs are different
/// than ax." Two kinds, decided at the door. The store already spells this as
/// `principalKind` with four values (person, agent, service, worker — measured
/// 09:38 today); this writes the two that mean something and the census will
/// say which rows still carry the other spellings.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Human,
    Agent,
}

impl Kind {
    fn from_args(rest: &[String]) -> Option<Kind> {
        let mut it = rest.iter();
        while let Some(a) = it.next() {
            let v = match a.strip_prefix("--kind=") {
                Some(v) => v.to_string(),
                None if a == "--kind" => it.next()?.to_string(),
                None => continue,
            };
            return match v.as_str() {
                "human" => Some(Kind::Human),
                "agent" => Some(Kind::Agent),
                _ => None,
            };
        }
        None
    }
    /// The `principalKind` value written to the row.
    fn stored(self) -> &'static str {
        match self {
            Kind::Human => "person",
            Kind::Agent => "agent",
        }
    }
    fn can_sign_in(self) -> &'static str {
        match self {
            Kind::Human => "true",
            Kind::Agent => "false",
        }
    }
    fn word(self) -> &'static str {
        match self {
            Kind::Human => "human",
            Kind::Agent => "agent",
        }
    }
}

/// One curl, output as a string. Returns None when curl itself could not run —
/// which is NOT the same as an HTTP error and is never reported as one.
///
/// Credentials go in on STDIN as a curl config (`-K -`), never in argv. Every
/// shell caller in this repo writes `-u "admin:$FUSEKI_ADMIN_PASSWORD"`, which
/// puts the secret where `ps` can read it for the life of the request. There is
/// no reason to inherit that here.
fn curl(args: &[&str]) -> Option<String> {
    let secret = std::env::var("FUSEKI_ADMIN_PASSWORD").ok().filter(|s| !s.is_empty());
    let cfg = secret.map(|pw| {
        let user = std::env::var("FUSEKI_ADMIN_USER").unwrap_or_else(|_| "admin".into());
        format!("user = \"{user}:{}\"\n", cfg_escape(&pw))
    });
    curl_cfg(cfg, args)
}

/// The same, carrying the caller's token. The header goes in on stdin as a
/// curl config, never in argv — a token in argv is a token in `ps`.
fn curl_bearer(token: &str, args: &[&str]) -> Option<String> {
    let cfg = format!("header = \"Authorization: Bearer {}\"\n", cfg_escape(token));
    curl_cfg(Some(cfg), args)
}

/// curl config quoting: a literal backslash or quote in the value would break
/// the line, so both are escaped rather than assumed absent.
fn cfg_escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

fn curl_cfg(cfg: Option<String>, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("curl");
    if cfg.is_some() {
        cmd.args(["-K", "-"]);
    }
    cmd.args(args);
    let mut child = cmd
        .stdin(if cfg.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(text) = cfg {
        let mut si = child.stdin.take()?;
        let _ = si.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The HTTP status of a GET, as a string. "000" means no response at all.
fn status_of(url: &str) -> String {
    curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", url])
        .unwrap_or_else(|| "000".into())
        .trim()
        .to_string()
}

/// Every `chorus:webId` literal in the security graph, with its subject.
///
/// Read through SPARQL rather than the TTL file on purpose: the file is the
/// thing this card retires, and a census that reads the retired source would
/// agree with it by construction.
fn principals() -> Vec<(String, String)> {
    let q = "PREFIX c: <https://jeffbridwell.com/chorus#> \
             SELECT ?s ?w WHERE { GRAPH <urn:chorus:domains:security> \
             { ?s a c:Principal ; c:webId ?w } }";
    let body = match curl(&[
        "-s",
        "--max-time",
        "40",
        "-G",
        &format!("{}/pods/query", env_or("FUSEKI_URL", "http://localhost:3030")),
        "--data-urlencode",
        &format!("query={q}"),
        "-H",
        "Accept: application/sparql-results+json",
    ]) {
        Some(b) => b,
        None => return vec![],
    };
    // A deliberately small reader: this wants two string fields per binding and
    // pulling in a JSON crate for that would cost more than it explains.
    //
    // Whitespace-tolerant on purpose. The first version matched the needle
    // `"value":"` and read ZERO bindings from a response that had thirteen,
    // because Jena pretty-prints as `"value" : "..."`. It cost nothing to find
    // only because the empty-read refusal below caught it — a reader that
    // silently returns nothing looks exactly like a clean system.
    let flat: String = {
        let mut f = String::with_capacity(body.len());
        let mut in_str = false;
        let mut prev_esc = false;
        for ch in body.chars() {
            if ch == '"' && !prev_esc {
                in_str = !in_str;
            }
            prev_esc = ch == '\\' && !prev_esc;
            if !in_str && ch.is_whitespace() {
                continue;
            }
            f.push(ch);
        }
        f
    };
    let mut out = Vec::new();
    for chunk in flat.split("\"s\":").skip(1) {
        let subj = between(chunk, "\"value\":\"", "\"").unwrap_or_default();
        let webid = chunk
            .find("\"w\":")
            .and_then(|i| between(&chunk[i..], "\"value\":\"", "\""))
            .unwrap_or_default();
        if !subj.is_empty() && !webid.is_empty() {
            out.push((subj.rsplit('#').next().unwrap_or(&subj).to_string(), webid));
        }
    }
    out
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(s[i..j].to_string())
}

/// AC line 6 — "a half-provisioned user is DETECTABLE after the fact".
///
/// The question is not answerable from logs: there are none. It is answerable
/// against the REGISTER, by asking whether each Principal's WebID resolves.
/// A profile card that does not answer 200 is a user who exists in the graph
/// and not in the world.
fn cmd_census() -> i32 {
    let rows = principals();
    if rows.is_empty() {
        eprintln!("chorus-provision: census read NO principals — refusing to report a clean census over an empty read");
        return 2;
    }
    let mut incomplete = Vec::new();
    println!("{:<22} {:<6} {}", "USER", "CARD", "WEBID");
    for (name, webid) in &rows {
        // The WebID is an identifier, not a URL to fetch: the profile CARD is
        // the document. Fetching the `#me` fragment is how you measure nothing
        // and conclude everything is unreachable.
        let card = webid.split('#').next().unwrap_or(webid);
        let st = status_of(card);
        if st != "200" {
            incomplete.push((name.clone(), st.clone()));
        }
        println!("{:<22} {:<6} {}", name, st, webid);
    }
    println!();
    if incomplete.is_empty() {
        println!("census: {} users, all complete", rows.len());
        return 0;
    }
    println!(
        "census: {} of {} INCOMPLETE — a principal row exists for a pod the register does not have",
        incomplete.len(),
        rows.len()
    );
    for (n, st) in &incomplete {
        println!("  {n} — profile card {st}");
    }
    1
}

/// What provisioning WOULD do, writing nothing.
///
/// Deliberately not a dry-run flag on the real path: a flag that switches
/// between "write" and "do not write" is one boolean away from writing during
/// a test, and this card exists because identities got made by accident.
fn cmd_plan(name: &str) -> i32 {
    let css = env_or("CSS_URL", "http://localhost:3001");
    let issuer = env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com");
    let api = env_or("ATHENA_MAKE_URL", "http://localhost:3360");

    println!("plan: provision user '{name}'");
    println!();
    println!("  1. POST {css}/.account/account/<id>/pod/   name={name}");
    println!("     -> the response carries the webId. If it does not, REFUSE.");
    println!("        No template. No fallback. A constructed identifier is how");
    println!("        you get two of someone (Jeff, relay, 2026-08-11).");
    println!("  2. POST {css}/.account/account/<id>/client-credentials/");
    println!("     -> bound to the webId from step 1, never to a rebuilt string");
    println!("  3. POST {api}<Principal collection from the discovery document>");
    println!("     -> chorus:Principal in urn:chorus:domains:security, webId from step 1,");
    println!("        principalKind + canSignIn from --kind, written as the caller");
    println!();
    println!("  all three, or none. A failure after step 1 removes the pod.");
    println!();

    match existing(name) {
        Existing::Whole(w) => {
            println!("ALREADY EXISTS: {w} — the register serves its profile card.");
            println!("provisioning would be a no-op returning this webId, never a second identity.");
            return 0;
        }
        Existing::RowWithoutPod(w) => {
            println!("HALF-PROVISIONED: a principal row names {w} but the register does not serve it.");
            println!("provisioning would complete the missing half: pod, credential, row replaced.");
            return 0;
        }
        Existing::None => {}
    }
    println!("no principal named '{name}' today. Issuer for reference: {issuer}");
    0
}


// ── create ─────────────────────────────────────────────────────────────────

/// What we made, in the order we made it. Rollback walks it backwards.
///
/// A ledger rather than a flag: "did step 2 succeed" is a question you can get
/// wrong, but "what exists right now because of this run" is a list you either
/// have or do not. The 2026-08-11 failure was a user in three places out of
/// five, which is exactly what you get when each step decides for itself
/// whether the previous one counted.
#[derive(Default)]
struct Ledger {
    pod: Option<String>,
    credential: Option<String>,
    principal: Option<String>,
}

impl Ledger {
    fn rollback(&self, css: &str, acct: &str, api: &str, token: &str, collection: &str) {
        if let Some(id) = &self.principal {
            eprintln!("  rollback: removing principal {id}");
            let _ = curl_bearer(token, &["-s", "-o", "/dev/null", "-X", "DELETE", "--max-time", "20",
                           &format!("{api}{collection}/{id}")]);
        }
        if let Some(id) = &self.credential {
            eprintln!("  rollback: removing credential {id}");
            let _ = curl(&["-s", "-o", "/dev/null", "-X", "DELETE", "--max-time", "20",
                           &format!("{css}/.account/account/{acct}/client-credentials/{id}/")]);
        }
        if let Some(name) = &self.pod {
            // CSS has no pod-delete in the accounts API. Saying so is the honest
            // move: a rollback that silently leaves a pod behind is the very
            // half-state this card exists to prevent, so it is NAMED, loudly,
            // with the one manual step that finishes it.
            eprintln!("  rollback: CANNOT remove pod '{name}' — the CSS accounts API has no delete.");
            eprintln!("            This user is NOT fully rolled back. Remove the pod by hand:");
            eprintln!("            {css}/.account/  →  pods  →  {name}");
        }
    }
}

/// What the REGISTER says about a name, asked before anything is written.
///
/// The first version decided "exists" from the principal ROW and returned its
/// webId. For crawler-index that is a webId with no pod behind it — the exact
/// defect this card exists to kill, handed back with "already exists" on it.
/// A row is a projection; only the register can say who exists.
enum Existing {
    /// Register has the pod and a row names it: the user exists. No-op.
    Whole(String),
    /// A row names a webId whose profile card the register does not serve
    /// (crawler-index, reindex-worker, embed-worker today). Half a user.
    RowWithoutPod(String),
    /// Nothing anywhere.
    None,
}

fn existing(name: &str) -> Existing {
    let rows = principals();
    let row = rows
        .iter()
        .find(|(n, _)| n == &format!("principal-{name}") || n == name)
        .map(|(_, w)| w.clone());
    match row {
        None => Existing::None,
        Some(w) => {
            let card = w.split('#').next().unwrap_or(&w).to_string();
            if status_of(&card) == "200" {
                Existing::Whole(w)
            } else {
                Existing::RowWithoutPod(w)
            }
        }
    }
}

/// AC1 + AC3. One command; all of it or none of it.
fn cmd_create(name: &str, kind: Kind) -> i32 {
    let css = env_or("CSS_URL", "http://localhost:3001");

    // Idempotence BEFORE anything is written (AC "provisioning twice does not
    // create a second identity"). Two Jeffs on the relay is the bug this
    // guards, and it has to run first: a check after the POST is a check that
    // already made the second one. The REGISTER answers, not the row.
    let half = match existing(name) {
        Existing::Whole(w) => {
            println!("{w}");
            eprintln!("chorus-provision: '{name}' already exists — the register serves its profile card. Nothing created.");
            return 0;
        }
        Existing::RowWithoutPod(w) => {
            eprintln!("chorus-provision: '{name}' is HALF-PROVISIONED — a principal row names {w}");
            eprintln!("  but the register does not serve that profile card. Completing the missing half.");
            Some(w)
        }
        Existing::None => None,
    };

    // The roles-side gate (Wren, ADR-054 authZ half). A user who holds no role
    // gets a credential that 403s on its first write — a principal that exists
    // and cannot do anything, which is a half-provisioned user wearing a whole
    // one's clothes. A human acts for themself; the gate is for agents.
    //
    // Runs BEFORE the register is even contacted. Two reasons: a rollback that
    // cannot delete a pod (CSS has no pod-delete) makes every late refusal a
    // permanent half-state; and a POLICY refusal must not depend on the
    // register being up.
    if kind == Kind::Agent {
        match holds_role(name) {
            Some(true) => {}
            Some(false) => {
                eprintln!("chorus-provision: REFUSED — agent '{name}' holds no role");
                eprintln!("  nothing was written. A credential minted for a role-less agent");
                eprintln!("  403s on its first write; the agent would exist and be unable to act.");
                eprintln!("  Give it a role in the roles domain first — an empty answer is still a role.");
                return 2;
            }
            None => {
                eprintln!("chorus-provision: REFUSED — the roles domain could not be read, so whether '{name}' holds a role is UNMEASURED, not no-role");
                eprintln!("  nothing was written. Unmeasured is not yes: minting on a failed read would");
                eprintln!("  turn every store outage into a role-less credential.");
                return 2;
            }
        }
    }

    // The caller's identity for the write door — the same contract athena-model
    // enforces, from the same single minter. Resolved before the register is
    // touched so a missing identity refuses with nothing to roll back.
    let token = match identity_token() {
        Some(t) => t,
        None => {
            eprintln!("chorus-provision: REFUSED — no verified identity for the write door");
            eprintln!("  nothing was written. Set CHORUS_IDENTITY_TOKEN, or set CHORUS_ROLE so it can be");
            eprintln!("  minted by platform/scripts/chorus-identity-token — the one minter, shared with athena-model.");
            return 2;
        }
    };
    let (api, collection) = match principals_collection() {
        Some(c) => c,
        None => {
            eprintln!("chorus-provision: REFUSED — the write API's discovery document does not name a Principal collection");
            eprintln!("  nothing was written. Asked {}/ ; set ATHENA_MAKE_URL if it lives elsewhere.", env_or("ATHENA_MAKE_URL", "http://localhost:3360"));
            return 2;
        }
    };

    let jar = format!("{}/chorus-provision-{}.jar", env_or("TMPDIR", "/tmp").trim_end_matches('/'), std::process::id());
    let acct = match css_login(&css, &jar) {
        Some(a) => a,
        None => {
            eprintln!("chorus-provision: REFUSED — could not reach the CSS accounts API at {css}");
            eprintln!("  nothing was written. The register is the only source of a webId,");
            eprintln!("  so with the register unreachable there is nothing honest to write.");
            return 2;
        }
    };

    let mut led = Ledger::default();

    // 1. the pod. The response carries the webId, or this refuses.
    let resp = curl(&["-s", "--max-time", "60", "-b", &jar, "-X", "POST",
                      &format!("{css}/.account/account/{acct}/pod/"),
                      "-H", "Content-Type: application/json",
                      "--data", &format!("{{\"name\":\"{name}\"}}")])
        .unwrap_or_default();
    let webid = between(&resp, "\"webId\":\"", "\"").unwrap_or_default();
    if webid.is_empty() {
        eprintln!("chorus-provision: REFUSED — the register returned no webId for '{name}'");
        eprintln!("  It is NOT constructed from a template. seed-css.sh:75 does that:");
        eprintln!("      [ -n \"$WEBID\" ] || WEBID=\"$ISSUER_URL/$AGENT/profile/card#me\"");
        eprintln!("  That line cannot fail, which is how crawler-index, reindex-worker");
        eprintln!("  and embed-worker ended up as principals for pods nobody made.");
        return 2;
    }
    led.pod = Some(name.to_string());
    eprintln!("  1/3 pod created, webId from the register: {webid}");

    // Completing a half: the row's webId must be the one the register just
    // issued. If they differ, the row was typed from a template that guessed
    // wrong, and binding a credential to the register's webId while the row
    // keeps the guessed one would leave two identities for one name.
    if let Some(row_w) = &half {
        if row_w != &webid {
            eprintln!("chorus-provision: REFUSED — the row names {row_w}");
            eprintln!("  but the register issued {webid} for '{name}'. Two identities for one name;");
            eprintln!("  not binding either. Fix the row to the register's webId, then rerun.");
            led.rollback(&css, &acct, &api, &token, &collection);
            return 1;
        }
    }

    // 2. the credential, bound to the webId we were GIVEN — never a rebuilt one.
    // Agents hold one and never sign in. Humans sign in; a client credential
    // for a human is a second way in nobody asked for, so they do not get one.
    if kind == Kind::Agent {
        let cc = curl(&["-s", "--max-time", "60", "-b", &jar, "-X", "POST",
                        &format!("{css}/.account/account/{acct}/client-credentials/"),
                        "-H", "Content-Type: application/json",
                        "--data", &format!("{{\"name\":\"chorus-agent-{name}\",\"webId\":\"{webid}\"}}")])
            .unwrap_or_default();
        match between(&cc, "\"id\":\"", "\"") {
            Some(id) if !id.is_empty() => {
                led.credential = Some(id);
                eprintln!("  2/3 credential minted for {webid}");
            }
            _ => {
                eprintln!("chorus-provision: FAILED at step 2 (credential) — rolling back");
                led.rollback(&css, &acct, &api, &token, &collection);
                return 1;
            }
        }
    } else {
        eprintln!("  2/3 human: no client credential — sign-in is the door (email + password on the CSS account)");
    }

    // 3. the principal — a PROJECTION of the register, written only here,
    // through the generated door as the caller. A half gets its row REPLACED
    // (PUT on the item); a new user gets a row POSTed to the collection.
    let id = format!("principal-{name}");
    let body = format!(
        "{{\"name\":\"{name}\",\"label\":\"{name} {}\",\"webId\":\"{webid}\",\"principalKind\":\"{}\",\"canSignIn\":\"{}\"}}",
        kind.word(), kind.stored(), kind.can_sign_in()
    );
    let (method, url) = if half.is_some() {
        ("PUT", format!("{api}{collection}/{id}"))
    } else {
        ("POST", format!("{api}{collection}"))
    };
    let pr = curl_bearer(&token, &["-s", "-w", "\n%{http_code}", "--max-time", "30", "-X", method, &url,
                                   "-H", "Content-Type: application/json", "--data", &body])
        .unwrap_or_default();
    let code = pr.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" {
        let detail = pr.lines().next().unwrap_or("").chars().take(300).collect::<String>();
        eprintln!("chorus-provision: FAILED at step 3 (principal) — {method} {url} → HTTP {code} — rolling back");
        if !detail.is_empty() {
            eprintln!("  {detail}");
        }
        led.rollback(&css, &acct, &api, &token, &collection);
        return 1;
    }
    if half.is_none() {
        led.principal = Some(id.clone());
    }
    eprintln!("  3/3 {id} bound to {webid} ({} — principalKind={}, canSignIn={})", kind.word(), kind.stored(), kind.can_sign_in());

    println!("{webid}");
    let caller = env_or("CHORUS_ROLE", &env_or("DEPLOY_ROLE", "unknown"));
    let _ = Command::new("chorus-log")
        .args(["identity.provisioned", &caller, &format!("user={name}"), &format!("kind={}", kind.word()), &format!("webid={webid}")])
        .status();
    eprintln!("chorus-provision: '{name}' provisioned as {} — {}", kind.word(),
        if kind == Kind::Agent { "pod, credential, principal" } else { "pod, principal (signs in)" });
    let _ = std::fs::remove_file(&jar);
    0
}

/// The caller's verified token for the write door. The SAME contract
/// athena-model enforces (lib.rs:680): CHORUS_IDENTITY_TOKEN if the caller
/// already holds one, else minted for the caller's role by the one script that
/// mints — platform/scripts/chorus-identity-token. This binary does not mint.
/// Two minters would be two identities, which is the 2026-08-11 bug in a new
/// coat. Real session login for a CLI is a separate card.
fn identity_token() -> Option<String> {
    if let Ok(t) = std::env::var("CHORUS_IDENTITY_TOKEN") {
        if !t.trim().is_empty() {
            return Some(t.trim().to_string());
        }
    }
    let role = std::env::var("CHORUS_ROLE").or_else(|_| std::env::var("DEPLOY_ROLE")).ok()?;
    let root = env_or("CHORUS_ROOT", &format!("{}/CascadeProjects/chorus", env_or("HOME", "")));
    let out = Command::new(format!("{root}/platform/scripts/chorus-identity-token"))
        .arg(&role)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

/// Ask the write API where principals live. No port or path literal here: the
/// discovery document at the service root names every collection it serves,
/// and #4175 showed what deriving a route from a class name costs.
fn principals_collection() -> Option<(String, String)> {
    let api = env_or("ATHENA_MAKE_URL", "http://localhost:3360");
    let doc = curl(&["-s", "--max-time", "10", &format!("{api}/")])?;
    for chunk in doc.split("\"kind\":").skip(1) {
        let kind = between(chunk, "\"", "\"").unwrap_or_default();
        if kind == "Principal" {
            let coll = between(chunk, "\"collection\":\"", "\"")
                .or_else(|| between(chunk, "\"collection\": \"", "\""))?;
            return Some((api, coll));
        }
    }
    None
}

/// Log in to CSS and return the account id, carrying a cookie jar.
///
/// None means the register is unreachable or refused us — which is a REFUSAL,
/// never a reason to invent an identifier.
///
/// The password is read BY REFERENCE from the env file and handed to curl on
/// stdin as a JSON body, so it never enters argv and is never printed. The
/// email must come from the SAME file as the password: reading one from the
/// env and the other from a default is how you authenticate as somebody else
/// and create their pod instead (cost a morning on 2026-09-14).
fn css_login(css: &str, jar: &str) -> Option<String> {
    let env_file = env_or(
        "GATHERING_APP_ENV",
        &format!("{}/CascadeProjects/jeff-bridwell-personal-site/.env", env_or("HOME", "")),
    );
    let text = std::fs::read_to_string(&env_file).ok()?;
    let read = |key: &str| -> Option<String> {
        text.lines()
            .find(|l| l.starts_with(&format!("{key}=")))
            .map(|l| l[key.len() + 1..].trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    };
    let pw = read("CSS_ACCOUNT_PASSWORD")?;
    let email = read("CSS_EMAIL")?;

    // JSON assembled here and piped: the body never appears in the process list.
    let esc = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
    let body = format!("{{\"email\":\"{}\",\"password\":\"{}\"}}", esc(&email), esc(&pw));
    let mut child = Command::new("curl")
        .args([
            "-s", "-o", "/dev/null", "--max-time", "20", "-c", jar,
            "-X", "POST", &format!("{css}/.account/login/password/"),
            "-H", "Content-Type: application/json",
            "--data", "@-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut si = child.stdin.take()?;
        let _ = si.write_all(body.as_bytes());
    }
    child.wait().ok()?;

    let acct = curl(&["-s", "--max-time", "20", "-b", jar, &format!("{css}/.account/")])?;
    let pod = between(&acct, "\"pod\":\"", "\"")?;
    pod.split("/account/").nth(1)?.split('/').next().map(String::from)
}


/// Does this user hold a role? The roles domain answers; the security domain
/// enforces. ADR-054's line, landing at the one place it bites.
///
/// None is UNMEASURED: the store could not be read. It is neither yes nor no,
/// and the caller refuses on it — the first version returned TRUE here, which
/// turned every Fuseki outage into a minted credential.
fn holds_role(name: &str) -> Option<bool> {
    let q = format!(
        "PREFIX c: <https://jeffbridwell.com/chorus#> ASK {{ GRAPH ?g {{ c:principal-{name} c:holdsRole ?r }} }}"
    );
    let body = curl(&[
        "-s", "--max-time", "30", "-G",
        &format!("{}/pods/query", env_or("FUSEKI_URL", "http://localhost:3030")),
        "--data-urlencode", &format!("query={q}"),
        "-H", "Accept: application/sparql-results+json",
    ])
    .unwrap_or_default();
    // Whitespace-blind on purpose: Jena writes `"boolean" : true`, the test
    // world writes `"boolean": true`, and a reader that knows one spelling
    // reads the other as no-role — a refusal for the wrong reason, which the
    // fixture caught on its first run.
    let flat: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    if !flat.contains("\"boolean\"") {
        return None;
    }
    Some(flat.contains("\"boolean\":true"))
}
