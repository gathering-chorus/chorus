//! chorus-principal — create a USER everywhere, or nowhere.
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
chorus-principal — create a user everywhere, or nowhere

  chorus-principal census            who exists incompletely
  chorus-principal plan <name>       what would be created, writes nothing
  chorus-principal create <name>     create the user, all of it or none of it

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
                Some(Kind::Human) => match Person::from_args(&args[2..]) {
                    Some(p) => cmd_create(n, Kind::Human, Some(p), None),
                    None => {
                        eprintln!("chorus-principal: REFUSED — a human needs --name \"Full Name\" and --email <address>");
                        eprintln!("  nothing was written. The name is what the row is labelled and the email is");
                        eprintln!("  how they sign in; a human without either is a pod with nobody behind it.");
                        2
                    }
                },
                // #4202 — an agent's role is REQUIRED, not a flag it might
                // carry. Wren's ruling, 2026-09-17: "a principal holding no
                // role must never exist, not even briefly." Refusing at a gate
                // after the fact is the same half-made state read backwards.
                Some(Kind::Agent) => match flag(&args[2..], "--role") {
                    Some(r) if !r.is_empty() => cmd_create(n, Kind::Agent, None, Some(&r)),
                    _ => {
                        eprintln!("chorus-principal: REFUSED — agent '{n}' has no --role");
                        eprintln!("  nothing was written. An agent holding no role is refused its first");
                        eprintln!("  write, so it would exist and be unable to act. Name the role it holds;");
                        eprintln!("  a test agent can hold its own, e.g. --role role-{n}.");
                        2
                    }
                },
                Some(k) => cmd_create(n, k, None, None),
                None => {
                    eprintln!("chorus-principal: REFUSED — '{n}' has no kind. Say --kind human or --kind agent.");
                    eprintln!("  nothing was written. A human and an agent are provisioned differently");
                    eprintln!("  (sign-in vs client credential), so the kind is decided at the door.");
                    2
                }
            },
            None => {
                eprintln!("chorus-principal: create needs a user name");
                2
            }
        },
        Some("plan") => match args.get(1) {
            Some(n) => cmd_plan(n),
            None => {
                eprintln!("chorus-principal: plan needs a user name");
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

/// `--name value` or `--name=value`, the same shape Kind and Person read.
fn flag(rest: &[String], name: &str) -> Option<String> {
    let eq = format!("{name}=");
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if let Some(v) = a.strip_prefix(&eq) {
            return Some(v.to_string());
        }
        if a == name {
            return it.next().cloned();
        }
    }
    None
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
    /// The `principalKind` value written to the row. The same two words Role
    /// already uses for `roleKind` (#4175 retired HumanRole/AgentRole in their
    /// favour) — one vocabulary for "which kind of user", on both classes.
    /// The store's older spellings (person, service, worker) are a migration,
    /// not a third and fourth answer to write.
    fn stored(self) -> &'static str {
        match self {
            Kind::Human => "human",
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
/// CSS behind the tunnel only answers for its logical issuer host: on the local
/// hairpin every call carries Host + X-Forwarded-* for that issuer, exactly as
/// chorus-identity-token does. Without them the accounts API answers 500
/// "outside the configured identifier space" (measured 2026-09-16 13:31).
fn css_headers() -> Vec<String> {
    let host = env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com");
    let host = host.split("://").nth(1).unwrap_or(&host).trim_end_matches('/').to_string();
    vec![
        "-H".into(), format!("Host: {host}"),
        "-H".into(), "X-Forwarded-Proto: https".into(),
        "-H".into(), format!("X-Forwarded-Host: {host}"),
    ]
}

/// curl against CSS: the forwarded headers, plus any config on stdin.
fn curl_css(cfg: Option<String>, args: &[&str]) -> Option<String> {
    let h = css_headers();
    let mut all: Vec<&str> = h.iter().map(String::as_str).collect();
    all.extend_from_slice(args);
    curl_cfg(cfg, &all)
}

/// A human user: the name FOAF would call foaf:name, and the sign-in email.
struct Person {
    full_name: String,
    email: String,
}

impl Person {
    fn from_args(rest: &[String]) -> Option<Person> {
        let mut name = None;
        let mut email = None;
        let mut it = rest.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--name" => name = it.next().cloned(),
                "--email" => email = it.next().cloned(),
                _ => {}
            }
        }
        let full_name = name.filter(|n| !n.trim().is_empty())?;
        let email = email.filter(|e| e.contains('@'))?;
        Some(Person { full_name, email })
    }
}

/// Whose home this tool reads its own configuration from.
///
/// #4202 — creating an agent's account needs root, and under sudo HOME becomes
/// root's, where none of this exists. Trusting HOME made the caller responsible
/// for typing `sudo env HOME=...`: the tool asking a person to carry a fact it
/// already has. SUDO_USER names who invoked it and the directory service knows
/// their home. Off root, HOME is simply right.
fn owner_home() -> String {
    if is_root() {
        if let Ok(who) = std::env::var("SUDO_USER") {
            if !who.is_empty() {
                if let Some(h) = account_home(&who) {
                    return h;
                }
            }
        }
    }
    env_or("HOME", "")
}

/// The store's writer credential, BY REFERENCE: the env if set, else the same
/// file platform/scripts/fuseki-auth.sh reads. Never printed, never in argv.
fn fuseki_secret() -> Option<(String, String)> {
    let user_env = std::env::var("FUSEKI_ADMIN_USER").ok().filter(|s| !s.is_empty());
    if let Some(pw) = std::env::var("FUSEKI_ADMIN_PASSWORD").ok().filter(|s| !s.is_empty()) {
        return Some((user_env.unwrap_or_else(|| "admin".into()), pw));
    }
    let f = env_or("FUSEKI_WRITE_ENV", &format!("{}/.gathering/data/fuseki-write.env", owner_home()));
    let text = std::fs::read_to_string(f).ok()?;
    let read = |k: &str| {
        text.lines()
            .find(|l| l.starts_with(&format!("{k}=")))
            .map(|l| l[k.len() + 1..].trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let pw = read("FUSEKI_ADMIN_PASSWORD")?;
    Some((user_env.or_else(|| read("FUSEKI_ADMIN_USER")).unwrap_or_else(|| "admin".into()), pw))
}

fn curl(args: &[&str]) -> Option<String> {
    let cfg = fuseki_secret().map(|(user, pw)| format!("user = \"{user}:{}\"\n", cfg_escape(&pw)));
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
        eprintln!("chorus-principal: census read NO principals — refusing to report a clean census over an empty read");
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
    account: Option<String>,
    account_token: String,
    pod: Option<String>,
    credential: Option<String>,
    principal: Option<String>,
    /// #4202 — the credential file this run wrote into the agent's own account.
    /// On the ledger like everything else, so a later failure takes it back out
    /// rather than leaving a live secret in a home nobody finished making.
    home_cred: Option<String>,
}

impl Ledger {
    fn rollback(&self, css: &str, acct: &str, api: &str, token: &str, collection: &str) {
        if let Some(path) = &self.home_cred {
            eprintln!("  rollback: removing credential file {path}");
            let _ = std::fs::remove_file(path);
        }
        if let Some(id) = &self.principal {
            eprintln!("  rollback: removing principal {id}");
            let _ = (api, collection, token);
            let _ = curl(&["-s", "-o", "/dev/null", "--max-time", "20", "-X", "POST",
                           &format!("{}/pods/update", env_or("FUSEKI_URL", "http://localhost:3030")),
                           "-H", "Content-Type: application/sparql-update",
                           "--data-binary", &format!("PREFIX c: <https://jeffbridwell.com/chorus#> DELETE WHERE {{ GRAPH <urn:chorus:domains:security> {{ c:{id} ?p ?o }} }}")]);
        }
        if let Some(id) = &self.credential {
            eprintln!("  rollback: removing credential {id}");
            let _ = curl_css(None, &["-s", "-o", "/dev/null", "-X", "DELETE", "--max-time", "20",
                           "-b", &format!("{}/chorus-principal-{}.jar", env_or("TMPDIR", "/tmp").trim_end_matches('/'), std::process::id()),
                           &format!("{css}/.account/account/{acct}/client-credentials/{id}/")]);
        }
        if let Some(acct) = &self.account {
            // A human's own account CAN be deleted, pod and all — the accounts
            // API has DELETE on the account. So a human rollback is whole.
            eprintln!("  rollback: removing CSS account {acct} (and its pod)");
            let code = curl_css(Some(format!("header = \"Authorization: CSS-Account-Token {}\"\n", cfg_escape(&self.account_token))),
                &["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "DELETE", "--max-time", "20",
                  &format!("{css}/.account/account/{acct}/")]).unwrap_or_default();
            // Verified, not assumed: on 2026-09-16 13:36 the account delete
            // answered and the pod's profile card was still served afterwards.
            if let Some(name) = &self.pod {
                let card = format!("{}/{name}/profile/card", env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com").trim_end_matches('/'));
                let st = status_of(&card);
                if st == "200" {
                    eprintln!("  rollback: account delete answered HTTP {code}, but the register STILL serves {card}");
                    eprintln!("            This user is NOT fully rolled back: the pod name '{name}' is burned until");
                    eprintln!("            the DBA path removes that card. A rerun under this name will be refused.");
                } else {
                    eprintln!("  rollback: account delete HTTP {code}; profile card now {st} — clean");
                }
            }
            return;
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
fn cmd_create(name: &str, kind: Kind, person: Option<Person>, role: Option<&str>) -> i32 {
    let css = env_or("CSS_URL", "http://localhost:3001");

    // Idempotence BEFORE anything is written (AC "provisioning twice does not
    // create a second identity"). Two Jeffs on the relay is the bug this
    // guards, and it has to run first: a check after the POST is a check that
    // already made the second one. The REGISTER answers, not the row.
    // A pod this tool already made whose row never landed: (account id, webId
    // the register links). Set only when the register itself proves it.
    let mut resume: Option<(String, String)> = None;
    let half = match existing(name) {
        Existing::None if status_of(&format!("{}/{name}/profile/card", env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com").trim_end_matches('/'))) == "200" => {
            // The register serves a profile card for this name and no row
            // names it: a pod with nobody behind it. Measured 13:38 today on
            // debmajumdar — this tool made the account, pod and login, its row
            // write was refused, and its "rollback" turned out to be a 404 that
            // deleted nothing. If THIS tool made it, it can prove so: sign in
            // with the password it saved and read the register's own webId
            // link. Then the missing half is the row, and it is completed.
            // Anything else is refused — a second account under the same pod
            // name is a second identity.
            match (&person, saved_password(name)) {
                (Some(p), Some(pw)) => match css_login_person(&css, &jar_path(), &p.email, &pw) {
                    Some((acct, webid)) if webid.contains(&format!("/{name}/")) => {
                        eprintln!("chorus-principal: '{name}' is HALF-PROVISIONED — this tool made the account and pod ({acct}),");
                        eprintln!("  the register links {webid}, and no row names it. Completing the missing half.");
                        resume = Some((acct, webid));
                    }
                    _ => {
                        eprintln!("chorus-principal: REFUSED — the register serves a profile card for '{name}' and no principal row names it,");
                        eprintln!("  and signing in with the password this tool saved for {} did not reach a pod of that name.", p.email);
                        eprintln!("  nothing was written. Pick another name, or have the DBA path remove the card.");
                        return 2;
                    }
                },
                _ => {
                    eprintln!("chorus-principal: REFUSED — the register already serves a profile card for '{name}' and no principal row names it");
                    eprintln!("  nothing was written. Either a rollback left the card behind, or someone made the pod");
                    eprintln!("  by hand. Pick another name, or have the DBA path remove the card, then rerun.");
                    return 2;
                }
            }
            None
        }
        Existing::Whole(w) => {
            // Whole. One thing may still be wrong: the NAME. The first live run
            // for Deb Majumdar stored the label `"Deb` because a shell split a
            // quoted argument; a user who exists keeps their identity and gets
            // their name corrected, never a second row.
            if let Some(p) = &person {
                let current = label_of(&format!("principal-{name}"));
                if current.as_deref() != Some(p.full_name.as_str()) {
                    let esc = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
                    let fuseki = env_or("FUSEKI_URL", "http://localhost:3030");
                    let update = format!(
                        "PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
                         DELETE WHERE {{ GRAPH <urn:chorus:domains:security> {{ c:principal-{name} rdfs:label ?l }} }} ; \
                         INSERT DATA {{ GRAPH <urn:chorus:domains:security> {{ c:principal-{name} rdfs:label \"{}\" }} }}",
                        esc(&p.full_name)
                    );
                    let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "30", "-X", "POST",
                                      &format!("{fuseki}/pods/update"), "-H", "Content-Type: application/sparql-update",
                                      "--data-binary", &update]).unwrap_or_default();
                    if label_of(&format!("principal-{name}")).as_deref() == Some(p.full_name.as_str()) {
                        eprintln!("chorus-principal: '{name}' already exists — name corrected from {:?} to {:?}", current.unwrap_or_default(), p.full_name);
                    } else {
                        eprintln!("chorus-principal: '{name}' already exists — name correction FAILED (store HTTP {code}); label still {:?}", current.unwrap_or_default());
                        println!("{w}");
                        return 1;
                    }
                }
            }
            println!("{w}");
            eprintln!("chorus-principal: '{name}' already exists — the register serves its profile card. Nothing created.");
            return 0;
        }
        Existing::RowWithoutPod(w) => {
            eprintln!("chorus-principal: '{name}' is HALF-PROVISIONED — a principal row names {w}");
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
    if kind == Kind::Agent && role.is_none() {
        match holds_role(name) {
            Some(true) => {}
            Some(false) => {
                eprintln!("chorus-principal: REFUSED — agent '{name}' holds no role");
                eprintln!("  nothing was written. A credential minted for a role-less agent");
                eprintln!("  403s on its first write; the agent would exist and be unable to act.");
                eprintln!("  Give it a role in the roles domain first — an empty answer is still a role.");
                return 2;
            }
            None => {
                eprintln!("chorus-principal: REFUSED — the roles domain could not be read, so whether '{name}' holds a role is UNMEASURED, not no-role");
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
            eprintln!("chorus-principal: REFUSED — no verified identity for the write door");
            eprintln!("  nothing was written. Set CHORUS_IDENTITY_TOKEN, or set CHORUS_ROLE so it can be");
            eprintln!("  minted by platform/scripts/chorus-identity-token — the one minter, shared with athena-model.");
            return 2;
        }
    };
    let (api, collection) = match principals_collection() {
        Some(c) => c,
        None => {
            eprintln!("chorus-principal: REFUSED — the write API's discovery document does not name a Principal collection");
            eprintln!("  nothing was written. Asked {}/ ; set ATHENA_MAKE_URL if it lives elsewhere.", env_or("ATHENA_MAKE_URL", "http://localhost:3360"));
            return 2;
        }
    };

    let jar = jar_path();
    let mut led = Ledger::default();

    // The account. An agent's pod hangs off the ONE service account (it never
    // signs in). A human gets their OWN account: email + password, so the
    // browser sign-in is theirs and nobody else's. Jeff, 09:00 today: "the hx
    // needs are different than ax". A resumed half already has both.
    let acct = match (&person, &resume) {
        (_, Some((a, _))) => a.clone(),
        (None, _) => match css_login(&css, &jar) {
            Some(a) => a,
            None => {
                eprintln!("chorus-principal: REFUSED — could not reach the CSS accounts API at {css}");
                eprintln!("  nothing was written. The register is the only source of a webId,");
                eprintln!("  so with the register unreachable there is nothing honest to write.");
                return 2;
            }
        },
        (Some(p), None) => match css_create_account(&css, &jar, name, p) {
            Ok(a) => {
                led.account = Some(a.clone());
                led.account_token = ACCOUNT_TOKEN.with(|t| t.borrow().clone());
                eprintln!("  0/3 CSS account created for {} — password written once to ~/.chorus/identity/{name}/initial-password (mode 600)", p.email);
                a
            }
            Err(why) => {
                eprintln!("chorus-principal: REFUSED — {why}");
                eprintln!("  nothing was written.");
                return 2;
            }
        },
    };

    // 1. the pod. The response carries the webId, or this refuses. A resumed
    // half already has its pod; the webId is the one the register LINKS.
    let webid = match &resume {
        Some((_, w)) => {
            eprintln!("  1/3 pod already exists, webId from the register's link: {w}");
            w.clone()
        }
        None => {
            let resp = curl_css(css_auth_cfg(), &["-s", "--max-time", "60", "-b", &jar, "-X", "POST",
                              &format!("{css}/.account/account/{acct}/pod/"),
                              "-H", "Content-Type: application/json",
                              "--data", &format!("{{\"name\":\"{name}\"}}")])
                .unwrap_or_default();
            let webid = between(&resp, "\"webId\":\"", "\"").unwrap_or_default();
            if webid.is_empty() {
                eprintln!("chorus-principal: REFUSED — the register returned no webId for '{name}'");
                eprintln!("  It is NOT constructed from a template. seed-css.sh:75 does that:");
                eprintln!("      [ -n \"$WEBID\" ] || WEBID=\"$ISSUER_URL/$AGENT/profile/card#me\"");
                eprintln!("  That line cannot fail, which is how crawler-index, reindex-worker");
                eprintln!("  and embed-worker ended up as principals for pods nobody made.");
                led.rollback(&css, &acct, &api, &token, &collection);
                return 2;
            }
            led.pod = Some(name.to_string());
            eprintln!("  1/3 pod created, webId from the register: {webid}");
            webid
        }
    };

    // Completing a half: the row's webId must be the one the register just
    // issued. If they differ, the row was typed from a template that guessed
    // wrong, and binding a credential to the register's webId while the row
    // keeps the guessed one would leave two identities for one name.
    if let Some(row_w) = &half {
        if row_w != &webid {
            eprintln!("chorus-principal: REFUSED — the row names {row_w}");
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
        let cc = curl_css(css_auth_cfg(), &["-s", "--max-time", "60", "-b", &jar, "-X", "POST",
                        &format!("{css}/.account/account/{acct}/client-credentials/"),
                        "-H", "Content-Type: application/json",
                        "--data", &format!("{{\"name\":\"chorus-agent-{name}\",\"webId\":\"{webid}\"}}")])
            .unwrap_or_default();
        match between(&cc, "\"id\":\"", "\"") {
            Some(id) if !id.is_empty() => {
                led.credential = Some(id.clone());
                eprintln!("  2/3 credential minted for {webid}");
                // A credential that exists only at the register is a credential
                // the agent cannot use. It goes into the agent's own account,
                // now, in this run — not into whoever happened to run this.
                let secret = between(&cc, "\"secret\":\"", "\"").unwrap_or_default();
                if secret.is_empty() {
                    eprintln!("chorus-principal: FAILED at step 2 (credential) — the register returned no secret — rolling back");
                    led.rollback(&css, &acct, &api, &token, &collection);
                    return 1;
                }
                let issuer = env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com");
                match place_credential(name, &id, &secret, &webid, issuer.trim_end_matches('/')) {
                    Ok(path) => {
                        led.home_cred = Some(path.clone());
                        eprintln!("  2/3 credential written to {path} (owned by {}, 0600)", agent_account(name));
                    }
                    Err(why) => {
                        eprintln!("chorus-principal: FAILED at step 2 (credential placement) — {why} — rolling back");
                        led.rollback(&css, &acct, &api, &token, &collection);
                        return 1;
                    }
                }
            }
            _ => {
                eprintln!("chorus-principal: FAILED at step 2 (credential) — rolling back");
                led.rollback(&css, &acct, &api, &token, &collection);
                return 1;
            }
        }
    } else {
        eprintln!("  2/3 human: no client credential — sign-in is the door (email + password on the CSS account)");
    }

    // 3. the principal — a PROJECTION of the register, written only here.
    //
    // NOT through the door. Measured 2026-09-16 13:36: athena-make delegates
    // every write to the pen, and the pen refuses the security graph by design
    // (#3356 AC4: "a writer minting itself a Principal" is the priv-esc the
    // design names; the Principal registry is bootstrap/DBA only). Provisioning
    // IS that bootstrap — the one place a Principal is minted, from the
    // register's own webId, by a caller who holds the security-graph scope
    // (the token + discovery checks above still gate who may run this). Jeff,
    // 13:50: "we need to fix this right? go ahead". The row goes in the way
    // the SECURITY_SET deploy puts rows in: a governed write to the store,
    // writer credential by reference, spine event, read back before claimed.
    // Moving this behind the door is #4183 (permissions as rows).
    let id = format!("principal-{name}");
    // The label IS the name field (PrincipalShape: rdfs:label, the one required
    // text). A human's is their full name, as FOAF would name them; an agent's
    // is what it is called plus what it is.
    let label = match &person {
        Some(p) => p.full_name.clone(),
        None => format!("{name} {}", kind.word()),
    };
    let esc = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
    let update = format!(
        "PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
         DELETE WHERE {{ GRAPH <urn:chorus:domains:security> {{ c:{id} ?p ?o }} }} ; \
         INSERT DATA {{ GRAPH <urn:chorus:domains:security> {{ \
           c:{id} a c:Principal ; rdfs:label \"{}\" ; c:webId \"{}\" ; c:principalKind \"{}\" ; c:canSignIn \"{}\" }} }}",
        esc(&label), esc(&webid), kind.stored(), kind.can_sign_in()
    );
    let _ = (&api, &collection, &token);
    let fuseki = env_or("FUSEKI_URL", "http://localhost:3030");
    let code = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "30", "-X", "POST",
                      &format!("{fuseki}/pods/update"), "-H", "Content-Type: application/sparql-update",
                      "--data-binary", &update])
        .unwrap_or_default()
        .trim()
        .to_string();
    if code != "200" && code != "204" {
        eprintln!("chorus-principal: FAILED at step 3 (principal) — the store answered HTTP {code} — rolling back");
        led.rollback(&css, &acct, &api, &token, &collection);
        return 1;
    }
    // Written is not readable until read: ask the store back before claiming it.
    if !principals().iter().any(|(n, w)| n == &id && w == &webid) {
        eprintln!("chorus-principal: FAILED at step 3 (principal) — the store accepted the write but does not read back {id} → {webid} — rolling back");
        led.rollback(&css, &acct, &api, &token, &collection);
        return 1;
    }
    if half.is_none() {
        led.principal = Some(id.clone());
    }

    // 3b — ATTACH the role, in the SAME run. Jeff, 2026-09-17: "we attach
    // roles." Provisioning binds a user to a role that already exists; it does
    // not invent one. A role says what someone may do — that is an org decision
    // authored in the roles domain, and a tool that conjures one to satisfy its
    // own check writes a row that means nothing (role-abby-normal landed with a
    // type and a label and no job at all). So: the role must be there, and the
    // edge is written onto the principal row that now exists.
    if let Some(r) = role {
        let r = r.trim_start_matches("chorus:");
        if !role_exists(r) {
            eprintln!("chorus-principal: FAILED at step 3b — no role '{r}' to attach — rolling back");
            eprintln!("  a role is authored in the roles domain and says what its holder may do;");
            eprintln!("  this creates users, not roles. Author '{r}' first, then rerun.");
            led.rollback(&css, &acct, &api, &token, &collection);
            return 1;
        }
        let edge = format!(
            "PREFIX c: <https://jeffbridwell.com/chorus#> \
             INSERT DATA {{ GRAPH <urn:chorus:domains:roles> {{ c:{id} c:holdsRole c:{r} }} }}"
        );
        let rc = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "30", "-X", "POST",
                        &format!("{fuseki}/pods/update"), "-H", "Content-Type: application/sparql-update",
                        "--data-binary", &edge])
            .unwrap_or_default().trim().to_string();
        // Asked back, never assumed: the gate's own question is the check.
        if holds_role(name) != Some(true) {
            eprintln!("chorus-principal: FAILED at step 3b (role) — the store answered HTTP {rc} but '{name}' still holds no role — rolling back");
            led.rollback(&css, &acct, &api, &token, &collection);
            return 1;
        }
        eprintln!("  3/3 {id} attached to {r}");
    }

    // 3c — the four hats, ON THE PRINCIPAL. Jeff 2026-09-17: "principals have
    // hats regardless of appointments." The hat is what the user IS — product
    // manager, solutions architect, engineering lead, operations lead — and it
    // holds whether or not anyone has been put over anything. Appointments are a
    // separate row about scope; they are not how a principal gets a hat, and
    // routing the hat through one made a user's identity depend on an org chart.
    if kind == Kind::Agent {
        let hats = ["product-manager", "solutions-architect", "engineering-lead", "operations-lead"];
        let worn: String = hats.iter()
            .map(|h| format!("c:{id} c:wearsHat c:hat-{h} . "))
            .collect();
        let up = format!(
            "PREFIX c: <https://jeffbridwell.com/chorus#> \
             INSERT DATA {{ GRAPH <urn:chorus:domains:roles> {{ {worn} }} }}"
        );
        let hc = curl(&["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "30", "-X", "POST",
                        &format!("{fuseki}/pods/update"), "-H", "Content-Type: application/sparql-update",
                        "--data-binary", &up]).unwrap_or_default().trim().to_string();
        let got = hats_held(name);
        if got != 4 {
            eprintln!("chorus-principal: FAILED at step 3c (hats) — store answered HTTP {hc} but '{name}' wears {got} of 4 — rolling back");
            led.rollback(&css, &acct, &api, &token, &collection);
            return 1;
        }
        eprintln!("  3/3 {id} wears all four hats");
    }
    eprintln!("  3/3 {id} bound to {webid} ({} — principalKind={}, canSignIn={})", kind.word(), kind.stored(), kind.can_sign_in());

    println!("{webid}");
    let caller = env_or("CHORUS_ROLE", &env_or("DEPLOY_ROLE", "unknown"));
    let _ = Command::new("chorus-log")
        .args(["identity.provisioned", &caller, &format!("user={name}"), &format!("kind={}", kind.word()), &format!("webid={webid}")])
        .status();
    eprintln!("chorus-principal: '{name}' provisioned as {} — {}", kind.word(),
        if kind == Kind::Agent { "pod, credential, principal" } else { "pod, principal (signs in)" });
    let _ = std::fs::remove_file(&jar);
    0
}

// ── the agent's own account ────────────────────────────────────────────────
//
// #4202. An agent's credential used to be written into whoever ran the seeder —
// in practice Jeff's home, holding every role's credential at once. Anything
// running as him could then mint a token as any role, so a write could never
// prove which agent made it. The credential belongs in the account the agent
// runs as, readable by nobody else, and that account is part of provisioning a
// user rather than a separate step somebody remembers to do.

/// The account an agent runs as. One name, formed here, never typed twice.
fn agent_account(name: &str) -> String {
    format!("chorus-{name}")
}

/// That account's home, as the directory service reports it. None = no account.
fn account_home(acct: &str) -> Option<String> {
    let out = Command::new("dscl")
        .args([".", "-read", &format!("/Users/{acct}"), "NFSHomeDirectory"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.split_whitespace().nth(1).map(|h| h.to_string())
}

/// Create the account if it is not there yet. Standard user, no keychain, and a
/// password nobody keeps: nothing signs in to these accounts interactively, work
/// reaches them through `sudo -u`, and a password nobody holds is a password
/// nobody leaks. Keychain commands are deliberately absent — running them under
/// sudo on 2026-09-17 rewrote Jeff's own login session and cost him the machine.
fn ensure_account(acct: &str) -> Result<String, String> {
    if let Some(home) = account_home(acct) {
        return Ok(home);
    }
    if !is_root() {
        return Err(format!(
            "no account '{acct}' and creating one needs root — rerun under sudo"
        ));
    }
    let pw = Command::new("openssl").args(["rand", "-base64", "24"]).output()
        .map_err(|e| format!("cannot generate a password: {e}"))?;
    let pw = String::from_utf8_lossy(&pw.stdout).trim().to_string();
    // "chorus-silas" → "Chorus Silas", without indexing into a string that may
    // be shorter than the prefix.
    let bare = acct.strip_prefix("chorus-").unwrap_or(acct);
    let mut cs = bare.chars();
    let full = match cs.next() {
        Some(first) => format!("Chorus {}{}", first.to_uppercase(), cs.as_str()),
        None => return Err("an account name with nothing after 'chorus-'".to_string()),
    };
    let st = Command::new("sysadminctl")
        .args(["-addUser", acct, "-fullName", &full, "-shell", "/bin/zsh",
               "-home", &format!("/Users/{acct}"), "-password", &pw])
        .output()
        .map_err(|e| format!("sysadminctl did not run: {e}"))?;
    if !st.status.success() {
        return Err(format!(
            "sysadminctl refused to create '{acct}': {}",
            String::from_utf8_lossy(&st.stderr).trim()
        ));
    }
    account_home(acct).ok_or_else(|| format!("created '{acct}' but it has no home directory"))
}

fn is_root() -> bool {
    Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// The credential file's contents. Separate from writing it so the shape can be
/// checked without root — `hostAccount` in particular, which is the field
/// chorus-identity-token refuses on when it names an account it is not running as.
/// A credential written without it is usable by anyone who can read the file.
fn cred_body(name: &str, id: &str, secret: &str, webid: &str, issuer: &str, acct: &str) -> String {
    format!(
        "{{\n  \"agent\": \"{name}\",\n  \"webId\": \"{webid}\",\n  \"issuer\": \"{issuer}/\",\n  \"tokenEndpoint\": \"{issuer}/.oidc/token\",\n  \"hostAccount\": \"{acct}\",\n  \"id\": \"{id}\",\n  \"secret\": \"{secret}\"\n}}\n"
    )
}

/// Write the credential into the agent's own home, owned by it, 0600.
///
/// Returns the path so the ledger can take it back out on a later failure.
fn place_credential(name: &str, id: &str, secret: &str, webid: &str, issuer: &str)
    -> Result<String, String>
{
    let acct = agent_account(name);
    let home = ensure_account(&acct)?;
    if !is_root() {
        return Err(format!(
            "writing inside {home} needs root — that home is the agent's alone, which is the point; rerun under sudo"
        ));
    }
    let dir = format!("{home}/.chorus/identity/{name}");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot make {dir}: {e}"))?;
    let path = format!("{dir}/cred.json");
    let body = cred_body(name, id, secret, webid, issuer, &acct);
    std::fs::write(&path, body).map_err(|e| format!("cannot write {path}: {e}"))?;
    // Owner and mode before anyone else can look: the secret is on disk now.
    let _ = Command::new("chmod").args(["600", &path]).status();
    let _ = Command::new("chown").args(["-R", &format!("{acct}:staff"), &format!("{home}/.chorus")]).status();
    // hostAccount is what chorus-identity-token checks (#4202): a credential
    // carrying an account it is not running as is refused before the mint.
    Ok(path)
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

/// The rdfs:label of a principal row, as the store holds it. None = no row or no label.
fn label_of(id: &str) -> Option<String> {
    let q = format!(
        "PREFIX c: <https://jeffbridwell.com/chorus#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
         SELECT ?l WHERE {{ GRAPH <urn:chorus:domains:security> {{ c:{id} rdfs:label ?l }} }} LIMIT 1"
    );
    let body = curl(&["-s", "--max-time", "20", "-G",
                      &format!("{}/pods/query", env_or("FUSEKI_URL", "http://localhost:3030")),
                      "--data-urlencode", &format!("query={q}"),
                      "-H", "Accept: application/sparql-results+json"])?;
    let flat: String = {
        let mut f = String::with_capacity(body.len());
        let mut in_str = false;
        let mut prev_esc = false;
        for ch in body.chars() {
            if ch == '"' && !prev_esc { in_str = !in_str; }
            prev_esc = ch == '\\' && !prev_esc;
            if !in_str && ch.is_whitespace() { continue; }
            f.push(ch);
        }
        f
    };
    let i = flat.find("\"l\":")?;
    between(&flat[i..], "\"value\":\"", "\"").map(|v| v.replace("\\\"", "\""))
}

fn jar_path() -> String {
    format!("{}/chorus-principal-{}.jar", env_or("TMPDIR", "/tmp").trim_end_matches('/'), std::process::id())
}

/// The password this tool saved for a human it made, if any. Read only to
/// prove a half-made user is OURS; never printed.
fn saved_password(name: &str) -> Option<String> {
    let p = format!("{}/.chorus/identity/{name}/initial-password", env_or("HOME", ""));
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Sign in as a human with email + password and return (account id, the webId
/// the register LINKS to that account). Both come from the register's own
/// answers; nothing is constructed. Leaves the account token set for the
/// calls that follow.
fn css_login_person(css: &str, jar: &str, email: &str, password: &str) -> Option<(String, String)> {
    let esc = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
    let body = format!("{{\"email\":\"{}\",\"password\":\"{}\"}}", esc(email), esc(password));
    let cfg = format!("header = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\ndata = \"{}\"\n", cfg_escape(&body));
    let login = curl_css(Some(cfg), &["-s", "--max-time", "30", "-c", jar, "-X", "POST",
                                     &format!("{css}/.account/login/password/")])?;
    let token = between(&login, "\"authorization\":\"", "\"")?;
    let auth = format!("header = \"Authorization: CSS-Account-Token {}\"\n", cfg_escape(&token));
    let index = curl_css(Some(auth.clone()), &["-s", "--max-time", "20", "-H", "Accept: application/json",
                                                &format!("{css}/.account/")])?;
    let webid_ctl = between(&index, "\"webId\":\"", "\"")?;
    let acct = webid_ctl.split("/account/").nth(1)?.split('/').next()?.to_string();
    let local = webid_ctl.replacen(&env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com").trim_end_matches('/').to_string(), css, 1);
    let links = curl_css(Some(auth), &["-s", "--max-time", "20", "-H", "Accept: application/json", &local])?;
    // {"webIdLinks":{"<webid>":"<link url>", ...}}
    let after = &links[links.find("\"webIdLinks\":{")? + "\"webIdLinks\":{".len()..];
    let webid = between(after, "\"", "\"")?;
    if webid.is_empty() {
        return None;
    }
    ACCOUNT_TOKEN.with(|t| *t.borrow_mut() = token);
    Some((acct, webid))
}

/// Create a human's OWN CSS account: account → password login (email + a
/// generated password) → account id. The password is written once, mode 600,
/// to ~/.chorus/identity/<name>/initial-password for Jeff to hand over; it is
/// never printed and never in argv. The account token rides a curl config on
/// stdin. A refusal at any step deletes the account (the API has DELETE), so a
/// human never half-exists in the register.
fn css_create_account(css: &str, jar: &str, name: &str, p: &Person) -> Result<String, String> {
    let created = curl_css(None, &["-s", "--max-time", "30", "-c", jar, "-X", "POST",
                         &format!("{css}/.account/account/"), "-H", "Content-Type: application/json",
                         "-H", "Accept: application/json", "--data", "{}"])
        .ok_or("could not reach the CSS accounts API")?;
    let token = between(&created, "\"authorization\":\"", "\"")
        .ok_or_else(|| format!("the register did not create an account (no authorization in the reply: {})", created.chars().take(160).collect::<String>()))?;
    let auth = format!("header = \"Authorization: CSS-Account-Token {}\"\n", cfg_escape(&token));
    // The create reply carries ONLY the token. The controls (which name the
    // account id) come from GET /.account/ as that account. The first version
    // looked for a pod control in the create reply, found none, and returned
    // without deleting — one orphan account (no login, no pod) on 2026-09-16
    // 13:34 is the cost of that; it cannot be listed or reached again.
    let index = curl_css(Some(auth.clone()), &["-s", "--max-time", "20", "-H", "Accept: application/json",
                                                &format!("{css}/.account/")]).unwrap_or_default();
    let pod_url = match between(&index, "\"pod\":\"", "\"") {
        Some(u) => u,
        None => {
            // no controls = no id = nothing we can even delete by id; say so
            return Err(format!("account created but its controls could not be read ({}); the register holds an account with no login — nothing else exists",
                index.chars().take(160).collect::<String>()));
        }
    };
    let acct = pod_url.split("/account/").nth(1).and_then(|r| r.split('/').next()).map(String::from)
        .ok_or("account created but its id could not be read")?;

    let password = random_password();
    let esc = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
    let body = format!("{{\"email\":\"{}\",\"password\":\"{}\"}}", esc(&p.email), esc(&password));
    let cfg = format!("{auth}header = \"Content-Type: application/json\"\ndata = \"{}\"\n", cfg_escape(&body));
    let login = curl_css(Some(cfg), &["-s", "-w", "\n%{http_code}", "--max-time", "30", "-X", "POST",
                                     &format!("{css}/.account/account/{acct}/login/password/")])
        .unwrap_or_default();
    let code = login.lines().last().unwrap_or("").trim().to_string();
    if code != "200" && code != "201" {
        let _ = curl_css(Some(auth.clone()), &["-s", "-o", "/dev/null", "-X", "DELETE", "--max-time", "20",
                                              &format!("{css}/.account/account/{acct}/")]);
        let detail = login.lines().next().unwrap_or("").chars().take(200).collect::<String>();
        return Err(format!("the register refused the sign-in for {} (HTTP {code}: {detail}); account removed", p.email));
    }
    let dir = format!("{}/.chorus/identity/{name}", env_or("HOME", ""));
    let _ = std::fs::create_dir_all(&dir);
    let path = format!("{dir}/initial-password");
    if std::fs::write(&path, format!("{password}\n")).is_err() {
        let _ = curl_css(Some(auth), &["-s", "-o", "/dev/null", "-X", "DELETE", "--max-time", "20",
                                      &format!("{css}/.account/account/{acct}/")]);
        return Err("could not write the initial password file; account removed".into());
    }
    let _ = Command::new("chmod").args(["600", &path]).status();
    // the pod + credential steps read the account from the cookie jar the way
    // the service path does; the token is also valid, so both paths converge.
    ACCOUNT_TOKEN.with(|t| *t.borrow_mut() = token);
    Ok(acct)
}

thread_local! {
    static ACCOUNT_TOKEN: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

/// The account token as a curl config line, when a human's own account is in
/// play; None on the service path (the cookie jar carries that session).
fn css_auth_cfg() -> Option<String> {
    let t = ACCOUNT_TOKEN.with(|t| t.borrow().clone());
    if t.is_empty() { None } else { Some(format!("header = \"Authorization: CSS-Account-Token {}\"\n", cfg_escape(&t))) }
}

fn random_password() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 24];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ23456789";
    bytes.iter().map(|b| A[(*b as usize) % A.len()] as char).collect()
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
        &format!("{}/CascadeProjects/jeff-bridwell-personal-site/.env", owner_home()),
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
            "-H", &format!("Host: {}", env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com").split("://").nth(1).unwrap_or("").trim_end_matches('/')),
            "-H", "X-Forwarded-Proto: https",
            "-H", &format!("X-Forwarded-Host: {}", env_or("CSS_ISSUER", "https://id.lightlifeurbangardens.com").split("://").nth(1).unwrap_or("").trim_end_matches('/')),
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

    let acct = curl_css(None, &["-s", "--max-time", "20", "-b", jar, &format!("{css}/.account/")])?;
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

// The door is the bats suite (platform/tests/3830-provision-user.bats): every
// refusal against a stub world. These are the pure parts — what the door
// decides from its arguments — and the run 69 lesson: a crate with no tests
// reads as a red, not as a nothing.
#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn kind_is_required_and_only_two_words_are_kinds() {
        assert_eq!(Kind::from_args(&args(&["--kind", "human"])), Some(Kind::Human));
        assert_eq!(Kind::from_args(&args(&["--kind=agent"])), Some(Kind::Agent));
        // NEGATIVE: no kind, or a third word, is not a kind
        assert_eq!(Kind::from_args(&args(&[])), None);
        assert_eq!(Kind::from_args(&args(&["--kind", "person"])), None);
        assert_eq!(Kind::from_args(&args(&["--kind", "service"])), None);
    }

    #[test]
    fn kind_decides_what_the_row_says_and_whether_they_sign_in() {
        assert_eq!(Kind::Human.stored(), "human");
        assert_eq!(Kind::Human.can_sign_in(), "true");
        assert_eq!(Kind::Agent.stored(), "agent");
        assert_eq!(Kind::Agent.can_sign_in(), "false");
    }

    #[test]
    fn a_human_needs_a_name_and_an_email_that_is_one() {
        let p = Person::from_args(&args(&["--kind", "human", "--name", "Deb Majumdar", "--email", "d@example.org"])).unwrap();
        assert_eq!(p.full_name, "Deb Majumdar");
        // NEGATIVE: half a person is not a person
        assert!(Person::from_args(&args(&["--name", "Deb Majumdar"])).is_none());
        assert!(Person::from_args(&args(&["--email", "d@example.org"])).is_none());
        assert!(Person::from_args(&args(&["--name", " ", "--email", "d@example.org"])).is_none());
        assert!(Person::from_args(&args(&["--name", "Deb", "--email", "not-an-address"])).is_none());
    }

    #[test]
    fn curl_config_escaping_survives_quotes_and_backslashes() {
        // a password with a quote in it must not end the config line early
        assert_eq!(cfg_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn between_reads_the_first_span_and_nothing_when_absent() {
        assert_eq!(between(r#"{"webId":"https://x/y#me","z":1}"#, "\"webId\":\"", "\"").as_deref(), Some("https://x/y#me"));
        assert_eq!(between("nothing here", "\"webId\":\"", "\""), None);
    }

    #[test]
    fn a_generated_password_is_long_and_never_empty() {
        let p = random_password();
        assert_eq!(p.chars().count(), 24);
        assert_ne!(p, random_password());
    }

    // ── #4202: the credential belongs to the agent's own account ────────────

    #[test]
    fn an_agent_account_is_the_name_with_one_prefix() {
        assert_eq!(agent_account("silas"), "chorus-silas");
        assert_eq!(agent_account("wren"), "chorus-wren");
    }

    /// NEGATIVE PROOF. The whole point is a credential only one account can
    /// read; a body without hostAccount is the state this exists to prevent,
    /// because chorus-identity-token has nothing to refuse on.
    #[test]
    fn the_credential_body_names_the_account_that_may_use_it() {
        let b = cred_body("silas", "cid", "csecret", "https://id.example/silas/profile/card#me",
                          "https://id.example", "chorus-silas");
        assert!(b.contains("\"hostAccount\": \"chorus-silas\""), "body must bind the account: {b}");
        assert!(b.contains("\"webId\": \"https://id.example/silas/profile/card#me\""));
        assert!(b.contains("\"tokenEndpoint\": \"https://id.example/.oidc/token\""));
        // The secret is in the file and nowhere else; if this ever stops being
        // true the file is not a credential.
        assert!(b.contains("\"secret\": \"csecret\""));
    }

    /// NEGATIVE PROOF. Without root this cannot write inside another account's
    /// home — and must say so rather than silently landing the credential in
    /// the caller's own home, which is the 2026-09-17 defect exactly.
    #[test]
    fn placing_a_credential_without_root_refuses_and_says_why() {
        if is_root() {
            return; // the refusal under test only exists off-root
        }
        let err = place_credential("silas", "cid", "csecret",
                                   "https://id.example/silas/profile/card#me", "https://id.example")
            .expect_err("must refuse without root");
        assert!(err.contains("root"), "the refusal names what is missing: {err}");
    }

    /// An account nobody has created cannot be conjured without root either.
    #[test]
    fn a_missing_account_off_root_refuses_rather_than_guessing() {
        if is_root() {
            return;
        }
        let err = ensure_account("chorus-nobody-4202").expect_err("must refuse");
        assert!(err.contains("sudo"), "the refusal names the way forward: {err}");
    }

    /// NEGATIVE PROOF — off root the owner is simply whoever is running, and
    /// SUDO_USER must NOT be able to redirect where secrets are read from.
    /// A tool that honoured it unprivileged would read another person's files
    /// on the say-so of an environment variable.
    #[test]
    fn sudo_user_does_not_redirect_config_when_not_root() {
        if is_root() { return; }
        let home = std::env::var("HOME").unwrap_or_default();
        // A REAL account whose home differs from ours. "nobody-4202" would not
        // resolve at all, so the check would pass for the wrong reason — it
        // could not tell "ignored SUDO_USER" from "looked it up and found
        // nothing", which are the two states this separates.
        let other = "chorus-silas";
        if account_home(other).is_none() { return; }
        assert_ne!(account_home(other).unwrap(), home, "the fixture account must have a different home");
        std::env::set_var("SUDO_USER", other);
        assert_eq!(owner_home(), home, "off root, SUDO_USER must not move the config home");
        std::env::remove_var("SUDO_USER");
    }
}

/// Does this role exist to be attached? #4202 — asked of the store, never
/// assumed from the name. An unreadable store answers no, which refuses the
/// create rather than attaching to something that may not be there.
fn role_exists(role: &str) -> bool {
    let q = format!("PREFIX c: <https://jeffbridwell.com/chorus#> ASK {{ GRAPH ?g {{ c:{role} a c:Role }} }}");
    let body = curl(&[
        "-s", "--max-time", "30", "-G",
        &format!("{}/pods/query", env_or("FUSEKI_URL", "http://localhost:3030")),
        "--data-urlencode", &format!("query={q}"),
        "-H", "Accept: application/sparql-results+json",
    ]).unwrap_or_default();
    let flat: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    flat.contains("\"boolean\":true")
}

/// How many of the four hats this principal wears. Counted from the store,
/// never assumed — the check that the appointments landed is the same question
/// anyone else would ask of the graph.
fn hats_held(name: &str) -> usize {
    let q = format!(
        "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?h) AS ?n) WHERE {{ \
         GRAPH ?g {{ c:principal-{name} c:wearsHat ?h }} }}"
    );
    let body = curl(&[
        "-s", "--max-time", "30", "-G",
        &format!("{}/pods/query", env_or("FUSEKI_URL", "http://localhost:3030")),
        "--data-urlencode", &format!("query={q}"),
        "-H", "Accept: application/sparql-results+json",
    ]).unwrap_or_default();
    between(&body, "\"value\":\"", "\"").and_then(|v| v.parse().ok()).unwrap_or(0)
}
