//! #4512 — chorus-runner: the Services tool runner core. Since #4515 it is
//! the only MCP server (the old TypeScript one is retired). Binds
//! 127.0.0.1:3342 (CHORUS_RUNNER_PORT).

use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chorus_oidc::oidc::{self, OidcVerifier};
use chorus_runner::server::{now_secs, refresh_collections, router};

#[allow(dead_code)]
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}
mod service_stop {
    include!("../../shared/service_stop.rs");
}

fn service_label() -> String {
    service_lifecycle::launchd_label().unwrap_or_else(|| "com.chorus.runner".to_string())
}

/// #4512 reopen 1 (#4446) — start, stop and failure go to the spine through chorus-log.
fn emit_lifecycle((event, fields): service_lifecycle::LifecycleEvent) {
    let home = std::env::var("CHORUS_HOME")
        .unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let mut args = vec![format!("{home}/platform/scripts/chorus-log"), event.to_string(), "system".to_string()];
    args.extend(fields.into_iter().map(|(k, v)| format!("{k}={v}")));
    if event == "service.failed" {
        args.push("--level=error".to_string());
    }
    let _ = Command::new("bash").args(&args).output();
}

fn build_verifier() -> OidcVerifier {
    let css_issuer = std::env::var("CSS_ISSUER").unwrap_or_else(|_| "http://localhost:3001/".to_string());
    let jwks_url = format!("{}/.oidc/jwks", css_issuer.trim_end_matches('/'));
    oidc::prime_allow_set_graph(|q| athena_make::sparql_json(q).ok());
    OidcVerifier::new(
        &css_issuer,
        || oidc::resolve_principal_webids(|q| athena_make::sparql_json(q).ok()),
        || oidc::resolve_principal_roles(|q| athena_make::sparql_json(q).ok()),
        || oidc::resolve_principal_scopes(|q| athena_make::sparql_json(q).ok()),
        move || {
            let out = Command::new("curl").args(["-sf", "--max-time", "3", &jwks_url]).output().ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        },
    )
    .with_principal_names(|| oidc::resolve_principal_names(|q| athena_make::sparql_json(q).ok()))
}

fn main() {
    // #4514 AC4 — `chorus-runner call <tool> [key=value…]`: a client call, not a
    // service start, so it never logs a lifecycle event
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("call") {
        let Some(tool) = a.get(2) else {
            eprintln!("usage: chorus-runner call <tool> [key=value …]   (signs as $ROLE)");
            std::process::exit(2);
        };
        let code = match chorus_runner::client::args_from_pairs(&a[3..]) {
            Ok(args) => chorus_runner::client::run(tool, &args),
            Err(e) => {
                eprintln!("chorus-runner call: {e}");
                2
            }
        };
        std::process::exit(code);
    }
    // #4515 — `chorus-runner stdio`: the same tools over stdin/stdout, for a
    // client that cannot renew an HTTP credential (Abby's Gemini: its headers
    // are fixed at launch and a credential lives 10 minutes). A client process,
    // so no lifecycle events; stdout is the protocol, every log goes to stderr.
    if a.get(1).map(String::as_str) == Some("stdio") {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
        if let Err(reason) = runtime.block_on(serve_stdio()) {
            eprintln!("chorus-runner stdio: {reason}");
            std::process::exit(1);
        }
        return;
    }
    // Before the runtime starts any thread: they inherit the blocked stop signals.
    let label = service_label();
    let pid = std::process::id();
    let stop_label = label.clone();
    service_stop::on_stop(move |sig| emit_lifecycle(service_lifecycle::stop_event(&stop_label, pid, sig)));
    let previous = service_lifecycle::launchd_label().and_then(|l| service_lifecycle::previous_run(&l));
    for e in service_lifecycle::start_events(&label, pid, &service_lifecycle::binary_version(), previous.as_ref()) {
        emit_lifecycle(e);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    if let Err(reason) = runtime.block_on(serve()) {
        eprintln!("chorus-runner: {reason}");
        emit_lifecycle(service_lifecycle::failed_event(&label, pid, &reason, 1));
        std::process::exit(1);
    }
}

/// #4515 — who a stdio runner serves: the role named in ROLE/CHORUS_ROLE,
/// proven by minting its credential as this process's own account (#4202:
/// a credential is usable only by the account it is bound to) and verifying
/// it with the gate's own verifier. Refuses to start otherwise.
async fn serve_stdio() -> Result<(), String> {
    use rmcp::ServiceExt;
    let role = std::env::var("ROLE").or_else(|_| std::env::var("CHORUS_ROLE")).unwrap_or_default();
    if role.is_empty() {
        return Err("no ROLE to serve as; nothing runs as nobody".into());
    }
    let r = role.clone();
    let token = tokio::task::spawn_blocking(move || chorus_runner::server::identity_token(&r)).await.ok().flatten()
        .ok_or_else(|| format!("could not mint {role}'s credential as this account"))?;
    let verifier = Arc::new(tokio::task::spawn_blocking(build_verifier).await.expect("verifier"));
    let v = verifier.clone();
    let caller = tokio::task::spawn_blocking(move || chorus_runner::authenticate(Some(&format!("Bearer {token}")), &v, now_secs()))
        .await
        .map_err(|e| format!("verifier failed: {e}"))?
        .map_err(|r| format!("{role}'s credential did not verify: {}", r.reason))?;
    if chorus_runner::skills::bare(&caller.0) != chorus_runner::skills::bare(&role) {
        return Err(format!("the credential verified as {}, not {role}", caller.0));
    }
    let collections = Arc::new(RwLock::new(Vec::new()));
    {
        let c = collections.clone();
        tokio::task::spawn_blocking(move || refresh_collections(&c)).await.ok();
    }
    let runner = chorus_runner::server::Runner { collections, skills: Arc::new(RwLock::new(Vec::new())), stdio_caller: Some(caller.0.clone()) };
    eprintln!("chorus-runner stdio: serving as {}", caller.0);
    let service = runner.serve(rmcp::transport::stdio()).await.map_err(|e| format!("stdio: {e}"))?;
    service.waiting().await.map_err(|e| format!("stdio: {e}"))?;
    Ok(())
}

async fn serve() -> Result<(), String> {
    let port: u16 = std::env::var("CHORUS_RUNNER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3342);
    let verifier = Arc::new(tokio::task::spawn_blocking(build_verifier).await.expect("verifier"));
    let warmed = {
        let v = verifier.clone();
        tokio::task::spawn_blocking(move || v.warm_allow(now_secs())).await.unwrap_or(0)
    };
    eprintln!("chorus-runner: {}", oidc::graph_provenance(warmed, "boot warm, model-resolved, TTL'd"));

    let collections = Arc::new(RwLock::new(Vec::new()));
    {
        let c = collections.clone();
        tokio::task::spawn_blocking(move || refresh_collections(&c)).await.ok();
    }
    {
        let c = collections.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                let cc = c.clone();
                tokio::task::spawn_blocking(move || refresh_collections(&cc)).await.ok();
            }
        });
    }

    let app = router(verifier, collections);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
    eprintln!("chorus-runner: listening on 127.0.0.1:{port}/mcp");
    axum::serve(listener, app).await.map_err(|e| format!("serve: {e}"))
}
