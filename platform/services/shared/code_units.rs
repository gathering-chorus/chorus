// #4499 — the code crawl: every unit of code that builds as one (a Rust crate,
// a TS package, the model a generated API is made from, a shared include) and
// what it depends on, read from its own build files on every land. Jeff,
// 2026-10-11 10:00: the blast radius is "a volatile data set" — so nothing here
// is hand-written, and an edge the build files no longer state disappears.
//
// One generator, included by chorus-crawl (which writes the rows) and, through
// scope_units.rs, by werk-build, werk-test and werk-deploy (which read the
// same edges to pick what to build, test and deploy). Pure except units_in,
// which lists the build files git tracks. std only.

/// One unit and the units it depends on, both by repo path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub path: String,
    pub kind: &'static str,
    pub deps: Vec<String>,
    /// the shared files a crate compiles in (`include!` or `#[path]`), repo paths
    pub includes: Vec<String>,
    /// the package name (Cargo `name =` or package.json `"name"`), when stated
    pub name: String,
}

/// The folders the model lives in (athena-gen's own list, model.rs:78): a
/// generated API depends on each, and on the generator.
pub const MODEL_DIRS: [&str; 4] = ["designing/data", "roles/kade/ontology", "roles/silas/ontology", "roles/wren/ontology"];
pub const GENERATOR: &str = "designing/services/athena-gen";
pub const SHARED: &str = "platform/services/shared";

