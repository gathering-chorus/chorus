// @domain: persistence — the stores crawl: every place Chorus keeps data, with its one role (#4422)
//! #4422 — the stores crawl. Pure decisions only: which discovered things are
//! which mapped stores, what each row says, and what is a finding. main does
//! the I/O (Fuseki's dataset and graph lists, a path and port walk on each
//! machine over ssh like `infra`, the door).
//!
//! A role is a fact about data flow and cannot be observed, so it comes from
//! the authored map (platform/config/store-map.json). A discovered store the map
//! does not name is a finding, never a guessed role.

use crate::infra::{disk_name, Row};
use crate::services::{parse_json, Json};

/// How a map entry recognises its store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    Path(String),
    /// every path on the machine under a prefix, a row each (the video archive drives)
    PathPrefix(String),
    Port(u16),
    /// a Fuseki dataset by exact name, or every dataset under a prefix
    Dataset { exact: Option<String>, prefixes: Vec<String>, per_match: bool },
    /// a named graph by exact name, or every graph under a prefix
    Graph { exact: Option<String>, prefixes: Vec<String>, per_match: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub label: String,
    pub machine: String,
    pub store_type: String,
    pub matcher: Match,
    pub role: String,
    pub source: Option<String>,
    pub in_store: Option<String>,
    pub served_by: Option<String>,
    /// where the disk is read from for a port or dataset store
    pub disk_path: Option<String>,
    /// live (still written) or closed (filled on purpose); default live
    pub write_mode: String,
    /// how the role is known: measured, inventory or inferred — required
    pub basis: String,
}

#[derive(Debug)]
pub struct StoreMap {
    pub entries: Vec<Entry>,
    pub report_graphs: Vec<String>,
    pub report_paths: Vec<String>,
}

pub const ROLES: &[&str] = &["systemOfRecord", "copyOf", "derivedFrom", "cacheOf"];
pub const BASES: &[&str] = &["measured", "inventory", "inferred"];

