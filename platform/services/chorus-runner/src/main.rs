//! #4512 — chorus-runner: the Services tool runner core, beside the old
//! platform/mcp-server (:3341). Binds 127.0.0.1:3342 (CHORUS_RUNNER_PORT).

use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chorus_oidc::oidc::{self, OidcVerifier};
use chorus_runner::server::{now_secs, refresh_collections, router};

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

#[tokio::main]
async fn main() {
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

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
    eprintln!("chorus-runner: listening on 127.0.0.1:{port}/mcp");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
