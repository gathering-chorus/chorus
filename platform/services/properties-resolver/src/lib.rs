//! properties-resolver — the PURE cascade precedence core (#3437, Properties C).
//!
//! Config-as-data resolution: given a TOTALLY-ORDERED scope chain and a key,
//! decide the effective value. The nearest (most-specific) scope that SETS the
//! key wins; scopes that don't set it are inherited through. This is the
//! "effective config" the projection (Properties A) bakes in.
//!
//! Deliberately hermetic — no Fuseki, no I/O, no graph. The graph-walk that
//! BUILDS the chain (ownership edges from #3433: a node's single-parent
//! Service > Domain > Product > ValueStream/Step) and the owl-api projection
//! (`/properties/effective?node=X&key=Y`) wire on top of this core. Keeping the
//! precedence logic pure makes it unit-testable in isolation and linkable by any
//! consumer (owl-api, werk-test, tagging-lift) without a live graph.
//!
//! Contract pinned with Kade (navigator, 2026-06-16):
//!  - total-ordering is a PRECONDITION — a malformed chain is an error, never a
//!    silent pick (the model question lives in the chain-builder, not here);
//!  - provenance: the winning Property IRI + value_type ride in the Resolution
//!    (audit "why did this resolve?" + coercion without a re-fetch);
//!  - explicit-empty (present, value == "") is an OVERRIDE that wins and stops;
//!    only an ABSENT datum falls through.

/// The structural scope kinds a property can attach to, in cascade order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Service,
    Domain,
    Product,
    ValueStreamStep,
    ValueStream,
}

impl ScopeKind {
    /// Specificity rank — higher wins. Service is most specific; ValueStream is
    /// the broadest default. Encodes Service > Domain > Product >
    /// ValueStreamStep > ValueStream.
    pub fn rank(self) -> u8 {
        match self {
            ScopeKind::Service => 5,
            ScopeKind::Domain => 4,
            ScopeKind::Product => 3,
            ScopeKind::ValueStreamStep => 2,
            ScopeKind::ValueStream => 1,
        }
    }
}

/// One config datum attached to a scope: a `chorus:Property` individual
/// (`iri`) carrying `propertyKey` / `propertyValue` / `propertyValueType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDatum {
    pub iri: String,
    pub key: String,
    pub value: String,
    pub value_type: String,
}

/// One node in the scope chain: a structural node and the properties it sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNode {
    pub kind: ScopeKind,
    pub iri: String,
    pub properties: Vec<PropertyDatum>,
}

/// The resolved effective value, with provenance for audit + coercion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub value: String,
    pub value_type: String,
    pub winning_scope_iri: String,
    pub winning_scope_kind: ScopeKind,
    pub winning_property_iri: String,
}

/// The chain violated the total-ordering precondition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CascadeError {
    MalformedChain(String),
}

/// Resolve the effective value for `key` over a totally-ordered `scope_chain`
/// (most-specific first).
///
/// - `Err(MalformedChain)` if the chain is not strictly descending by
///   specificity (duplicate or increasing rank) — the core refuses to guess.
/// - `Ok(Some(Resolution))` for the nearest scope that SETS `key` (present —
///   value may be empty, an explicit override).
/// - `Ok(None)` if no scope in the chain sets `key`.
pub fn decide_effective_value(
    scope_chain: &[ScopeNode],
    key: &str,
) -> Result<Option<Resolution>, CascadeError> {
    // PRECONDITION: strictly descending specificity, no ties. Skipping a level
    // (e.g. service then product, no domain) is allowed; equal or increasing
    // rank is not — that means the chain-builder handed us ambiguity.
    for w in scope_chain.windows(2) {
        let (a, b) = (w[0].kind.rank(), w[1].kind.rank());
        if a <= b {
            return Err(CascadeError::MalformedChain(format!(
                "chain not strictly descending by specificity: {:?}(rank {}) then {:?}(rank {})",
                w[0].kind, a, w[1].kind, b
            )));
        }
    }

    // Nearest-wins: the first scope with a datum PRESENT for `key` wins.
    // Match on presence-of-datum (find by key), never on non-empty value — so an
    // explicit value == "" overrides instead of silently inheriting the parent.
    for node in scope_chain {
        if let Some(d) = node.properties.iter().find(|p| p.key == key) {
            return Ok(Some(Resolution {
                value: d.value.clone(),
                value_type: d.value_type.clone(),
                winning_scope_iri: node.iri.clone(),
                winning_scope_kind: node.kind,
                winning_property_iri: d.iri.clone(),
            }));
        }
    }
    Ok(None)
}