fn strs(j: Option<&Json>) -> Vec<String> {
    match j {
        Some(Json::Arr(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(Json::Str(s)) => vec![s.clone()],
        _ => vec![],
    }
}

/// The map file → entries, refusing anything the shape would refuse later:
/// an unknown role, a copy with no source, a source or parent that names no entry.
pub fn parse_map(text: &str) -> Result<StoreMap, String> {
    let j = parse_json(text)?;
    let Some(Json::Arr(stores)) = j.get("stores") else { return Err("map has no stores list".into()) };
    let s = |o: &Json, k: &str| o.get(k).and_then(Json::as_str).map(String::from);
    let mut entries = Vec::new();
    for o in stores {
        let name = s(o, "name").ok_or("a store has no name")?;
        let m = o.get("match").ok_or(format!("{name}: no match"))?;
        let kind = s(m, "kind").unwrap_or_default();
        let per_match = matches!(m.get("perMatch"), Some(Json::Bool(true)));
        let matcher = match kind.as_str() {
            "path" => Match::Path(s(m, "value").ok_or(format!("{name}: path match has no value"))?),
            "path-prefix" => Match::PathPrefix(s(m, "prefix").ok_or(format!("{name}: path-prefix match has no prefix"))?),
            "port" => match m.get("value") {
                Some(Json::Num(n)) => Match::Port(n.parse().map_err(|_| format!("{name}: bad port"))?),
                _ => return Err(format!("{name}: port match has no number")),
            },
            "fuseki-dataset" => Match::Dataset { exact: s(m, "value"), prefixes: strs(m.get("prefix")), per_match },
            "graph" => Match::Graph { exact: s(m, "value"), prefixes: strs(m.get("prefix")), per_match },
            k => return Err(format!("{name}: unknown match kind {k:?}")),
        };
        let role = s(o, "role").unwrap_or_default();
        if !ROLES.contains(&role.as_str()) {
            return Err(format!("{name}: role {role:?} is not one of {}", ROLES.join(", ")));
        }
        let source = s(o, "source");
        if (role == "systemOfRecord") == source.is_some() {
            return Err(format!("{name}: a {role} {} a source", if source.is_some() { "cannot name" } else { "must name" }));
        }
        let write_mode = s(o, "writeMode").unwrap_or_else(|| "live".into());
        if write_mode != "live" && write_mode != "closed" {
            return Err(format!("{name}: writeMode {write_mode:?} is not live or closed"));
        }
        let basis = s(o, "basis").unwrap_or_default();
        if !BASES.contains(&basis.as_str()) {
            return Err(format!("{name}: basis {basis:?} is not one of {} — say how the role is known", BASES.join(", ")));
        }
        let in_store = s(o, "inStore");
        let machine = s(o, "machine").unwrap_or_default();
        if in_store.is_none() && machine.is_empty() {
            return Err(format!("{name}: needs a machine (its disk) or an inStore (its dataset)"));
        }
        entries.push(Entry {
            label: s(o, "label").unwrap_or_else(|| name.clone()),
            store_type: s(o, "storeType").ok_or(format!("{name}: no storeType"))?,
            served_by: s(o, "servedBy"),
            disk_path: s(o, "diskPath"),
            name, machine, matcher, role, source, in_store, write_mode, basis,
        });
    }
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    for e in &entries {
        for (what, r) in [("source", &e.source), ("inStore", &e.in_store)] {
            if let Some(r) = r {
                if !names.contains(&r.as_str()) {
                    return Err(format!("{}: {what} {r:?} names no store in the map", e.name));
                }
            }
        }
    }
    let rep = j.get("report");
    Ok(StoreMap {
        entries,
        report_graphs: strs(rep.and_then(|r| r.get("graphs"))),
        report_paths: strs(rep.and_then(|r| r.get("paths"))),
    })
}

/// What one machine showed: which mapped paths exist, which ports listen, and
/// the mount each path sits on. Err = the machine could not be walked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineSeen {
    pub paths: Vec<String>,
    pub ports: Vec<u16>,
    /// (path, mount point) from df
    pub mounts: Vec<(String, String)>,
    /// files found in the scanned directories, for the unmapped check
    pub scanned: Vec<String>,
    /// the port listing could not run: a port store reads unmeasured, never absent
    pub ports_unmeasured: bool,
}

pub struct Seen {
    pub machines: Vec<(String, Result<MachineSeen, String>)>,
    /// None = Fuseki did not answer: dataset and graph rows read unmeasured
    pub datasets: Option<Vec<String>>,
    pub graphs: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    Unmapped { what: String },
    Unmeasured { store: String, reason: String },
}

pub struct Desired {
    pub rows: Vec<Row>,
    pub findings: Vec<Finding>,
    /// residue and strays: named every run, never rows
    pub reported: Vec<String>,
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn prefixed<'a>(item: &str, prefixes: &'a [String]) -> Option<&'a String> {
    prefixes.iter().find(|p| item.starts_with(p.as_str()))
}

/// The one row a map entry yields, before its status and parent are known.
fn row(e: &Entry, name: String, namespace: &str, count: Option<usize>) -> Row {
    let mut f = vec![
        ("label".to_string(), if name == e.name { e.label.clone() } else { format!("{} — {namespace}", e.label) }),
        ("storeType".to_string(), e.store_type.clone()),
        ("storeNamespace".to_string(), namespace.to_string()),
        ("storeRole".to_string(), e.role.clone()),
        ("writeMode".to_string(), e.write_mode.clone()),
        ("roleBasis".to_string(), e.basis.clone()),
    ];
    if let Some(s) = &e.source {
        f.push(("roleSource".into(), s.clone()));
    }
    if let Some(s) = &e.in_store {
        f.push(("inStore".into(), s.clone()));
    }
    if let Some(s) = &e.served_by {
        f.push(("storeServedBy".into(), s.clone()));
    }
    if let Some(n) = count {
        f.push(("storeRecordCount".into(), n.to_string()));
    }
    Row { kind: "PersistenceStore", name, fields: f }
}

