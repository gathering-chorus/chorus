//! #4499 — the code crawl's decisions, as data: which unit rows to write,
//! which units each running service is built from, and the writes in the
//! order the door needs them. `run_code` in main.rs only does the I/O.

use crate::code::{run_paths, unit_fields, Unit, UnitPlan};

pub type Fields = Vec<(String, String)>;

/// The CodeUnit rows the build files state: (name, fields without the name).
pub fn desired_rows(units: &[Unit], repo: &str) -> Vec<(String, Fields)> {
    units
        .iter()
        .map(|u| {
            let f = unit_fields(u, repo);
            (f[0].1.clone(), f[1..].to_vec())
        })
        .collect()
}

/// The CodeUnit rows the store holds, in the same shape as `desired_rows`.
pub fn current_rows(rows: Vec<Fields>) -> Vec<(String, Fields)> {
    rows.into_iter()
        .map(|fields| {
            let name = fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()).unwrap_or_default();
            (name, crate::services::written_form(fields.into_iter().filter(|(k, _)| k != "name").collect()))
        })
        .collect()
}

/// #4405 — the stored rows this pass may change: the ones in its own
/// repository. Crawling a second repo must never delete the first one's units
/// (the plan deletes every current row it does not want). A row written before
/// rows carried a repository belongs to the home repo, and only the home repo's
/// pass may adopt or retire it.
pub fn rows_of_repo(current: Vec<(String, Fields)>, repo: &str, home_repo: &str) -> Vec<(String, Fields)> {
    current
        .into_iter()
        .filter(|(_, f)| match f.iter().find(|(k, _)| k == "inRepository") {
            Some((_, r)) => r.rsplit("repository-").next() == Some(repo),
            None => repo == home_repo,
        })
        .collect()
}

/// (service, path) for every ServiceInstance / ScheduledJob row that names a
/// service: the paths its source file, its binary or its launch script lead
/// to. `file_of` turns a sourcePath edge into a repo path and is asked once
/// per distinct edge. A row with no runsService links nothing.
#[allow(clippy::too_many_arguments)]
pub fn instance_links(
    rows: &[Fields],
    file_of: &mut dyn FnMut(&str) -> String,
    repo_root: &str,
    home: &str,
    units: &[Unit],
    bins: &[(String, String)],
    read_abs: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut out = Vec::new();
    for r in rows {
        let get = |k: &str| r.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        let (svc, src, bin) = (get("runsService"), get("sourcePath"), get("binaryPath"));
        if svc.is_empty() {
            continue;
        }
        let file = if src.is_empty() {
            String::new()
        } else if let Some((_, f)) = files.iter().find(|(n, _)| *n == src) {
            f.clone()
        } else {
            let f = file_of(&src);
            files.push((src.clone(), f.clone()));
            f
        };
        let svc = svc.strip_prefix("service-").unwrap_or(&svc).to_string();
        for p in run_paths(&file, &bin, repo_root, home, units, bins, read_abs) {
            out.push((svc.clone(), p));
        }
    }
    out
}

/// The door's name for a CodeFile row from the edge's IRI local name
/// (code-file-<name> → <name>).
pub fn code_file_door_name(edge: &str) -> &str {
    edge.strip_prefix("code-file-").unwrap_or(edge)
}

/// One write against the door. `rows` is the body: none for a DELETE, one row
/// for PUT/PATCH, many for a batch POST.
#[derive(Debug, PartialEq, Eq)]
pub struct Write {
    pub method: &'static str,
    pub path: String,
    pub rows: Vec<Fields>,
    pub what: String,
}

fn row(name: &str, fields: &[(String, String)]) -> Fields {
    let mut f = vec![("name".to_string(), name.to_string())];
    f.extend(fields.iter().cloned());
    f
}

/// The writes, in order: new units first without their edges (so every
/// edge's target exists when it is written), then every created or changed
/// unit whole, then deletes, then each service's builtFrom merged onto its
/// row (PATCH — the rest of a Service row belongs to other writers). No
/// Service collection served → no builtFrom writes.
pub fn code_writes(coll: &str, svc_coll: Option<&str>, plan: &UnitPlan, built: &[(String, Vec<String>)]) -> Vec<Write> {
    let mut w = Vec::new();
    let bare: Vec<Fields> = plan
        .create
        .iter()
        .map(|(n, f)| row(n, &f.iter().filter(|(k, _)| k != "dependsOn").cloned().collect::<Vec<_>>()))
        .collect();
    if !bare.is_empty() {
        w.push(Write { method: "POST", path: format!("{coll}/batch"), what: format!("create batch of {}", bare.len()), rows: bare });
    }
    for (n, f) in plan.create.iter().chain(plan.update.iter()) {
        w.push(Write { method: "PUT", path: format!("{coll}/{n}"), rows: vec![row(n, f)], what: format!("write {n}") });
    }
    for n in &plan.delete {
        w.push(Write { method: "DELETE", path: format!("{coll}/{n}"), rows: Vec::new(), what: format!("delete {n}") });
    }
    if let Some(sc) = svc_coll {
        for (svc, us) in built {
            let patch: Fields = us.iter().map(|u| ("builtFrom".to_string(), u.clone())).collect();
            w.push(Write { method: "PATCH", path: format!("{sc}/{svc}"), rows: vec![patch], what: format!("builtFrom {svc}") });
        }
    }
    w
}

