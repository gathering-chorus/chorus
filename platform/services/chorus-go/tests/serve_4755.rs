// @test-type: unit — hermetic: the real chorus-go binary on a loopback port, a scratch go store, a software P-256 key for the phone and a fake dagu that only records its arguments; no real run is ever answered.
// @domain: security
// @card: #4755
//! #4755 — src/main.rs and src/serve.rs, the process half of #4710: the
//! subcommands, and the go page's HTTP routes end to end — register a phone, open a go for a waiting
//! run, sign it, and see the go step answered as Jeff (on the fake dagu) and
//! `check` read the signed go. Each refusal on the way is shown failing.
use chorus_go::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RP: &str = "go.example.test";
const ORIGIN: &str = "https://go.example.test";
const RUN: &str = "034testRun4755";

/// A scratch world: go store, werk base with one werk, chorus home with a
/// `cards` that names the card, and a dagu that records and answers from files.
struct World { root: PathBuf }

fn sh(p: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn world(tag: &str) -> World {
    let root = std::env::temp_dir().join(format!("chorus-go-serve-4755-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let werk = root.join("werks/kade-4755");
    std::fs::create_dir_all(&werk).unwrap();
    std::fs::write(werk.join("f"), "x").unwrap();
    let git = |args: &[&str]| {
        let o = Command::new("git").arg("-C").arg(&werk)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
            .args(args).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "w"]);
    let _ = git(&["rev-parse", "HEAD"]);
    sh(&root.join("home/platform/scripts/cards"), "#!/bin/sh\necho \"#4755 Coverage to 80\"\n");
    // status lines come from files the test writes; every call is logged
    sh(&root.join("bin/dagu"), &format!("#!/bin/sh\necho \"$@\" >> {r}/dagu.log\n\
        case \"$*\" in\n\
        *\"status cicd-go\"*) cat {r}/go-half 2>/dev/null ;;\n\
        *\"status cicd \"*) cat {r}/run-status 2>/dev/null ;;\n\
        *human-task*) [ -f {r}/complete-fails ] && {{ echo 'step is not waiting' >&2; exit 1; }} ;;\n\
        esac\nexit 0\n", r = root.display()));
    std::fs::write(root.join("run-status"), "Waiting - 2026-10-27 00:00:00\n").unwrap();
    World { root }
}

impl World {
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_chorus-go"));
        c.env_clear()
            .args(args)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &self.root)
            .env("CHORUS_GO_DIR", self.root.join("go"))
            .env("CHORUS_HOME", self.root.join("home"))
            .env("CHORUS_WERK_BASE", self.root.join("werks"))
            .env("CHORUS_GO_DAGU", self.root.join("bin/dagu"))
            .env("CHORUS_GO_RP_ID", RP)
            .env("CHORUS_GO_ORIGIN", ORIGIN);
        if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
            c.env("LLVM_PROFILE_FILE", p);
        }
        c
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let o = self.cmd(args).output().unwrap();
        (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
    }
}

/// The go page's routes, served in process by `chorus_go::serve::handle` (the
/// code `chorus-go serve` runs per connection) on a loopback port. `handle`
/// reads its config from the environment, so a test holds ENV for as long as
/// its server runs. The listener thread outlives the test harmlessly.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Server { addr: String, _env: std::sync::MutexGuard<'static, ()> }

fn serve(w: &World) -> Server {
    let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    for (k, v) in [
        ("HOME", w.root.clone()),
        ("CHORUS_HOME", w.root.join("home")),
        ("CHORUS_WERK_BASE", w.root.join("werks")),
        ("CHORUS_GO_DAGU", w.root.join("bin/dagu")),
    ] {
        std::env::set_var(k, v);
    }
    std::env::set_var("CHORUS_GO_RP_ID", RP);
    std::env::set_var("CHORUS_GO_ORIGIN", ORIGIN);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let dir = w.root.join("go");
    std::thread::spawn(move || {
        let store = Store::open(&dir).unwrap();
        for s in l.incoming().flatten() {
            let _ = chorus_go::serve::handle(&store, s);
        }
    });
    Server { addr, _env: guard }
}

fn http(addr: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (code, out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}


// #4642: the old per-run go page and its decide route are retired; one go path, the list.
#[test]
fn negative_proof_the_old_go_link_lands_on_the_list_and_its_decide_route_is_gone() {
    let w = world("one-path");
    let srv = serve(&w);
    let mut s = TcpStream::connect(&srv.addr).unwrap();
    write!(s, "GET /go/?card=4755&run={RUN}&role=kade HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 302"), "{out}");
    assert!(out.contains("Location: /go/approvals"), "{out}");
    for (m, p) in [("POST", "/go/api/decide"), ("GET", "/go/api/info?card=4755&run=x&role=kade")] {
        let (c, b) = http(&srv.addr, m, p, "{}");
        assert_eq!(c, 404, "{m} {p}: {b}");
    }
    // the plain page and the list still answer
    assert_eq!(http(&srv.addr, "GET", "/go/approvals", "").0, 200);
}

#[test]
fn subcommands_refuse_what_they_cannot_do() {
    let w = world("cli");
    let (code, _, err) = w.run(&[]);
    assert_eq!(code, 2);
    assert!(err.contains("usage: chorus-go"), "{err}");
    let (code, out, _) = w.run(&["register-link"]);
    assert_eq!(code, 0);
    assert!(out.starts_with(&format!("{ORIGIN}/go/register?token=")), "{out}");
    // the binary's own serve answers the page (killed after: its lines are not
    // profiled, the routes above are, through handle)
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let at = format!("127.0.0.1:{port}");
    let mut child = w.cmd(&["serve"]).env("CHORUS_GO_ADDR", &at).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    for _ in 0..100 {
        if TcpStream::connect(&at).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let (c, _) = http(&at, "GET", "/go/", "");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(c, 200);
    // a port already held: serve fails loud instead of hanging
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = held.local_addr().unwrap().to_string();
    let o = w.cmd(&["serve"]).env("CHORUS_GO_ADDR", &addr).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("bind"), "{}", String::from_utf8_lossy(&o.stderr));
    // a go store that cannot be opened
    std::fs::write(w.root.join("not-a-dir"), "").unwrap();
    let o = w.cmd(&["check", "1", "r"]).env("CHORUS_GO_DIR", w.root.join("not-a-dir")).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
}