fn set(r: &mut Row, k: &str, v: String) {
    r.fields.retain(|(f, _)| f != k);
    r.fields.push((k.to_string(), v));
}

/// The mount a path sits on: the longest mount df reported that the path starts with.
fn mount_of<'a>(seen: &'a MachineSeen, path: &str) -> Option<&'a str> {
    seen.mounts.iter().find(|(p, _)| p == path).map(|(_, m)| m.as_str())
}

/// Every row the map and what was seen call for, plus the findings: anything
/// discovered that no entry names (unmapped), and every store on a machine or
/// in a Fuseki that could not be read (unmeasured, never "absent").
pub fn desired(map: &StoreMap, seen: &Seen, ts: &str) -> Desired {
    let mut rows: Vec<Row> = Vec::new();
    let mut findings = Vec::new();
    let mut reported = Vec::new();
    let machine = |m: &str| seen.machines.iter().find(|(n, _)| n == m).map(|(_, w)| w);
    // exact graph and dataset entries win over a prefix entry for the same item
    let exact_graphs: Vec<&str> = map.entries.iter().filter_map(|e| match &e.matcher { Match::Graph { exact: Some(x), .. } => Some(x.as_str()), _ => None }).collect();
    let exact_sets: Vec<&str> = map.entries.iter().filter_map(|e| match &e.matcher { Match::Dataset { exact: Some(x), .. } => Some(x.as_str()), _ => None }).collect();

    for e in &map.entries {
        let mut made: Vec<Row> = Vec::new();
        let mut unmeasured: Option<String> = None;
        match &e.matcher {
            Match::Path(p) => match machine(&e.machine) {
                Some(Ok(ms)) if ms.paths.contains(p) => made.push(row(e, e.name.clone(), p, None)),
                Some(Ok(_)) => {} // not there: absent, decided against the door's rows
                Some(Err(r)) => unmeasured = Some(r.clone()),
                None => unmeasured = Some(format!("machine {} is not walked", e.machine)),
            },
            Match::PathPrefix(pre) => match machine(&e.machine) {
                Some(Ok(ms)) => {
                    // an exact path entry on the same machine wins over the prefix
                    let exact: Vec<&str> = map.entries.iter().filter(|x| x.machine == e.machine).filter_map(|x| match &x.matcher { Match::Path(p) => Some(p.as_str()), _ => None }).collect();
                    for p in ms.paths.iter().filter(|p| p.starts_with(pre.as_str()) && !exact.contains(&p.as_str())) {
                        let tail = p.rsplit('/').next().unwrap_or(p);
                        made.push(row(e, format!("{}-{}", e.name, slug(tail)), p, None));
                    }
                }
                Some(Err(r)) => unmeasured = Some(r.clone()),
                None => unmeasured = Some(format!("machine {} is not walked", e.machine)),
            },
            Match::Port(port) => match machine(&e.machine) {
                Some(Ok(ms)) if ms.ports_unmeasured => unmeasured = Some("the port listing (lsof) could not run".into()),
                Some(Ok(ms)) if ms.ports.contains(port) => made.push(row(e, e.name.clone(), &format!(":{port}"), None)),
                Some(Ok(_)) => {}
                Some(Err(r)) => unmeasured = Some(r.clone()),
                None => unmeasured = Some(format!("machine {} is not walked", e.machine)),
            },
            Match::Dataset { exact, prefixes, per_match } | Match::Graph { exact, prefixes, per_match } => {
                let is_graph = matches!(e.matcher, Match::Graph { .. });
                let (items, taken) = if is_graph { (&seen.graphs, &exact_graphs) } else { (&seen.datasets, &exact_sets) };
                match items {
                    None => unmeasured = Some("Fuseki did not answer".into()),
                    Some(items) => {
                        if let Some(x) = exact {
                            if items.contains(x) {
                                made.push(row(e, e.name.clone(), x, None));
                            }
                        } else {
                            let hits: Vec<&String> = items.iter().filter(|i| prefixed(i, prefixes).is_some() && !taken.contains(&i.as_str())).collect();
                            if *per_match {
                                for h in &hits {
                                    // a graph is named by what follows its prefix (pods-domain-logs); a
                                    // dataset by its whole name, since the prefix can be all of it (/chorus-test)
                                    let tail = if is_graph { prefixed(h, prefixes).map(|p| &h[p.len()..]).unwrap_or(h) } else { h.as_str() };
                                    made.push(row(e, format!("{}-{}", e.name, slug(tail)), h, None));
                                }
                            } else if !hits.is_empty() {
                                // a family seen only through its own prefix has no count to give
                                let real = hits.iter().filter(|h| !prefixes.contains(h)).count();
                                made.push(row(e, e.name.clone(), &prefixes.join(" "), (real > 0).then_some(real)));
                            }
                        }
                    }
                }
            }
        }
        if let Some(reason) = unmeasured {
            findings.push(Finding::Unmeasured { store: e.name.clone(), reason });
            continue;
        }
        for mut r in made {
            set(&mut r, "storeStatus", "present".into());
            set(&mut r, "lastObserved", ts.into());
            if e.in_store.is_none() {
                // its parent is the disk it sits on
                let ms = match machine(&e.machine) { Some(Ok(ms)) => Some(ms), _ => None };
                let at = e.disk_path.clone().unwrap_or_else(|| match &e.matcher {
                    Match::Path(_) | Match::PathPrefix(_) => r.get("storeNamespace").to_string(),
                    _ => String::new(),
                });
                match ms.and_then(|ms| mount_of(ms, &at)) {
                    Some(mount) => set(&mut r, "onDisk", disk_name(&e.machine, mount)),
                    None => findings.push(Finding::Unmeasured { store: r.name.clone(), reason: format!("no disk read for {at:?}") }),
                }
            }
            rows.push(r);
        }
    }

    // the unmapped check: everything discovered that no entry and no report rule names
    let covers = |item: &str, graph: bool| {
        map.entries.iter().any(|e| match (&e.matcher, graph) {
            (Match::Graph { exact, prefixes, .. }, true) | (Match::Dataset { exact, prefixes, .. }, false) => {
                exact.as_deref() == Some(item) || prefixed(item, prefixes).is_some()
            }
            _ => false,
        })
    };
    for g in seen.graphs.iter().flatten() {
        if map.report_graphs.iter().any(|p| g.starts_with(p.as_str())) {
            reported.push(format!("residue graph {g}"));
        } else if !covers(g, true) {
            findings.push(Finding::Unmapped { what: format!("graph {g}") });
        }
    }
    for d in seen.datasets.iter().flatten() {
        if !covers(d, false) {
            findings.push(Finding::Unmapped { what: format!("Fuseki dataset {d}") });
        }
    }
    let mapped_paths: Vec<&str> = map.entries.iter().filter_map(|e| match &e.matcher { Match::Path(p) => Some(p.as_str()), _ => None }).collect();
    for (m, w) in &seen.machines {
        let Ok(ms) = w else { continue };
        for f in &ms.scanned {
            if mapped_paths.iter().any(|p| f == p || f.starts_with(&format!("{p}/"))) {
                continue;
            }
            if map.report_paths.iter().any(|p| f.starts_with(p.as_str())) {
                reported.push(format!("stray {m}:{f}"));
            } else {
                findings.push(Finding::Unmapped { what: format!("file {m}:{f}") });
            }
        }
    }
    Desired { rows, findings, reported }
}

