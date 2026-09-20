//! #4214 — the UI-Pages and API-Contract folds.
//!
//! A Page is a file that renders a route a person can open. An Endpoint is a
//! route an API serves, and the file that serves it. Jeff, 2026-09-18 15:33:
//! *"these are special cases of Code so if anything they are child classes"* —
//! so both are `rdfs:subClassOf chorus:CodeFile`, live in the code domain's
//! graph, and inherit `filePath`, `hasKind`, `hasDomain` and `ownedBy`. This
//! module adds only what the children carry: `route`/`pageType` and
//! `routePath`/`httpMethod`.
//!
//! What this REPLACES. `POST /api/athena/discover-pages` and
//! `/discover-endpoints` in chorus-api already wrote these rows, with raw
//! SPARQL, into `urn:chorus:instances`, and hung them off a `chorus:SubDomain`
//! by an inverse `hasPage`/`hasEndpoint` edge — the class #4187 is retiring.
//! Measured 2026-09-19: 63 Page + 448 Endpoint rows, `hasDomain` on none.
//!
//! What this does NOT carry over. The EJS classifier's last resort was a
//! name-prefix match against an alias table: a view called `music-foo` became
//! the music domain because of its NAME. That is the folder rule #4201 retired
//! ("a name is not evidence"). Here the route is read from the path, which is a
//! fact, and the domain is read from the file's CONTENT by the same rules that
//! place a CodeFile. A page those rules cannot place is listed by name and left
//! untagged — never dropped into a default.

/// A file that renders a route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRow {
    /// Repo-relative path, the same spelling the CodeFile row carries.
    pub path: String,
    /// The URL a person opens.
    pub route: String,
    /// What kind of surface it is: athena, loom, ontology, doc, page.
    pub page_type: String,
}

/// One route an API serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRow {
    /// The source file the route is declared in.
    pub path: String,
    pub route_path: String,
    pub http_method: String,
}

/// Which repo a page source lives in. Gathering is a separate checkout, and an
/// absent one is a SKIPPED scan, never an error — the same soft-fail contract
/// the TypeScript scanners carried (#3097).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repo {
    Chorus,
    Gathering,
}

/// A page source: one directory, the route prefix its files are served at, and
/// the extension that marks a rendered file. Adding a surface is a row here.
pub struct PageSource {
    pub repo: Repo,
    /// Repo-relative directory, from the root the walk was given.
    pub dir: &'static str,
    /// What the files under it are served at.
    pub route_prefix: &'static str,
    pub ext: &'static str,
    pub page_type: &'static str,
    /// Keep the extension in the route (`/athena/x.html`) or drop it (`/x`).
    pub route_keeps_ext: bool,
}

/// Every surface that renders a page today. The two gathering entries mirror
/// scanEjsViews / scanDocHtml; the roots are the same ones chorus-api scanned.
pub const PAGE_SOURCES: &[PageSource] = &[
    PageSource {
        repo: Repo::Chorus,
        dir: "platform/api/public/athena",
        route_prefix: "/athena/",
        ext: ".html",
        page_type: "athena",
        route_keeps_ext: true,
    },
    PageSource {
        repo: Repo::Chorus,
        dir: "platform/api/public/loom",
        route_prefix: "/loom/",
        ext: ".html",
        page_type: "loom",
        route_keeps_ext: true,
    },
    PageSource {
        repo: Repo::Gathering,
        dir: "public/gathering-docs",
        route_prefix: "/gathering-docs/",
        ext: ".html",
        page_type: "doc",
        route_keeps_ext: true,
    },
];

/// A template's route is DECLARED, not guessed.
///
/// The TypeScript scanner read a view's route off its filename through a table
/// of regexes — `collection-music.ejs` became `/music`, `foo-detail.ejs` became
/// `/foo/:slug` — and its last resort matched the name's prefix against an
/// alias table to pick a domain. That is the folder rule #4201 retired: a name
/// is not evidence. A template is reached by whatever route handler renders it,
/// which the router states in so many words.
///
/// This reads `app.<method>('<route>' ... .render('<view>')` and pairs each
/// render with the route declaration it sits inside. A view no handler renders
/// answers nothing, and is reported by name rather than given a route.
pub fn rendered_routes(content: &str) -> Vec<(String, String)> {
    let mut decls: Vec<(usize, String)> = Vec::new();
    for e in endpoints_at(content) {
        decls.push((e.0, e.1));
    }
    let mut out: Vec<(String, String)> = Vec::new();
    for (i, _) in content.match_indices(".render(") {
        let after = content[i + ".render(".len()..].trim_start();
        let quote = match after.chars().next() {
            Some(c @ ('\'' | '"' | '`')) => c,
            _ => continue,
        };
        let body = &after[quote.len_utf8()..];
        let Some(end) = body.find(quote) else { continue };
        let view = &body[..end];
        if view.is_empty() || view.contains("${") {
            continue;
        }
        // the route declaration this render sits inside: the nearest one before it
        let Some((_, route)) = decls.iter().filter(|(at, _)| *at < i).next_back() else {
            continue;
        };
        out.push((view.to_string(), route.clone()));
    }
    out.sort();
    out.dedup();
    out
}

/// The route one file under one source is served at. `rel` is the file's name
/// within `src.dir` — a name, never a path with a separator in it, because a
/// nested file is served at a route this table cannot state.
pub fn route_for(src: &PageSource, file_name: &str) -> Option<String> {
    if file_name.contains('/') || !file_name.ends_with(src.ext) {
        return None;
    }
    let stem = &file_name[..file_name.len() - src.ext.len()];
    if stem.is_empty() {
        return None;
    }
    Some(if src.route_keeps_ext {
        format!("{}{}", src.route_prefix, file_name)
    } else {
        format!("{}{}", src.route_prefix, stem)
    })
}

/// The page rows for one source directory, given the file names in it.
pub fn pages_in(src: &PageSource, file_names: &[String]) -> Vec<PageRow> {
    let mut out = Vec::new();
    for name in file_names {
        if let Some(route) = route_for(src, name) {
            out.push(PageRow {
                path: format!("{}/{}", src.dir, name),
                route,
                page_type: src.page_type.to_string(),
            });
        }
    }
    out
}