/// `a/b/c` joined with `../../x` → `a/x`. None when it climbs out of the repo.
pub fn join(base: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// A crate's path dependencies, as cargo reads them from its Cargo.toml
/// (`x = { path = "../x" }`), resolved to repo paths.
pub fn crate_deps(crate_path: &str, cargo_toml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in cargo_toml.lines() {
        let Some(i) = line.find("path") else { continue };
        let rest = line[i + 4..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let Some(q) = rest.trim_start().strip_prefix('"') else { continue };
        let Some(end) = q.find('"') else { continue };
        if let Some(p) = join(crate_path, &q[..end]) {
            if p != crate_path && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// The shared files a crate compiles in: `include!("../../shared/x.rs")` and
/// `#[path = "../../shared/x.rs"]`, as repo paths. A change to one rebuilds it.
pub fn shared_includes(sources: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for text in sources {
        for line in text.lines() {
            let target = if let Some(i) = line.find("include!(\"") {
                let rest = &line[i + "include!(\"".len()..];
                rest.find('"').map(|q| &rest[..q])
            } else if let Some(i) = line.find("#[path = \"") {
                let rest = &line[i + "#[path = \"".len()..];
                rest.find('"').map(|q| &rest[..q])
            } else {
                None
            };
            let Some(t) = target else { continue };
            if !t.contains("shared/") {
                continue;
            }
            if let Some(name) = t.rsplit('/').next() {
                let p = format!("{SHARED}/{name}");
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

/// #4499 reopen 2026-10-14 — the shared files a shared file compiles in, so a
/// crate's includes are closed: scope_units.rs carries `include!("code_units.rs")`
/// (a name relative to shared/, no `shared/` in it), and werk-build, werk-test
/// and werk-deploy reach code_units.rs only that way. Read directly, a change
/// to it rebuilt chorus-crawl alone and the cache kept werk-test (Silas, #4444).
pub fn shared_closure(direct: Vec<String>, read: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out = direct;
    let mut i = 0;
    while i < out.len() {
        let text = read(&out[i]).unwrap_or_default();
        for line in text.lines() {
            let target = if let Some(k) = line.find("include!(\"") {
                let rest = &line[k + "include!(\"".len()..];
                rest.find('"').map(|q| &rest[..q])
            } else if let Some(k) = line.find("#[path = \"") {
                let rest = &line[k + "#[path = \"".len()..];
                rest.find('"').map(|q| &rest[..q])
            } else {
                None
            };
            let Some(t) = target else { continue };
            // relative to the shared dir: a bare name, or one that names shared/
            if t.contains('/') && !t.contains("shared/") {
                continue;
            }
            if let Some(name) = t.rsplit('/').next() {
                let p = format!("{SHARED}/{name}");
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        i += 1;
    }
    out.sort();
    out
}

/// The `name` a Cargo.toml or package.json states for its package.
pub fn package_name(text: &str, json: bool) -> String {
    if json {
        let Some(i) = text.find("\"name\"") else { return String::new() };
        let rest = &text[i + 6..];
        let rest = rest[rest.find(':').map_or(0, |c| c + 1)..].trim_start();
        return rest.strip_prefix('"').and_then(|r| r.find('"').map(|e| r[..e].to_string())).unwrap_or_default();
    }
    text.lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("name").map(str::trim_start).and_then(|r| r.strip_prefix('=')))
        .and_then(|r| r.trim().strip_prefix('"').and_then(|r| r.find('"').map(|e| r[..e].to_string())))
        .unwrap_or_default()
}

/// Is this crate generated by athena-gen from the model (its Cargo.toml says so
/// on its first line)?
pub fn is_generated(cargo_toml: &str) -> bool {
    cargo_toml.lines().next().is_some_and(|l| l.contains("Generated by athena-gen"))
}

/// A TS package's local dependencies: `"file:../x"` entries in package.json and
/// project `references` in its tsconfig, resolved to repo paths.
pub fn package_deps(pkg_path: &str, package_json: &str, tsconfig: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let mut add = |rel: &str| {
        if let Some(p) = join(pkg_path, rel) {
            if p != pkg_path && !out.contains(&p) {
                out.push(p);
            }
        }
    };
    for piece in package_json.split("\"file:").skip(1) {
        if let Some(end) = piece.find('"') {
            add(&piece[..end]);
        }
    }
    if let Some(ts) = tsconfig {
        if let Some(refs) = ts.split("\"references\"").nth(1) {
            let block = refs.split(']').next().unwrap_or("");
            for piece in block.split("\"path\"").skip(1) {
                let v = piece.trim_start().trim_start_matches(':').trim_start();
                if let Some(v) = v.strip_prefix('"') {
                    if let Some(end) = v.find('"') {
                        add(v[..end].trim_end_matches("/tsconfig.json"));
                    }
                }
            }
        }
    }
    out
}

/// The door's name for a unit: its path, `/` as `-` (platform-services-werk-test).
/// #4405 — a unit's door name carries its repository, so two repos' units
/// keep apart ("chorus-platform-services-werk-test"), built with the door's own slug.
pub fn unit_name(repo: &str, path: &str) -> String {
    let p = if path == ROOT { "root" } else { path };
    door_slug(&format!("{repo}-{p}"))
}

/// #4405 — the name the door stores: lowercase, anything outside a-z0-9
/// becomes "-", runs of "-" collapse, the ends are trimmed. A name built any
/// other way is created under one name and then written to under another
/// (`shared-security--root` was stored as `shared-security-root`, so the PUT
/// that followed was a 404 and the next plan deleted the row it had made).
pub fn door_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out.trim_matches('-').to_string()
}

/// #4405 — a build file at the repository root (a plain Node or Rust repo is
/// one) is a unit whose path is ".": it holds every file no deeper unit does.
pub const ROOT: &str = ".";

/// Is `file` inside the unit at `path`? The root unit holds every file.
pub fn within(file: &str, path: &str) -> bool {
    path == ROOT || file == path || file.starts_with(&format!("{}/", path))
}

/// #4405 — the repository a tree is: the remote's last segment without .git
/// (gathering-chorus/chorus.git → chorus), else the directory's own name.
pub fn repo_id(remote: Option<&str>, dir: &std::path::Path) -> String {
    let from_remote = remote
        .map(|r| r.trim().trim_end_matches('/').trim_end_matches(".git"))
        .and_then(|r| r.rsplit(['/', ':']).next())
        .filter(|n| !n.is_empty());
    let raw = from_remote
        .map(String::from)
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "repo".to_string());
    raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' }).collect()
}

/// The unit a file belongs to: the one whose path is the longest prefix of it.
pub fn unit_of<'a>(file: &str, units: &'a [Unit]) -> Option<&'a Unit> {
    units
        .iter()
        .filter(|u| within(file, &u.path))
        .max_by_key(|u| u.path.len())
}

/// The unit a file belongs to, not counting the root unit: what a service's
/// run path resolves to. A root build file (Chorus's is the lint workspace)
/// is not what a service runs, and a script in no deeper unit is read for
/// the code it names instead.
pub fn unit_below_root<'a>(file: &str, units: &'a [Unit]) -> Option<&'a Unit> {
    unit_of(file, units).filter(|u| u.path != ROOT)
}

/// Every unit that depends on `path`, directly or through other units, nearest
/// first, each once. The blast radius of a change to that unit.
pub fn dependents(path: &str, units: &[Unit]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut frontier = vec![path.to_string()];
    while let Some(p) = frontier.pop() {
        for u in units.iter().filter(|u| u.deps.contains(&p)) {
            if u.path != path && !out.contains(&u.path) {
                out.push(u.path.clone());
                frontier.insert(0, u.path.clone());
            }
        }
    }
    out
}

/// The units in a repo, from its build files. `read` returns a file's text by
/// repo path; `crate_sources` returns a crate's src/*.rs texts.
pub fn units(
    cargo_tomls: &[String],
    package_jsons: &[String],
    python: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    crate_sources: &dyn Fn(&str) -> Vec<String>,
) -> Vec<Unit> {
    let dir = |f: &str| f.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_else(|| ROOT.to_string());
    let mut out: Vec<Unit> = Vec::new();
    let mut shared_used = false;
    for f in cargo_tomls {
        let path = dir(f);
        let toml = read(f).unwrap_or_default();
        let mut deps = crate_deps(&path, &toml);
        let includes = shared_closure(shared_includes(&crate_sources(&path)), read);
        if !includes.is_empty() {
            deps.push(SHARED.to_string());
            shared_used = true;
        }
        if is_generated(&toml) {
            deps.extend(MODEL_DIRS.iter().map(|m| m.to_string()));
            deps.push(GENERATOR.to_string());
        }
        out.push(Unit { name: package_name(&toml, false), path, kind: "crate", deps, includes });
    }
    for f in package_jsons {
        let path = dir(f);
        let pj = read(f).unwrap_or_default();
        let ts = read(&format!("{path}/tsconfig.json"));
        out.push(Unit { deps: package_deps(&path, &pj, ts.as_deref()), name: package_name(&pj, true), path, kind: "package", includes: Vec::new() });
    }
    // #4405 — a Python project (pyproject.toml, setup.py or requirements.txt)
    // is a package; one unit per directory however many of those it has
    for f in python {
        if !matches!(f.rsplit('/').next(), Some("pyproject.toml" | "setup.py" | "requirements.txt")) {
            continue;
        }
        let path = dir(f);
        if out.iter().any(|u| u.path == path) {
            continue;
        }
        out.push(Unit { path, kind: "package", deps: Vec::new(), includes: Vec::new(), name: String::new() });
    }
    if shared_used {
        out.push(Unit { path: SHARED.to_string(), kind: "include", deps: Vec::new(), includes: Vec::new(), name: String::new() });
    }
    // #4405 — the model folders are units only where a generated API reads
    // them; every other repository got four phantom "model" units
    if out.iter().any(|u| u.deps.iter().any(|d| MODEL_DIRS.contains(&d.as_str()))) {
        for m in MODEL_DIRS {
            out.push(Unit { path: m.to_string(), kind: "model", deps: Vec::new(), includes: Vec::new(), name: String::new() });
        }
    }
    // an edge to a path that is not a unit is a build file pointing nowhere:
    // kept off the graph, so no dangling dependsOn is ever written
    let known: Vec<String> = out.iter().map(|u| u.path.clone()).collect();
    for u in out.iter_mut() {
        u.deps.retain(|d| known.contains(d));
        u.deps.sort();
        u.deps.dedup();
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The row the door holds for a unit: its path and kind, and one dependsOn per
/// edge, by the target unit's door name.
pub fn unit_fields(u: &Unit, repo: &str) -> Vec<(String, String)> {
    let mut f = vec![
        ("name".to_string(), unit_name(repo, &u.path)),
        ("inRepository".to_string(), repo.to_string()),
        ("unitPath".to_string(), u.path.clone()),
        ("unitKind".to_string(), u.kind.to_string()),
    ];
    f.extend(u.deps.iter().map(|d| ("dependsOn".to_string(), unit_name(repo, d))));
    f
}

/// Is a build-file path a real unit, not a fixture or vendored tree?
pub fn is_unit_file(path: &str) -> bool {
    // #4405 — by path component, so a top-level tests/ or fixtures/ (another
    // repo's layout) is excluded the same as a nested one
    !path.split('/').any(|c| matches!(c, "node_modules" | "fixtures" | "tests" | "target"))
}

/// What to write: units to create, units whose fields changed (their edges
/// are replaced whole, so a dropped dependency goes), units no build file
/// states any more (deleted), and how many are unchanged. Rows are
/// (name, fields) with the name left out of fields. Edges compare as sets.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnitPlan {
    pub create: Vec<(String, Vec<(String, String)>)>,
    pub update: Vec<(String, Vec<(String, String)>)>,
    pub delete: Vec<String>,
    pub unchanged: usize,
}

fn canon(fields: &[(String, String)]) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = fields
        .iter()
        .filter(|(k, v)| !v.is_empty() && matches!(k.as_str(), "inRepository" | "unitPath" | "unitKind" | "dependsOn"))
        .cloned()
        .collect();
    f.sort();
    f.dedup();
    f
}

pub fn plan_units(desired: &[(String, Vec<(String, String)>)], current: &[(String, Vec<(String, String)>)]) -> UnitPlan {
    let mut p = UnitPlan::default();
    for (name, fields) in desired {
        match current.iter().find(|(n, _)| n == name) {
            None => p.create.push((name.clone(), fields.clone())),
            Some((_, cur)) if canon(cur) == canon(fields) => p.unchanged += 1,
            Some(_) => p.update.push((name.clone(), fields.clone())),
        }
    }
    for (name, _) in current {
        if !desired.iter().any(|(n, _)| n == name) {
            p.delete.push(name.clone());
        }
    }
    p
}

/// The units in the tree at `root`, from the build files git tracks there —
/// no folder walk. None when git cannot list the tree (not a repo).
pub fn units_in(root: &std::path::Path) -> Option<Vec<Unit>> {
    let ls = |pat: &str| -> Option<Vec<String>> {
        let o = std::process::Command::new("git").arg("-C").arg(root).args(["ls-files", "--", pat]).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).lines().filter(|l| is_unit_file(l)).map(String::from).collect())
    };
    let (cargo, pkgs) = (ls("*Cargo.toml")?, ls("*package.json")?);
    let mut py = ls("*pyproject.toml")?;
    py.extend(ls("*setup.py")?);
    py.extend(ls("*requirements.txt")?);
    let read = |f: &str| std::fs::read_to_string(root.join(f)).ok();
    let sources = |dir: &str| -> Vec<String> {
        std::fs::read_dir(root.join(dir).join("src")).map(|rd| rd.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect()).unwrap_or_default()
    };
    Some(units(&cargo, &pkgs, &py, &read, &sources))
}

/// The binaries a crate builds, as (binary name, crate path): its package name
/// and every `[[bin]] name`. What `~/.chorus/bin/<x>` was built from.
pub fn cargo_bins(crate_path: &str, cargo_toml: &str) -> Vec<(String, String)> {
    let mut names = vec![package_name(cargo_toml, false)];
    let mut in_bin = false;
    for l in cargo_toml.lines().map(str::trim) {
        if l.starts_with('[') {
            in_bin = l == "[[bin]]";
        } else if in_bin {
            if let Some(n) = l.strip_prefix("name").map(str::trim_start).and_then(|r| r.strip_prefix('=')) {
                names.push(n.trim().trim_matches('"').to_string());
            }
        }
    }
    names.retain(|n| !n.is_empty());
    names.dedup();
    names.into_iter().map(|n| (n, crate_path.to_string())).collect()
}

/// The repo paths an instance runs, from what launchd starts — never from a
/// name. Its source file when that sits in a unit; the binary launchd runs: a
/// `~/.chorus/bin/<x>` (or the deploy's `<x>-bin`, or a werk's
/// `target/release/<x>`) is the crate whose Cargo.toml builds `x`; a repo path
/// is itself; and a script in no unit is read once for the paths it names. A generic runner whose target lives in the plist names
/// nothing and links nothing.
pub fn run_paths(
    source: &str,
    binary: &str,
    repo_root: &str,
    home: &str,
    units: &[Unit],
    bins: &[(String, String)],
    read_abs: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let bin_dir = format!("{home}/.chorus/bin/");
    let resolve = |abs: &str| -> Option<String> {
        // a cargo build output (a werk's target/release) is its crate's binary too
        let built = abs.rsplit_once("/target/release/").map(|(_, n)| n);
        if let Some(name) = abs.strip_prefix(&bin_dir).or(built) {
            let name = name.strip_suffix("-bin").unwrap_or(name);
            return bins.iter().find(|(b, _)| b == name).map(|(_, c)| c.clone());
        }
        abs.strip_prefix(repo_root).and_then(|r| r.strip_prefix('/')).filter(|r| unit_below_root(r, units).is_some()).map(String::from)
    };
    let expand = |t: &str| -> String {
        let t = t.replace("${HOME}", home).replace("$HOME", home);
        let t = if let Some(r) = t.strip_prefix("~/") { format!("{home}/{r}") } else { t };
        ["${CHORUS_ROOT}", "$CHORUS_ROOT", "${CHORUS_HOME}", "$CHORUS_HOME"].iter().fold(t, |t, v| t.replace(v, repo_root))
    };
    let mut out: Vec<String> = Vec::new();
    let mut push = |p: String| if !out.contains(&p) { out.push(p) };
    if !source.is_empty() && unit_below_root(source, units).is_some() {
        push(source.to_string());
    }
    if !binary.is_empty() {
        match resolve(binary) {
            Some(p) => push(p),
            None => {
                let script = read_abs(binary).unwrap_or_default();
                for line in script.lines().filter(|l| !l.trim_start().starts_with('#')) {
                    for tok in line.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '=' | '(' | ')' | ';')) {
                        if let Some(p) = resolve(&expand(tok)) {
                            push(p);
                        }
                    }
                }
            }
        }
    }
    out
}

/// #4499 — which units each Service is built from, through its running
/// instances: an instance runs a service (runsService, #4472) from a source
/// file (sourcePath), and that file's unit is what the service is built from.
/// Input: (service, source file repo path) per instance. Output: service →
/// unit door names, sorted. An instance whose file is in no unit adds nothing.
pub fn built_from(instances: &[(String, String)], units: &[Unit], repo: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (svc, file) in instances {
        let Some(u) = unit_below_root(file, units) else { continue };
        let name = unit_name(repo, &u.path);
        match out.iter_mut().find(|(s, _)| s == svc) {
            Some((_, v)) => {
                if !v.contains(&name) {
                    v.push(name);
                }
            }
            None => out.push((svc.clone(), vec![name])),
        }
    }
    for (_, v) in out.iter_mut() {
        v.sort();
    }
    out.sort();
    out
}
