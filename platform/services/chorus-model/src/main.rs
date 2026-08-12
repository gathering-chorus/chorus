//! chorus-model CLI — the governed writer's command surface (#3257).
//!
//!   chorus-model add --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]
//!   chorus-model mint --kind <kind> --name <name>
//!   chorus-model kinds
//!
//! Callers never pass IRIs — fields are literals, edges are (property, kind:name)
//! pairs the mint resolves. --dry-run prints the Turtle and writes nothing.

use athena_model::{add_edge, batch, delete_entity, delete_iri, mint, parse_ntriples, remove_edge, seed_multi, SeedGroup, set_field, to_turtle, write, FusekiStore, Identity, Store, WriteReq};
use std::process::ExitCode;

fn usage() -> String {
    "athena-model — the governed RDF/OWL writer (ADR-040 Rule 0; #3257)\n\
     usage:\n\
       chorus-model add    --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]\n\
       chorus-model delete --kind <kind> --name <name>\n\
       chorus-model set    --kind <kind> --name <name> --field k=v [--graph <g>]\n\
       chorus-model link   --kind <kind> --name <name> --edge prop=kind:name\n\
       chorus-model unlink --kind <kind> --name <name> --edge prop=kind:name\n\
       chorus-model seed   (--kind <kind> --ttl <file>)... [--graph <g>] [--provenance migrated] [--base <iri>]\n\
                    --kind/--ttl repeat as pairs; several kinds load as ONE transaction (#3839)\n\
       chorus-model mint   --kind <kind> --name <name>\n\
       chorus-model kinds"
        .to_string()
}