/// Every `app.<method>('<path>'` declaration in one source file.
///
/// Parameterised routes are kept, unlike the TypeScript scanner that dropped
/// anything containing `:`. `/api/chorus/cards/:id` is a route the API serves;
/// a contract that lists only the concrete ones is not the contract. The colon
/// is part of the row's value, so the fold shows the shape a caller must send.
pub fn endpoints_in(path: &str, content: &str) -> Vec<EndpointRow> {
    let mut out: Vec<EndpointRow> = endpoints_at(content)
        .into_iter()
        .map(|(_, route_path, method)| EndpointRow {
            path: path.to_string(),
            route_path,
            http_method: method,
        })
        .collect();
    out.sort_by(|a, b| (&a.route_path, &a.http_method).cmp(&(&b.route_path, &b.http_method)));
    out.dedup();
    out
}

/// Every route declaration in a source, as (byte offset, path, METHOD), in the
/// order they appear. The offset is what lets a render call be paired with the
/// handler it sits inside.
pub fn endpoints_at(content: &str) -> Vec<(usize, String, String)> {
    const METHODS: &[&str] = &["get", "post", "put", "delete", "patch"];
    let mut out = Vec::new();
    for (i, _) in content.match_indices("app.") {
        let tail = &content[i + 4..];
        let Some(m) = METHODS.iter().find(|m| tail.starts_with(**m)) else {
            continue;
        };
        let after = tail[m.len()..].trim_start();
        let Some(after) = after.strip_prefix('(') else {
            continue;
        };
        let after = after.trim_start();
        let quote = match after.chars().next() {
            Some(c @ ('\'' | '"' | '`')) => c,
            _ => continue,
        };
        let body = &after[quote.len_utf8()..];
        let Some(end) = body.find(quote) else { continue };
        let route_path = &body[..end];
        if !route_path.starts_with('/') {
            continue;
        }
        out.push((i, route_path.to_string(), m.to_uppercase()));
    }
    out
}

/// #4214 — the desired rows for one walk, selected from the tracked file list.
///
/// `paths` is the walk's repo-relative file list (git ls-files), `read` hands
/// back a file's content. Selection is by PATH, never by name: a file sits
/// DIRECTLY under a declared source directory or it is not a page. A nested
/// file is skipped rather than guessed at, because the route table cannot state
/// where a nested file is served — the same refusal `route_for` makes.
///
/// Gathering sources are skipped when that checkout is not present. A skipped
/// scan is not an error (#3097), but it IS reported, so "no gathering pages"
/// can never be confused with "gathering has no pages".
pub fn desired_rows(
    paths: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    gathering_present: bool,
) -> (Vec<PageRow>, Vec<EndpointRow>, Vec<&'static str>) {
    let mut pages: Vec<PageRow> = Vec::new();
    let mut skipped: Vec<&'static str> = Vec::new();
    for src in PAGE_SOURCES {
        if src.repo == Repo::Gathering && !gathering_present {
            skipped.push(src.dir);
            continue;
        }
        let prefix = format!("{}/", src.dir);
        // Nested files are refused by route_for, which is the ONE place that
        // decides whether a name has a statable route. A second filter here
        // looked like a guard and proved nothing — it could be deleted with
        // every test still green, which is how a hollow gate is spotted.
        let names: Vec<String> = paths
            .iter()
            .filter_map(|p| p.strip_prefix(&prefix))
            .map(str::to_string)
            .collect();
        pages.extend(pages_in(src, &names));
    }
    // Same rule one fold over: a route is one Page row.
    pages.sort_by(|a, b| (&a.route, &a.path).cmp(&(&b.route, &b.path)));
    pages.dedup_by(|a, b| a.route == b.route);

    let mut endpoints: Vec<EndpointRow> = Vec::new();
    for p in paths.iter().filter(|p| serves_routes(p)) {
        if let Some(body) = read(p) {
            endpoints.extend(endpoints_in(p, &body));
        }
    }
    // A row is keyed on METHOD + path, so two files declaring the same route are
    // ONE row, not two. Deduping whole structs kept both (their `path` differs)
    // and the door refused the batch: "duplicate entity name in request". First
    // file wins, deterministically, because the list is sorted before the cut.
    endpoints.sort_by(|a, b| {
        (&a.route_path, &a.http_method, &a.path).cmp(&(&b.route_path, &b.http_method, &b.path))
    });
    endpoints.dedup_by(|a, b| a.route_path == b.route_path && a.http_method == b.http_method);
    (pages, endpoints, skipped)
}

/// A file that can declare routes. TypeScript and JavaScript sources only —
/// reading every tracked file to look for `app.get(` would parse the whole
/// tree, and a route declared in a fixture or a doc is not a served route.
pub fn serves_routes(path: &str) -> bool {
    (path.ends_with(".ts") || path.ends_with(".js"))
        && !path.contains("/tests/")
        && !path.contains("/node_modules/")
        && !path.ends_with(".test.ts")
        && !path.ends_with(".d.ts")
}

/// Deterministic door name. Same shape as `log_row_name` and `stable_name`: a
/// readable slug plus a digest of the EXACT key, so two rows that slug alike
/// still get two rows (the collision that refused the first CodeFile batch).
/// #4214 — the BARE name. The door mints the type prefix itself and refuses a
/// name that already carries one ("double-prefix", 422, caught on the variant
/// 2026-09-19 before any prod row). So this builds slug + digest and nothing
/// else; `page:` / `endpoint:` is the DAL's to add.
fn row_name(_prefix: &str, key: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let mut end = slug.len().min(100);
    while !slug.is_char_boundary(end) {
        end -= 1;
    }
    let slug = slug[..end].trim_end_matches('-');
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{slug}-{:08x}", (h & 0xffff_ffff) as u32)
}

/// A page is keyed on its ROUTE, not its file: the route is what a person
/// opens, and two files never serve one route.
pub fn page_row_name(route: &str) -> String {
    row_name("page", route)
}

/// An endpoint is keyed on method + path together — `GET /x` and `POST /x` are
/// two entries in the contract.
pub fn endpoint_row_name(method: &str, route_path: &str) -> String {
    row_name("endpoint", &format!("{method} {route_path}"))
}

/// A row as the door serves it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InGraph {
    pub name: String,
    /// The row's key: the route for a page, "METHOD path" for an endpoint.
    pub key: String,
    pub domain: Option<String>,
    /// The WHOLE row as served. A PUT is full-replace (#3345/#4178), so a
    /// replace merges onto this; reading two strings and putting them back
    /// would delete every field the crawler does not own.
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowAction<T> {
    Post(T),
    Replace { name: String, row: T },
    Unchanged { key: String },
    Delete { name: String, key: String },
}