/// Rows the door holds that this run did not find, on a source it could read:
/// marked absent (their row stays, AC4), never deleted. A row already absent,
/// or whose store could not be measured this run, is left alone.
pub fn absent(current: &[Row], desired: &Desired) -> Vec<Row> {
    let unmeasured: Vec<&str> = desired.findings.iter().filter_map(|f| match f { Finding::Unmeasured { store, .. } => Some(store.as_str()), _ => None }).collect();
    current
        .iter()
        .filter(|c| !desired.rows.iter().any(|d| d.name == c.name))
        .filter(|c| c.get("storeStatus") != "absent")
        .filter(|c| !unmeasured.iter().any(|u| c.name == *u || c.name.starts_with(&format!("{u}-"))))
        .map(|c| {
            let mut r = Row { kind: c.kind, name: c.name.clone(), fields: door_form(c.fields.clone()) };
            set(&mut r, "storeStatus", "absent".into());
            r
        })
        .collect()
}

/// #4485 + #4422 — the disks a full-disk finding can fire on: those a LIVE
/// store sits on. A closed store (the video archive, filled on purpose) never
/// makes its disk a finding (Wren 2026-10-11).
pub fn live_store_disks(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .filter(|r| r.get("writeMode") != "closed" && r.get("storeStatus") != "absent")
        .map(|r| r.get("onDisk").to_string())
        .filter(|d| !d.is_empty())
        .collect()
}

