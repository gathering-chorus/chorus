//! chorus-go — #4710. `serve` runs the go page behind the tunnel
//! (lightlifeurbangardens.com/go); `register-link` makes the one-time link that
//! registers Jeff's phone; `check <card> <round>` is what the land asks; `check-step <dag> <run> <step>`
//! is what a step after a Jeff-only human step asks (#4885).
use chorus_go::serve::{env_or, handle, origin};
use chorus_go::*;
use std::net::TcpListener;

#[allow(dead_code)]
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}

/// launchd's label; the prod name when run by hand.
fn service_label() -> String {
    service_lifecycle::launchd_label().unwrap_or_else(|| "com.chorus.go".to_string())
}

/// #4446 — every service logs its lifecycle; #4710 — over the spine's HTTP route (spine_post),
/// since the lander runs no chorus-log or bash.
fn emit_lifecycle((event, fields): service_lifecycle::LifecycleEvent) {
    let mut f: Vec<(&str, String)> = fields.iter().map(|(k, v)| (*k, v.to_string())).collect();
    if event == "service.failed" { f.push(("level", "error".into())); }
    spine_post(event, &f);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // #4710 slice 2 — the lander's store: challenges in ~/.chorus/go (0700), Jeff's public key in
    // the lander's public dir (found by account, not env). `check` runs as jeffbridwell in the land
    // and reads the same public dir.
    let public = Store::lander_public().unwrap_or_else(|| Store::default_dir().join("public"));
    let store = match Store::open_with(&Store::default_dir(), &public) {
        Ok(s) => s,
        Err(e) => { eprintln!("chorus-go: {e}"); std::process::exit(1) }
    };
    let code = match args.first().map(String::as_str) {
        Some("serve") => { serve(&store); 0 }
        Some("register-link") => match store.mint_register_token() {
            Ok(t) => { println!("{}/go/register?token={t}", origin()); 0 }
            Err(e) => { eprintln!("chorus-go: {e}"); 1 }
        },
        Some("check") => {
            let card = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let round = args.get(2).cloned().unwrap_or_default();
            match chorus_go::serve::land_go(&store, card, &round) {
                Ok(LandGo::Signed) => { println!("chorus-go: #{card} round {} — go signed by Jeff's passkey", short(&round)); 0 }
                // #4710 slice 2 — fail closed: no readable passkey on the lander is no go
                Ok(LandGo::NoPasskey) => { eprintln!("chorus-go: refused: no passkey registered on the lander — the go is not signed (#4710)"); 1 }
                Err(e) => { eprintln!("chorus-go: refused: {e}"); 1 }
            }
        }
        Some("check-step") => {
            let (dag, run, step) = (args.get(1).cloned().unwrap_or_default(), args.get(2).cloned().unwrap_or_default(), args.get(3).cloned().unwrap_or_default());
            match chorus_go::serve::land_step(&store, &dag, &run, &step) {
                Ok(()) => { println!("chorus-go: {dag} run {run} step {step} — signed by Jeff's passkey"); 0 }
                Err(e) => { eprintln!("chorus-go: refused: {e}"); 1 }
            }
        }
        _ => { eprintln!("usage: chorus-go serve | register-link | check <card> <round> | check-step <dag> <run> <step>"); 2 }
    };
    std::process::exit(code);
}

fn serve(store: &Store) {
    let addr = env_or("CHORUS_GO_ADDR", "127.0.0.1:3490");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        let why = format!("bind {addr}: {e}");
        eprintln!("chorus-go: {why}");
        emit_lifecycle(service_lifecycle::failed_event(&service_label(), std::process::id(), &why, 1));
        std::process::exit(1)
    });
    let label = service_label();
    let previous = service_lifecycle::previous_run(&label);
    for e in service_lifecycle::start_events(&label, std::process::id(), &service_lifecycle::binary_version(), previous.as_ref()) {
        emit_lifecycle(e);
    }
    eprintln!("chorus-go: serving {addr} for {}", origin());
    // one thread per connection: a phone waiting on Face ID never holds up the next request
    std::thread::scope(|scope| {
        for stream in listener.incoming().flatten() {
            scope.spawn(move || if let Err(e) = handle(store, stream) { eprintln!("chorus-go: {e}"); });
        }
    });
}