/// A name the door will address: its own minted shape, not a `urn:` id.
pub fn door_addressable(name: &str) -> bool {
    !name.is_empty() && !name.contains(':') && !name.contains('/')
}

/// What one pass does. `key_of` states each desired row's key; `domain_of`
/// states the domain the rules place it in NOW.
///
/// A row whose stored domain differs from what the rules say is stale even
/// though its content has not moved — the same question `plan_with` had to be
/// taught on #4201, where planning on the sha alone would have left 1,563 rows
/// blank forever.
pub fn plan_rows<T: Clone>(
    desired: &[T],
    rows: &[InGraph],
    key_of: &dyn Fn(&T) -> String,
    domain_of: &dyn Fn(&T) -> Option<String>,
    full: bool,
) -> Vec<RowAction<T>> {
    let mut out = Vec::new();
    let mut wanted: Vec<String> = Vec::new();
    for d in desired {
        let key = key_of(d);
        wanted.push(key.clone());
        match rows
            .iter()
            .find(|r| r.key == key && door_addressable(&r.name))
        {
            None => out.push(RowAction::Post(d.clone())),
            Some(r) if full || r.domain != domain_of(d) => out.push(RowAction::Replace {
                name: r.name.clone(),
                row: d.clone(),
            }),
            Some(_) => out.push(RowAction::Unchanged { key }),
        }
    }
    for r in rows {
        if !r.key.is_empty() && door_addressable(&r.name) && is_ours(r) && !wanted.contains(&r.key) {
            out.push(RowAction::Delete {
                name: r.name.clone(),
                key: r.key.clone(),
            });
        }
    }
    out
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    pub posted: usize,
    pub replaced: usize,
    pub unchanged: usize,
    pub deleted: usize,
}

pub fn counts<T>(actions: &[RowAction<T>]) -> Counts {
    let mut c = Counts::default();
    for a in actions {
        match a {
            RowAction::Post(_) => c.posted += 1,
            RowAction::Replace { .. } => c.replaced += 1,
            RowAction::Unchanged { .. } => c.unchanged += 1,
            RowAction::Delete { .. } => c.deleted += 1,
        }
    }
    c
}

/// #4222 — the domain an endpoint serves, read from its own route.
///
/// A route is the most honest signal an endpoint has: `/api/chorus/cards/:id`
/// serves cards, and no file-content heuristic beats reading the path the API
/// itself publishes. Two rules, in order, and nothing else:
///
///   1. any path segment that IS a domain name  → that domain
///   2. `/api/athena/...`                        → knowledge
///
/// Rule 2 exists because athena is the knowledge surface: 171 of the 227
/// untagged endpoints measured 2026-09-19 sit under it and name no domain of
/// their own. An endpoint matching neither stays unplaced and is reported —
/// the folder is never consulted, and no route gets a default.
pub(crate) const ROUTE: &[(&str, &str)] = &[
    ("/", "products"),
    ("/api/chorus/attention-analytics", "analytics"),
    ("/api/chorus/card-story/:id", "cards"),
    ("/api/chorus/codebase/topology", "code"),
    ("/api/chorus/conversation", "messages"),
    ("/api/chorus/crawl/:domain", "code"),
    ("/api/chorus/disk", "infrastructure"),
    ("/api/chorus/domain-story/:domain", "domains"),
    ("/api/chorus/fitness/summary", "metrics"),
    ("/api/chorus/freshness", "metrics"),
    ("/api/chorus/harvest", "integrations"),
    ("/api/chorus/jeff/posture/strip", "roles"),
    ("/api/chorus/patterns/summary", "analytics"),
    ("/api/chorus/perf", "metrics"),
    ("/api/chorus/reconcile", "code"),
    ("/api/chorus/refs", "knowledge"),
    ("/api/chorus/reprompt-analytics", "analytics"),
    ("/api/chorus/security-fitness", "security"),
    ("/api/chorus/seed-media/:filename", "memory"),
    ("/api/chorus/seeds", "memory"),
    ("/api/chorus/self", "roles"),
    ("/api/chorus/stats", "metrics"),
    ("/api/chorus/test-run/latest", "tests"),
    ("/api/chorus/ui-pages", "code"),
    ("/api/chorus/voice-analytics", "analytics"),
    ("/api/chorus/werk/activity", "cicd"),
    ("/api/doc-catalog", "knowledge"),
    ("/api/doc-catalog/add", "knowledge"),
    ("/api/doc-catalog/link", "knowledge"),
    ("/api/doc-catalog/tags", "knowledge"),
    ("/api/doc-catalog/tree", "knowledge"),
    ("/api/doc-inventory", "knowledge"),
    ("/api/loom-analytics", "analytics"),
    ("/api/loom-metrics", "metrics"),
    ("/api/photos", "products"),
    ("/api/photos/count", "products"),
    ("/api/photos/set", "products"),
    ("/api/playlists", "products"),
    ("/api/playlists/add", "products"),
    ("/api/playlists/remove", "products"),
    ("/api/proxy/image", "products"),
    ("/api/proxy/video", "products"),
    ("/api/video-tags", "products"),
    ("/api/videos", "products"),
    ("/api/videos/list", "products"),
    ("/api/werk/activity", "cicd"),
    ("/api/werk/schema", "cicd"),
    ("/borg-assessment", "analytics"),
    ("/chorus", "products"),
    ("/chorus-model-data", "domains"),
    ("/chorus/system", "services"),
    ("/clearing", "messages"),
    ("/flow", "value-streams"),
    ("/harvest-manifests", "integrations"),
    ("/harvesting/convergence", "integrations"),
    ("/harvesting/icd", "integrations"),
    ("/harvesting/mapper", "integrations"),
    ("/loom", "principles"),
    ("/loom/:role", "principles"),
    ("/model-data", "domains"),
    ("/ontology-views/:domain", "domains"),
    ("/test-run", "tests"),
    ("/werk", "cicd"),
    ("/x", "products"),
    ("/api/chorus/alert", "alerts"),
    ("/api/chorus/embed", "search"),
    ("/api/chorus/index", "search"),
    ("/api/chorus/open", "toolchain"),
    ("/api/chorus/rca", "rcas"),
    ("/api/chorus/reindex", "search"),
    ("/api/chorus/role-state", "roles"),
    ("/api/chorus/spine-event", "spine"),
    ("/api/chorus/voice/:role", "messages"),
    ("/sparql-read", "knowledge"),
];