fn parse_req(args: &[String]) -> Result<(WriteReq, bool), String> {
    let mut req = WriteReq::default();
    let mut dry = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                req.kind = args.get(i + 1).ok_or("--kind needs a value")?.clone();
                i += 2;
            }
            "--name" => {
                req.name = args.get(i + 1).ok_or("--name needs a value")?.clone();
                i += 2;
            }
            "--field" => {
                let kv = args.get(i + 1).ok_or("--field needs k=v")?;
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("--field '{}' is not k=v", kv))?;
                req.fields.insert(k.to_string(), v.to_string());
                i += 2;
            }
            "--edge" => {
                let spec = args.get(i + 1).ok_or("--edge needs prop=kind:name")?;
                let (prop, target) = spec.split_once('=').ok_or_else(|| format!("--edge '{}' is not prop=kind:name", spec))?;
                let (tkind, tname) = target.split_once(':').ok_or_else(|| format!("--edge target '{}' is not kind:name", target))?;
                req.edges.push((prop.to_string(), tkind.to_string(), tname.to_string()));
                i += 2;
            }
            "--graph" => {
                // #3647 — the class's model-declared instance home (owl-api resolves + passes it).
                req.graph = Some(args.get(i + 1).ok_or("--graph needs a value")?.clone());
                i += 2;
            }
            "--dry-run" => {
                dry = true;
                i += 1;
            }
            other => return Err(format!("unknown arg '{}'\n{}", other, usage())),
        }
    }
    if req.kind.is_empty() || req.name.is_empty() {
        return Err(format!("--kind and --name are required\n{}", usage()));
    }
    Ok((req, dry))
}

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // #3718 — SCHEMA VERBS. The instance verbs (mint/add) have governed the
        // ABox since #3257; the TBox — classes, properties, shapes — has been
        // hand-typed into .ttl, which is where every failure of 2026-07-30/31
        // came from: 186 classes over 13 files and 6 namespaces, 69 binding
        // nothing, 11 properties with no domain/range, a shape and an ontology
        // disagreeing about what a Domain is.
        //
        // These verbs DESCRIBE the write, run the ADR checks, and print what
        // would land. They do not write yet — the placement gate arms after the
        // definesVocabulary backfill (Silas). Describing before writing is
        // deliberate: it makes the refusals reviewable while the model catches up.
        Some("check") => {
            let rest = &args[1..];
            let get = |flag: &str| -> Option<String> {
                rest.iter().position(|a| a == flag).and_then(|i| rest.get(i + 1)).cloned()
            };
            let class = get("--class").ok_or("check: --class <ClassLocal> is required")?;
            let file = get("--file").unwrap_or_default();
            let is_def = rest.iter().any(|a| a == "--definition");
            let declared = get("--graph");
            let reason = get("--reason");
            let claimed = get("--claimed-by");
            let required: Vec<String> = get("--required")
                .map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect())
                .unwrap_or_default();
            let supplied: std::collections::BTreeSet<String> = get("--supplied")
                .map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect())
                .unwrap_or_default();
            let deploy_set: std::collections::BTreeSet<String> =
                athena_model::adr::deploy_set_from_script();

            let layer = athena_model::adr::layer_of(&class, is_def);
            let facts = athena_model::adr::WriteFacts {
                layer,
                kind: &get("--kind").unwrap_or_else(|| "class".into()),
                class_local: &class,
                target_file: &file,
                declared_types: &[],
                defining_domain: claimed.as_deref(),
                declared_placement: declared.as_deref(),
                override_reason: reason.as_deref(),
                has_shape: !rest.iter().any(|a| a == "--no-shape"),
                required: &required,
                supplied: &supplied,
                deploy_set: &deploy_set,
            };
            let refusals = athena_model::adr::check_all(&facts);
            if refusals.is_empty() {
                // Print the graph that governs FOR THIS LAYER. Showing the
                // instance derivation on a Schema-layer check would tell the
                // caller the wrong home — the exact class of quiet wrongness
                // this whole card exists to remove.
                let placement = match layer {
                    athena_model::adr::Layer::Schema =>
                        athena_model::adr::SCHEMA_GRAPH.to_string(),
                    athena_model::adr::Layer::Instance =>
                        athena_model::adr::derived_placement(&class, claimed.as_deref())
                            .unwrap_or_else(|| "(underivable)".into()),
                };
                return Ok(format!(
                    "OK — {} passes every decidable ADR check.\n  layer:     {:?}\n  placement: {}  (derived)",
                    class, layer, placement
                ));
            }
            // ALL refusals, never just the first — one-problem-per-attempt makes
            // the human iterate, which is the opposite of legibility.
            let body = refusals
                .iter()
                .map(|r| format!("  [{}] {}", r.code(), r.message()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!("REFUSED — {} ADR violation(s) for {}:\n{}", refusals.len(), class, body))
        }
        // #3718 — the TBox verbs. Same refusal-first shape as the instance verbs:
        // check EVERYTHING, refuse with every violation named, and only then
        // emit. Nothing is defaulted — Jeff's legibility ruling is enforced by
        // tbox::check_*, which has no default branch to fall through to.
        //
        // These EMIT turtle for a manifest file rather than POSTing to the
        // store. The schema graph is DBA-path-only by design (#3356), so a
        // writer that wrote schema straight to Fuseki would be bypassing the
        // very governance this card exists to add. The deploy is still the
        // deploy; what changes is that what it deploys was minted, not typed.
        Some(v @ ("class" | "property" | "shape")) => {
            let rest = &args[1..];
            let get = |flag: &str| -> Option<String> {
                rest.iter().position(|a| a == flag).and_then(|i| rest.get(i + 1)).cloned()
            };
            let file = get("--file").unwrap_or_default();
            let deploy_set = athena_model::adr::deploy_set_from_script();
            let dry = rest.iter().any(|a| a == "--dry-run");

            let (refusals, turtle) = match v {
                "class" => {
                    let name = get("--name").ok_or("class: --name <ClassLocal> is required")?;
                    let comment = get("--comment");
                    let claimed = get("--claimed-by");
                    let spec = athena_model::tbox::ClassSpec {
                        name: &name,
                        comment: comment.as_deref(),
                        claimed_by: claimed.as_deref(),
                        target_file: &file,
                    };
                    let r = athena_model::tbox::check_class(&spec, &deploy_set);
                    let ttl = if r.is_empty() { athena_model::tbox::class_turtle(&spec) } else { String::new() };
                    (r, ttl)
                }
                "property" => {
                    let name = get("--name").ok_or("property: --name <localName> is required")?;
                    let dom = get("--domain");
                    let rng = get("--range");
                    let comment = get("--comment");
                    let spec = athena_model::tbox::PropertySpec {
                        name: &name,
                        domain: dom.as_deref(),
                        range: rng.as_deref(),
                        comment: comment.as_deref(),
                        target_file: &file,
                    };
                    let r = athena_model::tbox::check_property(&spec, &deploy_set);
                    let ttl = if r.is_empty() { athena_model::tbox::property_turtle(&spec) } else { String::new() };
                    (r, ttl)
                }
                _ => {
                    let class = get("--class").ok_or("shape: --class <ClassLocal> is required")?;
                    let required: Vec<String> = get("--required")
                        .map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect())
                        .unwrap_or_default();
                    let ig = get("--instances-graph");
                    let spec = athena_model::tbox::ShapeSpec {
                        class: &class, required: &required, target_file: &file,
                        instances_graph: ig.as_deref(),
                    };
                    let r = athena_model::tbox::check_shape(&spec, &deploy_set);
                    let ttl = if r.is_empty() { athena_model::tbox::shape_turtle(&spec) } else { String::new() };
                    (r, ttl)
                }
            };

            if !refusals.is_empty() {
                let body = refusals.iter()
                    .map(|r| format!("  [{}] {}", r.code(), r))
                    .collect::<Vec<_>>().join("
");
                return Err(format!("REFUSED — {} violation(s):
{}", refusals.len(), body));
            }
            if dry {
                return Ok(format!("# dry-run — nothing written
{}", turtle));
            }
            // APPEND, never rewrite. A governed writer that rewrites a
            // hand-curated file would destroy comments and ordering that carry
            // real reasoning — and silently, which is the failure mode we spent
            // the week removing.
            use std::io::Write;
            let path = format!("{}/{}", std::env::var("CHORUS_ROOT").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string()), file);
            let mut f = std::fs::OpenOptions::new().append(true).open(&path)
                .map_err(|e| format!("cannot append to {}: {}", path, e))?;
            write!(f, "
{}", turtle).map_err(|e| format!("write failed: {}", e))?;
            Ok(format!("appended to {}:
{}", file, turtle))
        }
        // #3752 — the FOURTH TBox verb: retirement. Shares the mint verbs'
        // refusal-first machinery, DIVERGES at the write boundary: mint ends in
        // append-to-file, retire ends in emit-to-STAGING (designing/schemas/
        // model-retirements.jsonl) which chorus-model-deploy's retirement
        // section executes. NEVER a direct store write — the 2026-08-04
        // events-relic bounded-exception is the incident this verb closes.
        Some("retire-claim") => {
            let rest = &args[1..];
            let get = |flag: &str| -> Option<String> {
                rest.iter().position(|a| a == flag).and_then(|i| rest.get(i + 1)).cloned()
            };
            let domain = get("--domain").ok_or("retire-claim: --domain <domainLocal> is required")?;
            let class = get("--class").ok_or("retire-claim: --class <ClassLocal> is required")?;
            let reason = get("--reason").unwrap_or_default();
            let card = get("--card").ok_or("retire-claim: --card <id> is required — a retirement without a card is an unrecorded judgment")?;
            let dry = rest.iter().any(|a| a == "--dry-run");

            let store = FusekiStore::new();
            // Identity FIRST, fail closed — the staging entry records WHO
            // decided, from the verified token, never env (#3651/#3687).
            let by = if dry {
                "dry-run".to_string()
            } else {
                Identity::resolve(&store)?.role().to_string()
            };

            // Live truth for the checks: claims from the STORE, serving from
            // owl-api's route table — never a file (DECLARED⊃CLAIMED⊃SERVED).
            let live_claims = fetch_live_claims(&store)?;
            let served = fetch_served_routes();
            let spec = athena_model::tbox::RetireSpec {
                domain: &domain, class: &class, reason: &reason,
            };
            let refusals = athena_model::tbox::check_retire(&spec, &live_claims, &served);
            if !refusals.is_empty() {
                let body = refusals.iter()
                    .map(|r| format!("  [{}] {}", r.code(), r))
                    .collect::<Vec<_>>().join("\n");
                return Err(format!("REFUSED — {} violation(s):\n{}", refusals.len(), body));
            }
            let date = today_utc();
            let entry = athena_model::tbox::retirement_entry(&spec, &by, &card, &date);
            if dry {
                return Ok(format!("# dry-run — nothing staged\n{}", entry));
            }
            use std::io::Write;
            let path = format!(
                "{}/designing/schemas/model-retirements.jsonl",
                std::env::var("CHORUS_ROOT").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
            );
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)
                .map_err(|e| format!("cannot append to {}: {}", path, e))?;
            writeln!(f, "{}", entry).map_err(|e| format!("write failed: {}", e))?;
            Ok(format!(
                "staged retirement (executes on next model deploy):\n{}\n→ {}",
                entry, path
            ))
        }
        Some("kinds") => Ok("product domain role value-stream value-stream-step service principle practice policy skill gate decision document".into()),
        Some("mint") => {
            let (req, _) = parse_req(&args[1..])?;
            mint(&req.kind, &req.name)
        }
        Some("add") => {
            let (req, dry) = parse_req(&args[1..])?;
            if dry {
                let (subject, turtle) = to_turtle(&req)?;
                Ok(format!("# dry-run — nothing written\n# subject: {}\n{}", subject, turtle))
            } else {
                let store = FusekiStore::new();
                // #3651 — the identity gate: no verified Principal, no write.
                let id = Identity::resolve(&store)?;
                let subject = write(&store, &req, &id)?;
                Ok(format!("written: {}", subject))
            }
        }
        // #3468 — delete / link / unlink: the governed verbs owl-api delegates to,
        // so every entity-delete and edge-mutation rides ONE audited write path.
        Some("delete") => {
            // #3392 — by-IRI form for foreign-realm subjects (no chorus
            // (kind,name) to address): delete --iri <iri> --graph <g>.
            if args[1..].iter().any(|a| a == "--iri") {
                let rest = &args[1..];
                let mut iri = String::new();
                let mut graph = String::new();
                let mut i = 0;
                while i < rest.len() {
                    match rest[i].as_str() {
                        "--iri" => { i += 1; iri = rest.get(i).cloned().unwrap_or_default(); }
                        "--graph" => { i += 1; graph = rest.get(i).cloned().unwrap_or_default(); }
                        other => return Err(format!("delete --iri: unknown arg '{}'", other)),
                    }
                    i += 1;
                }
                if iri.is_empty() || graph.is_empty() {
                    return Err("delete --iri needs --iri <iri> and --graph <g>".into());
                }
                let store = FusekiStore::new();
                let id = Identity::resolve(&store)?; // #3651
                return Ok(format!("deleted: {}", delete_iri(&store, &iri, &graph, &id)?));
            }
            let (req, _) = parse_req(&args[1..])?;
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651
            Ok(format!("deleted: {}", delete_entity(&store, &req.kind, &req.name, req.graph.as_deref(), &id)?))
        }
        // #3692 — seed: bulk TTL ingest (the 5th DAL verb). Pre-minted IRIs
        // preserved, SHACL-validated fail-closed, provenance-stamped, idempotent.
        // TTL is normalized to N-Triples via riot (the same toolchain
        // model-deploy validates with) — fail-loud if riot is absent.
        Some("seed") => {
            // #3839 — --kind/--ttl may repeat. Each pair is one GROUP: the kind
            // the caller states that file is, validated against that class's
            // shape. Several groups load as ONE transaction, so kinds that
            // reference each other (a stream contains its steps, each step is
            // inStream its stream) can arrive together — two independent runs
            // could never bootstrap that from an empty store, whichever ran
            // first would refuse on targets that exist nowhere yet.
            let mut pairs: Vec<(String, String)> = Vec::new();
            let mut graph: Option<String> = None;
            let mut provenance = "migrated".to_string();
            let mut base: Option<String> = None;
            let rest = &args[1..];
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--kind" => {
                        i += 1;
                        pairs.push((rest.get(i).cloned().unwrap_or_default(), String::new()));
                    }
                    "--ttl" => {
                        i += 1;
                        let v = rest.get(i).cloned().unwrap_or_default();
                        match pairs.last_mut() {
                            // Attach to the kind it follows. A --ttl with no
                            // --kind before it has no stated class, and a file
                            // whose class nobody stated is precisely what this
                            // door exists to refuse.
                            Some(last) if last.1.is_empty() => last.1 = v,
                            _ => return Err("seed: --ttl must follow its own --kind (one --kind per --ttl)".into()),
                        }
                    }
                    "--graph" => { i += 1; graph = rest.get(i).cloned(); }
                    "--provenance" => { i += 1; provenance = rest.get(i).cloned().unwrap_or_default(); }
                    // #3392 — resolve relative TTL IRIs against this base so a
                    // migration reproduces the live store's IRIs byte-identically
                    // (the gathering ICD set resolved against https://jeffbridwell.com/).
                    "--base" => { i += 1; base = rest.get(i).cloned(); }
                    other => return Err(format!("seed: unknown arg '{}'\n{}", other, usage())),
                }
                i += 1;
            }
            if pairs.is_empty() || pairs.iter().any(|(k, t)| k.is_empty() || t.is_empty()) {
                return Err(format!("seed needs --kind <K> and --ttl <file> (repeatable, paired)\n{}", usage()));
            }
            let mut parsed: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
            for (kind, ttl_path) in &pairs {
                let nt = if ttl_path.ends_with(".nt") {
                    std::fs::read_to_string(ttl_path).map_err(|e| format!("seed: read {}: {}", ttl_path, e))?
                } else {
                    let mut riot_args: Vec<String> = vec!["--output=ntriples".to_string()];
                    if let Some(b) = &base {
                        riot_args.push(format!("--base={}", b));
                    }
                    riot_args.push(ttl_path.clone());
                    let out = std::process::Command::new("riot")
                        .args(&riot_args)
                        .output()
                        .map_err(|e| format!("seed: riot not runnable ({}) — TTL→N-Triples needs riot on PATH", e))?;
                    if !out.status.success() {
                        return Err(format!(
                            "seed: riot failed on {} — fix the TTL first:\n{}",
                            ttl_path,
                            String::from_utf8_lossy(&out.stderr)
                        ));
                    }
                    String::from_utf8_lossy(&out.stdout).to_string()
                };
                parsed.push((kind.clone(), parse_ntriples(&nt)?));
            }
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651 — same gate as every verb
            let groups: Vec<SeedGroup> = parsed
                .iter()
                .map(|(k, t)| SeedGroup { kind: k.as_str(), triples: t.as_slice() })
                .collect();
            let kind = pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join("+");
            let report = seed_multi(&store, &groups, &provenance, graph.as_deref(), &id)?;
            Ok(format!(
                "seeded: {} subjects / {} triples (kind={}, provenance={})",
                report.subjects, report.triples, kind, provenance
            ))
        }
        // #3686 — set: field-level single-predicate update, the datatype-prop
        // sibling of link/unlink. Exactly ONE --field k=v; edges stay link/unlink.
        Some("set") => {
            let (req, _) = parse_req(&args[1..])?;
            if req.fields.len() != 1 || !req.edges.is_empty() {
                return Err("set needs exactly one --field k=v (and no --edge — edges are link/unlink)".into());
            }
            let (prop, value) = req.fields.iter().next().map(|(k, v)| (k.clone(), v.clone())).unwrap();
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651
            let subject = set_field(&store, &req.kind, &req.name, &prop, &value, req.graph.as_deref(), &id)?;
            Ok(format!("set: {} {}={}", subject, prop, value))
        }
        Some(verb @ ("link" | "unlink")) => {
            let (req, _) = parse_req(&args[1..])?;
            let (prop, tkind, tname) = req
                .edges
                .first()
                .ok_or(format!("{} needs --edge prop=kind:name", verb))?;
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651
            let subject = if verb == "link" {
                add_edge(&store, &req.kind, &req.name, prop, tkind, tname, req.graph.as_deref(), &id)?
            } else {
                remove_edge(&store, &req.kind, &req.name, prop, tkind, tname, req.graph.as_deref(), &id)?
            };
            Ok(format!("{}: {} {} {}:{}", verb, subject, prop, tkind, tname))
        }
        // #3573 — governed BATCH: the migration target owl-api's /batch delegates to.
        // Typed slots only (no writer SPARQL text), structural single-graph, empty/
        // off-realm graph refused. Args: batch --graph <g> [--del S P O]... [--ins S P O]...
        // where S/P/O are already-serialized terms (<iri> or "literal").
        Some("batch") => {
            let mut graph = String::new();
            let mut deletes: Vec<(String, String, String)> = Vec::new();
            let mut inserts: Vec<(String, String, String)> = Vec::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--graph" => {
                        graph = args.get(i + 1).ok_or("--graph needs a value")?.clone();
                        i += 2;
                    }
                    "--del" | "--ins" => {
                        let s = args.get(i + 1).ok_or("--del/--ins needs S P O")?.clone();
                        let p = args.get(i + 2).ok_or("--del/--ins needs S P O")?.clone();
                        let o = args.get(i + 3).ok_or("--del/--ins needs S P O")?.clone();
                        if args[i] == "--del" { deletes.push((s, p, o)); } else { inserts.push((s, p, o)); }
                        i += 4;
                    }
                    other => return Err(format!("batch: unknown arg '{}'\n{}", other, usage())),
                }
            }
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651
            let n = batch(&store, &graph, &deletes, &inserts, &id)?;
            Ok(format!("batch: {} triple(s) applied to <{}>", n, graph))
        }
        _ => Err(usage()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(out) => {
            println!("{}", out);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("athena-model: {}", e);
            ExitCode::FAILURE
        }
    }
}

