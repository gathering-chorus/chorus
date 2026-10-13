//! #4512 — chorus-runner: the Services tool runner core, beside the old
//! platform/mcp-server (:3341). Binds 127.0.0.1:3342 (CHORUS_RUNNER_PORT).

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