pub fn endpoint_domain(route_path: &str, domains: &[String]) -> Option<String> {
    let segs: Vec<&str> = route_path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with(':'))
        .collect();
    if let Some(d) = segs.iter().find(|s| domains.iter().any(|d| d == *s)) {
        return Some((*d).to_string());
    }
    if segs.first() == Some(&"api") && segs.get(1) == Some(&"athena") {
        return domains.iter().find(|d| *d == "knowledge").cloned();
    }
    // A route segment that is the SINGULAR of a domain, or its obvious synonym.
    // Measured against the live contract 2026-09-19: these eleven cover the
    // remaining /api/chorus/* surface, and each is the name the API itself uses
    // for a domain the model already has. Nothing here invents a domain.
    const SYNONYM: &[(&str, &str)] = &[
        ("domain", "domains"),
        ("card", "cards"),
        ("test", "tests"),
        ("log", "logs"),
        ("trace", "spine"),
        ("pulse", "spine"),
        ("sessions", "identity"),
        ("health", "monitors"),
        ("nightly", "tests"),
        ("quality", "tests"),
        ("cost", "analytics"),
        ("pain", "rcas"),
        ("hooks", "spine"),
        ("catalog", "knowledge"),
        ("context", "knowledge"),
        ("nudge", "messages"),
    ];
    for s in &segs {
        if let Some((_, d)) = SYNONYM.iter().find(|(k, _)| k == s) {
            if let Some(hit) = domains.iter().find(|x| x == d) {
                return Some(hit.clone());
            }
        }
    }
    // #4222 — the remainder, assigned by hand against the live contract on
    // 2026-09-19. These are the 75 routes no segment rule reaches: their path
    // names a product word (`/api/photos`), a surface (`/clearing`), or nothing
    // at all (`/x`). A guess written down and reviewable beats a row with no
    // domain, and each line is one claim Wren and Silas can accept or correct.
    //
    // The whole route is the key, so a rename does not silently keep the old
    // answer — the route drops out of the table and the run reports it unplaced.
    if let Some((_, d)) = ROUTE.iter().find(|(k, _)| *k == route_path) {
        if let Some(hit) = domains.iter().find(|x| x == d) {
            return Some(hit.clone());
        }
    }
    None
}

/// #4222 — the domain a page renders, read from its own route.
///
/// Same discipline as `endpoint_domain`: the route is the fact, the folder is
/// never consulted. A page whose route names a domain (or the API's word for
/// one) gets it; `/athena/*` is the knowledge surface and `/loom/*` is
/// principles, which is what those two prefixes mean in the product.
/// Anything else stays unplaced and is reported by name.
pub fn page_domain(route: &str, domains: &[String]) -> Option<String> {
    let stem = route
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".html");
    // the page's own name first: /athena/domains.html is about domains
    for word in stem.split('-') {
        if let Some(d) = domains.iter().find(|d| d.as_str() == word) {
            return Some(d.clone());
        }
        let plural = format!("{word}s");
        if let Some(d) = domains.iter().find(|d| d.as_str() == plural) {
            return Some(d.clone());
        }
    }
    let prefix = route.split('/').nth(1).unwrap_or("");
    let surface = match prefix {
        "athena" => "knowledge",
        "loom" => "principles",
        _ => return None,
    };
    domains.iter().find(|d| d.as_str() == surface).cloned()
}

/// #4214 — the undo list for a run: the door names of the rows it CREATED.
///
/// Wren's blocking condition before this writer touches prod: a plan that can
/// only go forward is not a plan. Rolling back means deleting exactly what the
/// run added — never a row it replaced (that row existed before, and deleting
/// it would turn an undo into data loss), and never one it deleted or left
/// alone. So only Post contributes, and the test below proves the other three
/// do not.
pub fn created_names(
    actions: &[RowAction<PageRow>],
    endpoint_actions: &[RowAction<EndpointRow>],
) -> Vec<String> {
    // Each line is "<kind> <name>". The name is bare now (the door mints the
    // prefix), so the undo cannot tell a page from an endpoint by looking at it
    // — it has to be told, and a rollback that guesses is not a rollback.
    let mut out: Vec<String> = Vec::new();
    for a in actions {
        if let RowAction::Post(r) = a {
            out.push(format!("page {}", page_row_name(&r.route)));
        }
    }
    for a in endpoint_actions {
        if let RowAction::Post(r) = a {
            out.push(format!("endpoint {}", endpoint_row_name(&r.http_method, &r.route_path)));
        }
    }
    out
}

/// #4214 — is this served row one THIS writer could have produced?
///
/// The walk only knows rows that come from a source file. athena-make's own
/// generated collections are routes with no source file to read them from —
/// Wren measured 51 of them on 2026-09-19 — and a row for one of those carries
/// no `filePath`. Without this, the first pass after they exist would see them
/// as "not wanted" and delete every one, because absence from MY desired set is
/// not evidence about a row I never author.
pub fn is_ours(row: &InGraph) -> bool {
    row.fields
        .iter()
        .any(|(k, v)| k == "filePath" && !v.trim().is_empty())
}

/// #4214 — should this leg's deletes be refused as a mass delete?
///
/// NOT the shared `mass_delete_refused`, and the difference is the whole point:
/// that guard only engages at 100 rows or more, and there are 63 Page rows live.
/// A leg whose whole population sits under the floor is a leg the guard cannot
/// protect — it would have allowed a werk pass to delete all 63 and reported
/// success. Wren asked what the threshold was on 2026-09-19 and that is how the
/// hole was found. Here the share alone decides, at any size.
/// A share-only guard would be a WALL, not a guard: a 3-row collection retiring
/// 2 pages trips 50% every time and could never finish an ordinary cleanup
/// (Wren, 2026-09-19). So a handful of deletes always proceeds — the shape this
/// refuses is a WIPE, which is a large share AND more than a handful.
pub const SMALL_CLEANUP: usize = 2;

pub fn leg_mass_delete_refused(planned_deletes: usize, rows: usize) -> bool {
    rows > 0 && planned_deletes > SMALL_CLEANUP && planned_deletes * 2 > rows
}