// #3752 — impure fetchers for retire-claim's checks. Live truth only:
// claims from the STORE, serving from owl-api's own route table.
fn fetch_live_claims(store: &FusekiStore) -> Result<std::collections::BTreeSet<(String, String)>, String> {
    // One var (?v) per select_v's parser: pack domain|class into one binding.
    let sparql = r##"SELECT ?v WHERE { GRAPH ?g { ?d ?p ?c . FILTER(STRENDS(STR(?p),"definesVocabulary")) BIND(CONCAT(STRAFTER(STR(?d),"#"),"|",STRAFTER(STR(?c),"#")) AS ?v) } }"##;
    let rows = store.select_v(sparql)?;
    if rows.is_empty() {
        // could-not-ask must never read as "no claims": an empty claim set
        // would make EVERY retire refuse claim-not-found — safe direction —
        // but an unreachable store should say so, not masquerade as data.
        return Err("retire-claim: store returned ZERO claims — either Fuseki is unreachable or the model is empty; refusing to check against a blind set (#3731 class)".into());
    }
    Ok(rows.into_iter().filter_map(|r| {
        let mut it = r.splitn(2, '|');
        match (it.next(), it.next()) {
            (Some(d), Some(c)) if !d.is_empty() && !c.is_empty() => Some((d.to_string(), c.to_string())),
            _ => None,
        }
    }).collect())
}

