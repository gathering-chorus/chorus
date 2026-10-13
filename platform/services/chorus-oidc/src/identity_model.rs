//! #4518 — WHOSE identity model this verifier reads.
//!
//! Until today every query here named Chorus: the `https://jeffbridwell.com/chorus#`
//! vocabulary and the `urn:chorus:` graphs, 40 references in all. A second app
//! (Art's wine diary) could only verify its people by reading Chorus's security
//! graphs, which puts his accounts beside the roles' accounts. Jeff, 2026-10-13:
//! "conceptually we just spin up a new oidc for an app" — so each app has its own
//! sign-in server and its own store, and this ONE verifier reads that app's model.
//!
//! The queries stay written once, in the Chorus vocabulary. `localize` maps the
//! `chorus:` prefix and the graph prefix onto the product's, so a product declares
//! the same identity terms (Principal, webId, holdsRole, Permission, agent,
//! accessTo, mode, oidcClientId) in its own namespace. Modelling terms
//! (instancesGraph on a shape) stay Chorus's. The keys
//! match what a product row declares for athena-gen (#4519): `model.namespace`
//! and `store.graph_prefix`.
//!
//! Set once per process, before the first token: by `configure` (a generated app
//! calls it at boot) or by env (`CHORUS_OIDC_MODEL_NAMESPACE`,
//! `CHORUS_OIDC_STORE_GRAPH_PREFIX`). Neither set = Chorus, so every existing
//! door reads exactly what it read before.

use std::sync::OnceLock;

pub const CHORUS_NAMESPACE: &str = "https://jeffbridwell.com/chorus#";
pub const CHORUS_GRAPH_PREFIX: &str = "urn:chorus:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityModel {
    pub namespace: String,
    pub graph_prefix: String,
}

impl IdentityModel {
    pub fn chorus() -> Self {
        IdentityModel { namespace: CHORUS_NAMESPACE.into(), graph_prefix: CHORUS_GRAPH_PREFIX.into() }
    }

    pub fn new(namespace: &str, graph_prefix: &str) -> Self {
        IdentityModel { namespace: namespace.into(), graph_prefix: graph_prefix.into() }
    }

    pub fn is_chorus(&self) -> bool {
        self.namespace == CHORUS_NAMESPACE && self.graph_prefix == CHORUS_GRAPH_PREFIX
    }

    /// A query written in the Chorus vocabulary, rewritten for this product.
    pub fn localize(&self, q: &str) -> String {
        if self.is_chorus() {
            return q.to_string();
        }
        // Only the `chorus:` PREFIX declaration moves: product terms (Principal,
        // webId, Permission…) are written `chorus:X` and land in the product's
        // namespace. Chorus's modelling terms (instancesGraph on a shape) are
        // build-time language every product shares (Wren + Silas, 2026-10-13),
        // so queries spell those as full IRIs, which this leaves alone.
        q.replace(&format!("PREFIX chorus: <{CHORUS_NAMESPACE}>"), &format!("PREFIX chorus: <{}>", self.namespace))
            .replace(CHORUS_GRAPH_PREFIX, &self.graph_prefix)
    }

    /// A graph under this product's prefix, e.g. `graph("domains:security")`.
    pub fn graph(&self, local: &str) -> String {
        format!("{}{}", self.graph_prefix, local)
    }

    fn from_env() -> Self {
        let ns = std::env::var("CHORUS_OIDC_MODEL_NAMESPACE").ok().filter(|s| !s.is_empty());
        let gp = std::env::var("CHORUS_OIDC_STORE_GRAPH_PREFIX").ok().filter(|s| !s.is_empty());
        IdentityModel {
            namespace: ns.unwrap_or_else(|| CHORUS_NAMESPACE.into()),
            graph_prefix: gp.unwrap_or_else(|| CHORUS_GRAPH_PREFIX.into()),
        }
    }
}

static MODEL: OnceLock<IdentityModel> = OnceLock::new();

/// Set this process's identity model. Once only: a verifier that changes whose
/// people it trusts mid-run is the failure this exists to prevent. Setting the
/// same model again is fine; a different one is refused and said.
pub fn configure(model: IdentityModel) -> Result<(), String> {
    let set = MODEL.get_or_init(|| model.clone());
    if *set == model {
        Ok(())
    } else {
        Err(format!(
            "chorus-oidc: identity model already set to {} / {}; refusing {} / {}",
            set.namespace, set.graph_prefix, model.namespace, model.graph_prefix
        ))
    }
}

/// The model every resolver in this process reads.
pub fn identity_model() -> &'static IdentityModel {
    MODEL.get_or_init(IdentityModel::from_env)
}