/// The graph these rows live in. Not a default the caller may override: the
/// whole point of the card is that they stop living in the catch-all.
pub const HOME_GRAPH: &str = "urn:chorus:domains:code";

/// A run that cannot place a page or an endpoint says so by name and fails.
/// Jeff, on the CodeFile walk: an unmapped path FAILS the card and names the
/// file. Silence is what let 511 rows sit domainless for months.
pub fn unplaced_report(kind: &str, unplaced: &[String]) -> Option<String> {
    if unplaced.is_empty() {
        return None;
    }
    let mut shown: Vec<&str> = unplaced.iter().map(|s| s.as_str()).collect();
    shown.sort_unstable();
    Some(format!(
        "chorus-crawl: {} {} the rules cannot place, and a default is not an answer: {}",
        unplaced.len(),
        kind,
        shown.join(", ")
    ))
}


#[cfg(test)]
mod desired_tests {
    use super::*;

    fn no_read(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_file_directly_under_a_source_dir_becomes_a_page() {
        let paths = vec!["platform/api/public/athena/product.html".to_string()];
        let (pages, _, _) = desired_rows(&paths, &no_read, false);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].route, "/athena/product.html");
        assert_eq!(pages[0].page_type, "athena");
    }

    #[test]
    fn negative_proof_a_nested_file_is_not_given_a_route() {
        // The violation this selection exists to refuse: a file the route table
        // cannot place getting a route anyway. The guard is route_for's
        // `file_name.contains('/')` refusal — dropping THAT turns this red
        // (proved by mutation 2026-09-19), and `assert_route_for` below pins
        // the same guard at the unit it actually lives in.
        let paths = vec!["platform/api/public/athena/sub/deep.html".to_string()];
        let (pages, _, _) = desired_rows(&paths, &no_read, false);
        assert!(pages.is_empty(), "{pages:?}");
        assert!(route_for(&PAGE_SOURCES[0], "sub/deep.html").is_none());
    }

    #[test]
    fn a_file_outside_every_source_dir_is_not_a_page() {
        let paths = vec!["platform/api/public/other/x.html".to_string()];
        assert!(desired_rows(&paths, &no_read, false).0.is_empty());
    }

    #[test]
    fn a_missing_gathering_checkout_is_reported_not_silent() {
        let (_, _, skipped) = desired_rows(&[], &no_read, false);
        assert_eq!(skipped, vec!["public/gathering-docs"]);
        let (_, _, none_skipped) = desired_rows(&[], &no_read, true);
        assert!(none_skipped.is_empty());
    }

    #[test]
    fn endpoints_are_read_only_from_route_bearing_sources() {
        let paths = vec![
            "platform/api/src/server.ts".to_string(),
            "platform/api/tests/server.test.ts".to_string(),
            "designing/docs/notes.md".to_string(),
        ];
        let read = |p: &str| -> Option<String> {
            Some(match p {
                "platform/api/src/server.ts" => "app.get('/api/chorus/cards/:id', h);",
                _ => "app.get('/should/not/be/read', h);",
            }
            .to_string())
        };
        let (_, endpoints, _) = desired_rows(&paths, &read, false);
        assert_eq!(endpoints.len(), 1, "{endpoints:?}");
        assert_eq!(endpoints[0].route_path, "/api/chorus/cards/:id");
        assert_eq!(endpoints[0].http_method, "GET");
        assert_eq!(endpoints[0].path, "platform/api/src/server.ts");
    }

    #[test]
    fn negative_proof_a_test_file_declaring_routes_is_excluded() {
        // One path per clause, or the assert cannot say which clause held.
        assert!(!serves_routes("platform/api/tests/routes.ts"), "the /tests/ clause");
        assert!(!serves_routes("platform/api/src/server.test.ts"), "the .test.ts clause");
        assert!(!serves_routes("platform/api/src/server.d.ts"));
        assert!(serves_routes("platform/api/src/server.ts"));
    }

    #[test]
    fn negative_proof_a_small_leg_is_still_protected_from_a_wipe() {
        // The state the shared guard CANNOT see: 63 rows is under its floor of
        // 100, so mass_delete_refused(40, 63) is FALSE and all 40 deletes would
        // run. This leg refuses them. Mutating `planned_deletes * 2 > rows` to
        // `false` turns this red (proved 2026-09-19).
        assert!(!crate::mass_delete_refused(40, 63), "the shared guard is blind here");
        assert!(leg_mass_delete_refused(40, 63), "this leg must not be");
        assert!(leg_mass_delete_refused(63, 63), "a full wipe is refused");
    }

    #[test]
    fn an_ordinary_run_of_deletes_is_not_a_wipe() {
        // The control: retiring a handful of pages is normal and must proceed,
        // or the guard becomes a freeze.
        assert!(!leg_mass_delete_refused(3, 63));
        assert!(!leg_mass_delete_refused(0, 0));
        // Wren's case: a tiny collection must still be able to finish a real
        // cleanup. 2 of 3 is 67%, over the share, and it proceeds.
        assert!(!leg_mass_delete_refused(2, 3), "a small cleanup must not be walled");
        assert!(leg_mass_delete_refused(3, 4), "but a wipe of a small set still refuses");
    }


    fn served(name: &str, key: &str, file: Option<&str>) -> InGraph {
        InGraph {
            name: name.to_string(),
            key: key.to_string(),
            domain: None,
            fields: match file {
                Some(f) => vec![("filePath".to_string(), f.to_string())],
                None => vec![],
            },
        }
    }

    #[test]
    fn negative_proof_a_row_this_writer_never_authors_is_not_deleted() {
        // The violation: athena-make's generated routes have no source file, so
        // they can never appear in a desired set built by walking files. Before
        // this guard the plan deleted them. Removing `is_ours` from plan_rows
        // turns this red (proved 2026-09-19).
        let rows = vec![
            served("endpoint-generated", "GET /v1/code/files", None),
            served("endpoint-authored", "GET /api/chorus/cards/:id", Some("platform/api/src/server.ts")),
        ];
        let desired: Vec<EndpointRow> = vec![];
        let plan = plan_rows(&desired, &rows, &|r: &EndpointRow| r.route_path.clone(), &|_| None, false);
        let deleted: Vec<&str> = plan
            .iter()
            .filter_map(|a| match a {
                RowAction::Delete { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deleted, vec!["endpoint-authored"], "only rows from a source file are ours");
    }


    #[test]
    fn the_undo_list_holds_only_what_the_run_created() {
        let made = PageRow { path: "platform/api/public/athena/new.html".into(), route: "/athena/new.html".into(), page_type: "athena".into() };
        let touched = PageRow { path: "platform/api/public/athena/old.html".into(), route: "/athena/old.html".into(), page_type: "athena".into() };
        let plan = vec![
            RowAction::Post(made.clone()),
            RowAction::Replace { name: page_row_name("/athena/old.html"), row: touched },
            RowAction::Unchanged { key: "/athena/same.html".into() },
            RowAction::Delete { name: page_row_name("/athena/gone.html"), key: "/athena/gone.html".into() },
        ];
        let undo = created_names(&plan, &[]);
        assert_eq!(undo, vec![format!("page {}", page_row_name("/athena/new.html"))]);
    }

    #[test]
    fn negative_proof_the_undo_never_names_a_row_that_existed_before() {
        // The violation: an undo that deletes a REPLACED row. That row was in the
        // graph before the run — deleting it is data loss wearing a rollback's
        // name. Widening created_names to include Replace turns this red.
        let touched = PageRow { path: "platform/api/public/athena/old.html".into(), route: "/athena/old.html".into(), page_type: "athena".into() };
        let plan = vec![RowAction::Replace { name: page_row_name("/athena/old.html"), row: touched }];
        assert!(created_names(&plan, &[]).is_empty(), "a replaced row is not ours to delete");
    }

    #[test]
    fn negative_proof_a_row_name_carries_no_type_prefix() {
        // The door mints `page:` / `endpoint:` and REFUSES a name that already
        // has one — 422 double-prefix, caught on the variant. Restoring the
        // prefix to row_name turns this red.
        assert!(!page_row_name("/athena/x.html").starts_with("page-"), "{}", page_row_name("/athena/x.html"));
        assert!(!endpoint_row_name("GET", "/api/x").starts_with("endpoint-"));
        assert!(page_row_name("/athena/x.html").starts_with("athena-x-html-"));
    }

    #[test]
    fn the_undo_covers_endpoints_too_keyed_on_method_and_path() {
        let e = EndpointRow { path: "platform/api/src/server.ts".into(), route_path: "/api/x".into(), http_method: "POST".into() };
        let undo = created_names(&[], &[RowAction::Post(e)]);
        assert_eq!(undo, vec![format!("endpoint {}", endpoint_row_name("POST", "/api/x"))]);
        assert_ne!(endpoint_row_name("POST", "/api/x"), endpoint_row_name("GET", "/api/x"));
    }

    #[test]
    fn one_page_gone_from_the_source_deletes_exactly_that_one_row() {
        // Wren's rehearsal C, as a fixture: a desired set one short. Exactly one
        // row is planned for deletion, and it is the one that left.
        let kept = PageRow { path: "platform/api/public/athena/kept.html".into(), route: "/athena/kept.html".into(), page_type: "athena".into() };
        let rows = vec![
            served(&page_row_name("/athena/kept.html"), "/athena/kept.html", Some("platform/api/public/athena/kept.html")),
            served(&page_row_name("/athena/gone.html"), "/athena/gone.html", Some("platform/api/public/athena/gone.html")),
        ];
        let plan = plan_rows(&[kept], &rows, &|r: &PageRow| r.route.clone(), &|_| None, false);
        let c = counts(&plan);
        assert_eq!((c.deleted, c.posted), (1, 0), "{plan:?}");
        assert!(matches!(&plan[..], [_, RowAction::Delete { key, .. }] if key == "/athena/gone.html"));
        // and that single delete is a cleanup, not a wipe, so it proceeds
        assert!(!leg_mass_delete_refused(1, 2));
    }


    #[test]
    fn negative_proof_one_route_declared_twice_is_one_row() {
        // The door refused a real batch over this on 2026-09-19: two files
        // declaring the same route produced two rows whose door NAME is
        // identical, and a batch cannot carry the same name twice. Removing the
        // dedup_by turns this red.
        let paths = vec!["a/one.ts".to_string(), "a/two.ts".to_string()];
        let read = |_: &str| Some("app.get('/api/same', h);".to_string());
        let (_, endpoints, _) = desired_rows(&paths, &read, false);
        assert_eq!(endpoints.len(), 1, "{endpoints:?}");
        assert_eq!(endpoints[0].path, "a/one.ts", "first file wins, deterministically");
        let names: std::collections::BTreeSet<String> =
            endpoints.iter().map(|e| endpoint_row_name(&e.http_method, &e.route_path)).collect();
        assert_eq!(names.len(), endpoints.len(), "every row name is unique");
    }

    #[test]
    fn different_methods_on_one_path_stay_two_rows() {
        let paths = vec!["a/one.ts".to_string()];
        let read = |_: &str| Some("app.get('/api/x', h); app.post('/api/x', h);".to_string());
        let (_, endpoints, _) = desired_rows(&paths, &read, false);
        assert_eq!(endpoints.len(), 2, "{endpoints:?}");
    }


    fn doms() -> Vec<String> {
        ["cards", "knowledge", "code", "logs"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_endpoint_is_placed_by_the_route_it_serves() {
        assert_eq!(endpoint_domain("/api/chorus/cards/:id", &doms()).as_deref(), Some("cards"));
        assert_eq!(endpoint_domain("/api/athena/class-atlas", &doms()).as_deref(), Some("knowledge"));
    }

    #[test]
    fn negative_proof_a_route_naming_no_domain_stays_unplaced() {
        // The guarded condition: a route that names nothing must NOT get a
        // default. Falling back to the first domain, or to the folder, is the
        // #4201 failure this rule exists to avoid. Returning Some(..) here
        // turns this red.
        assert_eq!(endpoint_domain("/", &doms()), None);
        assert_eq!(endpoint_domain("/api/proxy/thing", &doms()), None);
    }

    #[test]
    fn a_route_segment_that_is_the_api_name_for_a_domain_places_it() {
        let d: Vec<String> = ["spine", "domains", "knowledge"].iter().map(|s| s.to_string()).collect();
        assert_eq!(endpoint_domain("/api/chorus/trace/:id", &d).as_deref(), Some("spine"));
        assert_eq!(endpoint_domain("/api/chorus/domain/:name", &d).as_deref(), Some("domains"));
    }

    #[test]
    fn a_domain_absent_from_the_model_is_never_invented() {
        // "cards" is real, "widgets" is not — the rule can only name a domain
        // the model already has.
        assert_eq!(endpoint_domain("/api/chorus/widgets", &doms()), None);
    }


    #[test]
    fn a_page_is_placed_by_its_own_name_then_its_surface() {
        let d: Vec<String> = ["domains", "knowledge", "principles", "value-streams"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(page_domain("/athena/domains.html", &d).as_deref(), Some("domains"));
        assert_eq!(page_domain("/athena/class-atlas.html", &d).as_deref(), Some("knowledge"));
        assert_eq!(page_domain("/loom/cookbook.html", &d).as_deref(), Some("principles"));
    }

    #[test]
    fn negative_proof_a_page_outside_a_known_surface_stays_unplaced() {
        // No default, no folder read. A page we cannot place is reported, the
        // same rule the endpoint side follows.
        let d: Vec<String> = ["domains", "knowledge"].iter().map(|s| s.to_string()).collect();
        assert_eq!(page_domain("/something/else.html", &d), None);
    }

}

#[cfg(test)]
mod pages_4214 {
    use super::*;

    fn src(dir: &'static str) -> &'static PageSource {
        PAGE_SOURCES.iter().find(|s| s.dir == dir).unwrap()
    }

    #[test]
    fn a_page_route_is_read_from_the_path() {
        let athena = src("platform/api/public/athena");
        assert_eq!(
            route_for(athena, "product.html").as_deref(),
            Some("/athena/product.html")
        );
        let docs = src("public/gathering-docs");
        assert_eq!(
            route_for(docs, "domain-music.html").as_deref(),
            Some("/gathering-docs/domain-music.html")
        );
    }

    // NEGATIVE PROOF (#3734): the route table states routes for files sitting
    // DIRECTLY in a source dir. A nested file is served at a route this table
    // cannot state, so it must answer nothing rather than invent
    // `/athena/sub/x.html` — and a file of the wrong extension is not a page.
    #[test]
    fn negative_proof_a_nested_or_wrong_extension_file_gets_no_route() {
        let athena = src("platform/api/public/athena");
        assert_eq!(route_for(athena, "sub/product.html"), None);
        assert_eq!(route_for(athena, "product.js"), None);
        assert_eq!(route_for(athena, ".html"), None);
    }

    #[test]
    fn pages_in_carries_the_repo_path_and_the_type() {
        let rows = pages_in(
            src("platform/api/public/loom"),
            &["principles.html".to_string(), "README.md".to_string()],
        );
        assert_eq!(rows.len(), 1, "only the .html file is a page");
        assert_eq!(rows[0].path, "platform/api/public/loom/principles.html");
        assert_eq!(rows[0].route, "/loom/principles.html");
        assert_eq!(rows[0].page_type, "loom");
    }

    #[test]
    fn endpoints_are_read_off_the_declaration() {
        let src = r#"
            app.get('/api/chorus/cards', h);
            app.post("/api/chorus/cards", h);
            app.put(`/api/chorus/cards/:id`, h);
        "#;
        let rows = endpoints_in("platform/api/src/server.ts", src);
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .any(|r| r.http_method == "PUT" && r.route_path == "/api/chorus/cards/:id"));
        assert!(rows.iter().all(|r| r.path == "platform/api/src/server.ts"));
    }

    // NEGATIVE PROOF: the TypeScript scanner dropped every route containing a
    // colon, which is why the served contract was 448 rows and not the whole
    // surface. If that skip comes back, this fails.
    #[test]
    fn negative_proof_a_parameterised_route_is_part_of_the_contract() {
        let rows = endpoints_in("s.ts", "app.get('/api/chorus/cards/:id', h);");
        assert_eq!(rows.len(), 1, "a :id route is a route the API serves");
        assert_eq!(rows[0].route_path, "/api/chorus/cards/:id");
    }

    // NEGATIVE PROOF: `app.` appears in prose and in unrelated calls. A match
    // that is not a method + quoted absolute path must yield nothing, or the
    // fold fills with noise nobody can act on.
    #[test]
    fn negative_proof_app_dot_something_else_is_not_an_endpoint() {
        assert!(endpoints_in("s.ts", "app.listen(3340);").is_empty());
        assert!(endpoints_in("s.ts", "app.use(express.json());").is_empty());
        assert!(
            endpoints_in("s.ts", "app.get(routeVar, h);").is_empty(),
            "an unquoted route is not a literal we can record"
        );
        assert!(
            endpoints_in("s.ts", "app.get('relative/path', h);").is_empty(),
            "a served route starts at the root"
        );
    }

    // NEGATIVE PROOF (the collision that refused the first live CodeFile
    // batch): two keys that slug to the same string must not share a row.
    #[test]
    fn negative_proof_two_keys_that_slug_alike_do_not_share_a_row() {
        assert_ne!(
            page_row_name("/athena/LOG_RELATEDNESS.html"),
            page_row_name("/athena/log-relatedness.html")
        );
        assert_ne!(
            endpoint_row_name("GET", "/api/x"),
            endpoint_row_name("POST", "/api/x"),
            "method is part of the key — two entries in the contract"
        );
    }

    #[test]
    fn the_same_key_answers_the_same_name_every_run() {
        assert_eq!(page_row_name("/athena/product.html"), page_row_name("/athena/product.html"));
        // bare, no type prefix — the door mints that (#4214, 422 double-prefix)
        assert!(page_row_name("/athena/product.html").starts_with("athena-product-html-"));
    }

    #[test]
    fn a_second_pass_over_an_unchanged_tree_writes_nothing() {
        let rows = vec![PageRow {
            path: "platform/api/public/athena/product.html".into(),
            route: "/athena/product.html".into(),
            page_type: "athena".into(),
        }];
        let graph = vec![InGraph { name: page_row_name("/athena/product.html"), key: "/athena/product.html".into(), domain: Some("athena".into()), fields: vec![] }];
        let actions = plan_rows(
            &rows,
            &graph,
            &|r: &PageRow| r.route.clone(),
            &|_| Some("athena".to_string()),
            false,
        );
        assert_eq!(
            counts(&actions),
            Counts { unchanged: 1, ..Default::default() },
            "idempotent: the same page twice is one row"
        );
    }

    // NEGATIVE PROOF (#4201's lesson): a row whose stored domain no longer
    // matches what the rules say is STALE, even with an unchanged key. Planning
    // on the key alone would leave every domainless row blank forever — which
    // is the exact state these 511 rows are in today.
    #[test]
    fn negative_proof_a_row_whose_domain_moved_is_replaced_not_left_alone() {
        let rows = vec![PageRow {
            path: "platform/api/public/athena/product.html".into(),
            route: "/athena/product.html".into(),
            page_type: "athena".into(),
        }];
        let graph = vec![InGraph { name: page_row_name("/athena/product.html"), key: "/athena/product.html".into(), domain: None, fields: vec![] }];
        let actions = plan_rows(
            &rows,
            &graph,
            &|r: &PageRow| r.route.clone(),
            &|_| Some("athena".to_string()),
            false,
        );
        assert_eq!(counts(&actions).replaced, 1);
        assert_eq!(counts(&actions).unchanged, 0);
    }

    #[test]
    fn a_page_that_left_the_source_stops_being_served() {
        // A served page row carries the filePath it came from — that is what
        // marks it as this writer's to retire (#4214 is_ours).
        let graph = vec![InGraph {
            name: page_row_name("/athena/gone.html"),
            key: "/athena/gone.html".into(),
            domain: Some("athena".into()),
            fields: vec![("filePath".into(), "platform/api/public/athena/gone.html".into())],
        }];
        let actions: Vec<RowAction<PageRow>> =
            plan_rows(&[], &graph, &|r: &PageRow| r.route.clone(), &|_| None, false);
        assert_eq!(counts(&actions).deleted, 1);
    }

    // A row the door cannot address by name is never planned against — the
    // guard that kept the log leg from POSTing over urn: ids.
    #[test]
    fn negative_proof_an_unaddressable_row_is_left_alone() {
        assert!(!door_addressable("urn:chorus:page-x"));
        assert!(!door_addressable(""));
        assert!(door_addressable("page-athena-product-html-1a2b3c4d"));
        let graph = vec![InGraph { name: "urn:chorus:page-x".into(), key: "/athena/x.html".into(), domain: None, fields: vec![] }];
        let actions: Vec<RowAction<PageRow>> =
            plan_rows(&[], &graph, &|r: &PageRow| r.route.clone(), &|_| None, false);
        assert!(actions.is_empty(), "never address a row the door did not mint");
    }

    #[test]
    fn a_template_route_is_read_from_the_handler_that_renders_it() {
        let router = r#"
            app.get('/music', (req, res) => res.render('collection-music'));
            app.get('/music/:slug', (req, res) => { res.render('music-detail'); });
        "#;
        let got = rendered_routes(router);
        assert!(got.contains(&("collection-music".to_string(), "/music".to_string())));
        assert!(got.contains(&("music-detail".to_string(), "/music/:slug".to_string())));
    }

    // NEGATIVE PROOF (#3734): the retired rule. `collection-music.ejs` became
    // `/music` because of the word "collection" in its NAME, and `foo-detail`
    // became `/foo/:slug` the same way. A template no handler renders must
    // answer NOTHING now — if a name-derived route comes back, this fails.
    #[test]
    fn negative_proof_a_template_nobody_renders_gets_no_route_from_its_name() {
        assert!(
            rendered_routes("// collection-music.ejs exists on disk").is_empty(),
            "a filename is not a route declaration"
        );
        // rendered, but by no handler: still nothing to state
        assert!(rendered_routes("res.render('collection-music');").is_empty());
        // and a computed view name is not a literal we can record
        assert!(rendered_routes("app.get('/x', (q,s) => s.render(`${v}-detail`));").is_empty());
    }

    // The pairing must take the handler the render sits INSIDE, not the first
    // or last route in the file — the two states a "nearest declaration" rule
    // could confuse.
    #[test]
    fn negative_proof_a_render_pairs_with_its_own_handler_not_a_neighbour() {
        let router = r#"
            app.get('/first', (q, s) => s.render('first-view'));
            app.get('/second', (q, s) => s.render('second-view'));
        "#;
        let got = rendered_routes(router);
        assert!(got.contains(&("second-view".to_string(), "/second".to_string())));
        assert!(
            !got.contains(&("second-view".to_string(), "/first".to_string())),
            "the second render belongs to the second route"
        );
    }

    #[test]
    fn an_unplaceable_page_is_named_not_defaulted() {
        assert_eq!(unplaced_report("pages", &[]), None);
        let msg = unplaced_report("pages", &["/athena/x.html".to_string()]).unwrap();
        assert!(msg.contains("/athena/x.html"));
        assert!(msg.contains("a default is not an answer"));
    }

    // The home graph is the card's whole point: one graph per class family, the
    // same as code, tests and logs. If this becomes the catch-all again, fail.
    #[test]
    fn negative_proof_the_home_graph_is_not_the_catch_all() {
        assert_eq!(HOME_GRAPH, "urn:chorus:domains:code");
        assert_ne!(HOME_GRAPH, "urn:chorus:instances");
        assert_ne!(HOME_GRAPH, "urn:chorus:ontology");
    }
    /// #4222 — every route the hand table claims, placed. The fixture is the 74
    /// routes that carried no domain on 2026-09-19; if one is renamed the table
    /// no longer matches it and this goes red rather than keeping a stale answer.
    #[test]
    fn every_hand_assigned_route_places() {
        let domains = all_domains();
        for (route, want) in super::ROUTE {
            assert_eq!(
                endpoint_domain(route, &domains).as_deref(),
                Some(*want),
                "{route} must place in {want}"
            );
        }
    }

    /// NEGATIVE PROOF. A route no rule reaches stays unplaced and is reported by
    /// name. Give the table a catch-all and this goes green when it must not.
    #[test]
    fn a_route_no_rule_reaches_stays_unplaced() {
        let domains = all_domains();
        assert_eq!(endpoint_domain("/api/zzz/not-a-thing", &domains), None);
        // and a near-miss of a real entry is NOT the entry
        assert_eq!(endpoint_domain("/api/photos/nope", &domains), None);
    }

    fn all_domains() -> Vec<String> {
        ["alerts", "analytics", "cards", "cicd", "code", "domains", "infrastructure", "integrations", "knowledge", "memory", "messages", "metrics", "principles", "products", "rcas", "roles", "search", "security", "services", "spine", "tests", "toolchain", "value-streams"].iter().map(|s| s.to_string()).collect()
    }


}