fn fetch_served_routes() -> std::collections::BTreeSet<String> {
    // owl-api's error envelope lists every served route. Unreachable owl-api →
    // EMPTY set → the ClaimServed check can't fire; acceptable fail-open ONLY
    // because the deploy-side ASK re-verifies, and a down owl-api means nothing
    // is being served to protect. Logged loudly either way.
    let out = std::process::Command::new("curl")
        .args(["-s", "-m", "5", "http://localhost:3360/__athena_model_probe__"])
        .output();
    let body = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            eprintln!("athena-model: WARN owl-api unreachable — served-route check running against an EMPTY set (nothing is being served while it is down)");
            return Default::default();
        }
    };
    // hand-parse {"served": ["/a", "/b", ...]} — zero-dep like select_v.
    let mut routes = std::collections::BTreeSet::new();
    if let Some(i) = body.find("\"served\"") {
        let rest = &body[i..];
        if let (Some(s), Some(e)) = (rest.find('['), rest.find(']')) {
            for tok in rest[s + 1..e].split(',') {
                let r = tok.trim().trim_matches('"').trim_start_matches('/').to_string();
                if !r.is_empty() { routes.insert(r); }
            }
        }
    }
    routes
}

fn today_utc() -> String {
    // date -u: zero-dep, same approach as chorus-model-deploy's stamp.
    std::process::Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