/// The graphs the map itself names: each exact graph, and each family's prefix
/// standing for the family. The crawl cannot list the store's graphs without
/// SPARQL (#4433), so these are taken as present; the Domain rows add the rest.
pub fn map_graphs(map: &StoreMap) -> Vec<String> {
    map.entries
        .iter()
        .flat_map(|e| match &e.matcher {
            Match::Graph { exact: Some(x), .. } => vec![x.clone()],
            Match::Graph { exact: None, prefixes, per_match: false } => prefixes.clone(),
            _ => vec![],
        })
        .collect()
}

/// A row as the door lists it → the form this crawl writes and compares: links
/// carry the door's kind prefix on the way out (onDisk "disk-…", inStore
/// "persistence-store-…") and go back bare; empty values and the door's own
/// `status` are not fields of this row.
pub fn door_form(fields: Vec<(String, String)>) -> Vec<(String, String)> {
    const PREFIX: &[(&str, &str)] = &[
        ("onDisk", "disk-"),
        ("inStore", "persistence-store-"),
        ("roleSource", "persistence-store-"),
        ("storeServedBy", "service-instance-"),
    ];
    fields
        .into_iter()
        .filter(|(k, v)| !v.is_empty() && k != "status" && k != "name")
        .map(|(k, v)| {
            let v = v.rsplit(':').next().filter(|_| v.starts_with("chorus:")).map(String::from).unwrap_or(v);
            match PREFIX.iter().find(|(f, _)| *f == k) {
                Some((_, p)) => {
                    let bare = v.strip_prefix(p).map(String::from).unwrap_or(v);
                    (k, bare)
                }
                None => (k, v),
            }
        })
        .collect()
}

/// A store's disk must be a row before the store can link to it (the writer
/// refuses a link to a row that does not exist, and refuses the whole batch with
/// it). A store on a disk the infra crawl has not written yet is held back and
/// named, not sent: run `chorus-crawl infra` first. 2026-10-11: a Bedroom drive
/// renamed since the last infra run took 38 stores down with it.
pub fn hold_unwritten_disks(want: &mut Desired, disks: &[String]) {
    let (keep, held): (Vec<Row>, Vec<Row>) = std::mem::take(&mut want.rows)
        .into_iter()
        .partition(|r| r.get("onDisk").is_empty() || disks.iter().any(|d| d == r.get("onDisk")));
    for r in &held {
        want.findings.push(Finding::Unmeasured {
            store: r.name.clone(),
            reason: format!("its disk {} has no row yet — run `chorus-crawl infra` first", r.get("onDisk")),
        });
    }
    want.rows = keep;
}
