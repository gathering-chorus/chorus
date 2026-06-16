//! cascade — the effective-config resolver (#3437, Properties C).
//!
//! A `chorus:Property` (config-as-data, #3433) attaches to ANY structural node
//! via `chorus:hasProperty` — a Service, SubDomain, Domain, Product,
//! ValueStreamStep, or ValueStream. The *effective* value for a (node, key) is
//! the NEAREST override along that node's containment ancestry: a value on the
//! Service overrides one on its Domain, which overrides one on its Product,
//! which overrides one on the ValueStream. Absent its own value a node inherits
//! its ancestor's. Set nowhere → no-match (the caller falls back to the code
//! default; config-as-data models only the VARYING config, never invariants).
//!
//! This is PURE logic: the specificity LAW lives here (the model's meaning),
//! unit-tested without a live graph — the `project_secured` pattern. Properties A
//! (#3435) does the graph read: it walks containment in Fuseki to gather the
//! bindings on a node's ancestry, tags each with its `Level`, and calls `resolve`
//! to bake in the effective value at projection time. Type COERCION
//! (propertyValueType) is A's job downstream — the resolver carries the declared
//! type through untouched so A coerces ONCE on the winner.

/// The containment specificity ladder — MOST specific first (lowest rank wins).
/// This is the law the cascade enforces; A's traversal order must not get a vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Service,
    SubDomain,
    Domain,
    Product,
    ValueStreamStep,
    ValueStream,
}

impl Level {
    /// Specificity rank — 0 is most specific (Service), 5 least (ValueStream).
    /// The nearest override is the binding with the smallest rank.
    pub fn rank(&self) -> u8 {
        match self {
            Level::Service => 0,
            Level::SubDomain => 1,
            Level::Domain => 2,
            Level::Product => 3,
            Level::ValueStreamStep => 4,
            Level::ValueStream => 5,
        }
    }

    /// Map an OWL class local-name to its containment level. The union domain of
    /// `chorus:hasProperty` (chorus.ttl) is exactly these six classes; anything
    /// else is not a config-bearing node and yields None (the caller drops it).
    pub fn from_class(local: &str) -> Option<Level> {
        match local {
            "Service" => Some(Level::Service),
            "SubDomain" => Some(Level::SubDomain),
            "Domain" => Some(Level::Domain),
            "Product" => Some(Level::Product),
            "ValueStreamStep" => Some(Level::ValueStreamStep),
            "ValueStream" => Some(Level::ValueStream),
            _ => None,
        }
    }
}

/// One config datum gathered from a node on the target's containment ancestry.
/// `level` is the containment level of the node the Property is attached to —
/// that, not list order, decides specificity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub key: String,
    pub value: String,
    /// propertyValueType (string|int|bool|json|list) — carried through, NOT
    /// coerced here; A coerces ONCE on the resolved winner.
    pub value_type: String,
    pub level: Level,
}

impl Binding {
    pub fn new(key: &str, value: &str, value_type: &str, level: Level) -> Binding {
        Binding {
            key: key.to_string(),
            value: value.to_string(),
            value_type: value_type.to_string(),
            level,
        }
    }
}

/// The resolved effective value + the level it was resolved AT (so A and the
/// dashboards can show WHERE a value came from — the override provenance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub value: String,
    pub value_type: String,
    pub level: Level,
}

/// Resolve the effective value for `key`: among the bindings for that key on the
/// node's ancestry, the most specific (lowest `Level::rank`) wins. None = the key
/// is set nowhere on the chain (no-match → fall back to the code default).
///
/// A single valid ancestry has at most one node per level, so a same-level
/// collision is not expected; if one is passed anyway, the FIRST binding seen at
/// the winning rank is taken (`min_by_key` keeps the first of equal keys) —
/// deterministic, never a panic.
pub fn resolve(key: &str, bindings: &[Binding]) -> Option<Effective> {
    bindings
        .iter()
        .filter(|b| b.key == key)
        .min_by_key(|b| b.level.rank())
        .map(|b| Effective {
            value: b.value.clone(),
            value_type: b.value_type.clone(),
            level: b.level,
        })
}
