//! athena-model CLI — the governed writer's command surface (#3257).
//!
//!   athena-model add --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]
//!   athena-model mint --kind <kind> --name <name>
//!   athena-model kinds
//!
//! Callers never pass IRIs — fields are literals, edges are (property, kind:name)
//! pairs the mint resolves. --dry-run prints the Turtle and writes nothing.

use athena_model::{add_batch, add_edge, batch, curl_http, delete_entity, delete_iri, deploy_home, deploy_partitions, mint, post_all, post_rows, seed_multi_at, parse_add_batch_ndjson, parse_ntriples, remove_edge, seed_multi, OwnerTokens, SeedGroup, set_field, to_turtle, write, write_many, FusekiStore, Identity, Store, WriteReq};
use std::io::Read;
use std::process::ExitCode;

fn usage() -> String {
    "athena-model — the governed RDF/OWL writer (ADR-040 Rule 0; #3257)\n\
     usage:\n\
       athena-model add    --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]\n\
       athena-model add-batch    # NDJSON WriteReq objects on stdin; one atomic transaction\n\
       athena-model delete --kind <kind> --name <name>\n\
       athena-model set    --kind <kind> --name <name> --field k=v [--graph <g>]\n\
       athena-model link   --kind <kind> --name <name> --edge prop=kind:name\n\
       athena-model unlink --kind <kind> --name <name> --edge prop=kind:name\n\
       athena-model seed   (--kind <kind> --ttl <file>)... [--graph <g>] [--provenance migrated] [--base <iri>]\n\
                    --kind/--ttl repeat as pairs; several kinds load as ONE transaction (#3839)\n\
       athena-model seed --deploy   built-in instance manifest -> urn:chorus:instances, output-verified (#3895)\n\
       athena-model mint   --kind <kind> --name <name>\n\
       athena-model kinds"
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
                // #4096 — a repeated key is a further value of a multi-valued
                // property, not an overwrite (the first value stays in `fields`).
                if req.fields.contains_key(k) {
                    req.more_values.push((k.to_string(), v.to_string()));
                } else {
                    req.fields.insert(k.to_string(), v.to_string());
                }
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
                // #3647 — the class's model-declared instance home (athena-make resolves + passes it).
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
                    // #3885 — collect EVERY --required, not just the first. `get`
                    // returns one occurrence, so `--required a --required b --required c`
                    // silently became `a`: the shape emitted a one-property floor while
                    // the caller believed they had asked for four. The writer already
                    // refuses a shape with NO floor ("the appearance of validation, not
                    // validation") — accepting a floor narrower than requested is the
                    // same failure one step quieter. Comma form still works.
                    let required: Vec<String> = rest
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.as_str() == "--required")
                        .filter_map(|(i, _)| rest.get(i + 1))
                        .flat_map(|s| s.split(','))
                        .filter(|x| !x.is_empty())
                        .map(str::to_string)
                        .collect();
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
            // #3885 — an ABSOLUTE --file is honored as given; only a relative one
            // is resolved against CHORUS_ROOT. Before this, a relative path typed
            // from inside a werk silently wrote to READ-ONLY CANONICAL and then
            // reported the same relative path back, which reads as "your werk".
            // Wren hit it live on 2026-08-21 cutting the StreamEvent class: six
            // properties and a shape landed in canonical, git-clean werk, no
            // refusal — the canonical_write_guard blocks a ROLE's edit, not a
            // verb writing on the role's behalf. Same CHORUS_ROOT-unpinned trap
            // that made the clippy ratchet grade the wrong tree the day before.
            let path = if std::path::Path::new(&file).is_absolute() {
                file.clone()
            } else {
                format!("{}/{}", std::env::var("CHORUS_ROOT").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string()), file)
            };
            // #3885 — ENSURE THE PREFIXES THE OUTPUT USES. The shape verb emits
            // `sh:` turtle; domains-wren-silas.ttl declared only chorus:/owl:/rdfs:.
            // The write "succeeded", riot refused the file at deploy, and
            // model-deploy was blocked TEAM-WIDE with the change already merged —
            // landed-but-not-live, found by Silas, not by the writer. A governed
            // writer that emits a prefix it has not declared is writing something
            // it cannot itself parse.
            {
                let existing = std::fs::read_to_string(&path).unwrap_or_default();
                let mut header = String::new();
                for (p, iri) in [
                    ("sh:", "http://www.w3.org/ns/shacl#"),
                    ("owl:", "http://www.w3.org/2002/07/owl#"),
                    ("rdfs:", "http://www.w3.org/2000/01/rdf-schema#"),
                ] {
                    let used = turtle.contains(p);
                    let declared = existing.contains(&format!("@prefix {}", p));
                    if used && !declared {
                        header.push_str(&format!("@prefix {:<7} <{}> .\n", p, iri));
                    }
                }
                if !header.is_empty() {
                    let merged = format!("{}{}", header, existing);
                    std::fs::write(&path, merged)
                        .map_err(|e| format!("cannot add prefixes to {}: {}", path, e))?;
                }
            }
            let mut f = std::fs::OpenOptions::new().append(true).open(&path)
                .map_err(|e| format!("cannot append to {}: {}", path, e))?;
            write!(f, "
{}", turtle).map_err(|e| format!("write failed: {}", e))?;
            // #3902 — the pen DECLARES the version: every TBox write classifies
            // itself (add = MINOR) and bumps the vocabulary ledger + its store
            // projection. Fail-loud: an unrecorded bump would be a hand-edit
            // wearing the pen's clothes.
            let root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
            let who = std::env::var("DEPLOY_ROLE").or_else(|_| std::env::var("CHORUS_ROLE")).unwrap_or_else(|_| "system".to_string());
            let card = std::env::var("CHORUS_CARD").unwrap_or_default();
            let vv = athena_model::vocab_version::record(&root, v, &format!("{}:{}", v, file), &who, &card)?;
            Ok(format!("appended to {} (vocabVersion -> {}):
{}", file, vv, turtle))
        }
        // #3752 — the FOURTH TBox verb: retirement. Shares the mint verbs'
        // refusal-first machinery, DIVERGES at the write boundary: mint ends in
        // append-to-file, retire ends in emit-to-STAGING (designing/schemas/
        // model-retirements.jsonl) which athena-deploy-model's retirement
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
            // athena-make's route table — never a file (DECLARED⊃CLAIMED⊃SERVED).
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
            // #3902 — a retire is a BREAKING change: MAJOR bump at the pen,
            // recorded when the retirement is STAGED (the declaration moment),
            // not at the deploy that executes it.
            let root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
            let vv = athena_model::vocab_version::record(&root, "retire", &format!("{}:{}", spec.domain, spec.class), &by, &card)?;
            Ok(format!(
                "staged retirement (executes on next model deploy; vocabVersion -> {}):\n{}\n→ {}",
                vv, entry, path
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
        // Phase A1 — entity-generic bulk add. The stdin protocol is NDJSON, one
        // WriteReq-shaped object per line; robust decoding lives in the library
        // so direct callers and this CLI share the exact same wire contract.
        // Identity resolves only after the entire stream parses, and add_batch
        // validates every entity before its single Store::update.
        Some("add-batch") => {
            if args.len() != 1 {
                return Err("add-batch: no command-line arguments; pipe one WriteReq JSON object per line on stdin".into());
            }
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|e| format!("add-batch: cannot read stdin: {}", e))?;
            let reqs = parse_add_batch_ndjson(&input)?;
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // same verified identity gate as add
            let report = add_batch(&store, &reqs, &id)?;
            Ok(format!(
                "written-batch: {} entity(s)\n{}",
                report.subjects.len(),
                report.subjects.join("\n")
            ))
        }
        // #4102 — replace several rows in ONE update. athena-make sends the row
        // being written and the Revision holding the version it displaces, so a
        // failure cannot leave one without the other.
        Some("write-many") => {
            if args.len() != 1 {
                return Err("write-many: no command-line arguments; pipe one WriteReq JSON object per line on stdin".into());
            }
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|e| format!("write-many: cannot read stdin: {}", e))?;
            let reqs = parse_add_batch_ndjson(&input)?;
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?;
            let subjects = write_many(&store, &reqs, &id)?;
            Ok(format!("written: {} entity(s)\n{}", subjects.len(), subjects.join("\n")))
        }
        // #3468 — delete / link / unlink: the governed verbs athena-make delegates to,
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
            let mut deploy = false;
            let mut post = false;
            let mut api: Option<String> = None;
            let mut dry = false;
            let mut unowned_mode = String::from("refuse");
            // #4096 — owned rows post AFTER the ownerless kinds load (roles, value streams,
            // steps are what the owned rows point at); held here across the loader path
            let mut deferred_post: Option<(Vec<athena_model::PostRow>, String)> = None;
            let rest = &args[1..];
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    // #3895 — the deploy manifest, IN the binary (ADR-038: no
                    // deploy-path bash). Replaces athena-deploy-model.sh's
                    // INSTANCE_SET leg, which had put an identity gate inside
                    // the #3785 recovery path. --deploy loads the built-in
                    // kind:file set, targets urn:chorus:instances, stamps
                    // provenance=deploy, and output-verifies after the write.
                    "--deploy" => deploy = true,
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
                    // #4096 — post through the API as each row's owner (Jeff: "each owner in turn")
                    "--post" => { post = true; }
                    "--api" => { i += 1; api = rest.get(i).cloned(); }
                    "--dry-run" => { dry = true; }
                    // #4096 — what to do with rows whose shape carries no owner (value
                    // streams, steps, roles, pipelines, cards today: 95 of 200): "refuse"
                    // (default) or "load" them through the file loader while the door
                    // gains an owner for those kinds. Always printed, never silent.
                    "--unowned" => { i += 1; unowned_mode = rest.get(i).cloned().unwrap_or_default(); }
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
            if deploy && post {
                return Err("seed: --deploy (file loader) and --post (the API) are two doors — pick one; --post is the land's".into());
            }
            if deploy || post {
                if !pairs.is_empty() {
                    return Err("seed: --deploy/--post carry their own manifest — do not combine with --kind/--ttl".into());
                }
                let root = std::env::var("CHORUS_ROOT")
                    .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
                // The manifest is DATA with one home — read by this verb and
                // parsed by athena-make's deploy_set() audit. kind:path per line,
                // stated per file, because the door validates each group
                // against the STATED class's shape (#3839).
                let mpath = format!("{}/platform/config/instance-seed-manifest.txt", root);
                let mbody = std::fs::read_to_string(&mpath)
                    .map_err(|e| format!("seed --deploy: manifest unreadable ({}): {}", mpath, e))?;
                for line in mbody.lines() {
                    let l = line.trim();
                    if l.is_empty() || l.starts_with('#') {
                        continue;
                    }
                    let (k, rel) = l
                        .split_once(':')
                        .ok_or_else(|| format!("seed --deploy: manifest line not kind:path — '{}'", l))?;
                    pairs.push((k.trim().to_string(), format!("{}/{}", root, rel.trim())));
                }
                if pairs.is_empty() {
                    return Err(format!("seed --deploy: manifest {} declares no groups — refusing a vacuous deploy", mpath));
                }
                for (_, f) in &pairs {
                    if !std::path::Path::new(f).is_file() {
                        return Err(format!("seed --deploy: manifest TTL not found: {}", f));
                    }
                }
                if graph.is_none() {
                    graph = Some("urn:chorus:instances".into());
                }
                provenance = "deploy".into();
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
            if post {
                // #4096 — the API is the door. Every subject becomes the API's own
                // write body and is sent as its owner; the store is never touched
                // from here. Manifest order is dependency order (targets first).
                let api = api
                    .or_else(|| std::env::var("ATHENA_MAKE_URL").ok())
                    .unwrap_or_else(|| "http://localhost:3360".to_string());
                let root = std::env::var("CHORUS_ROOT")
                    .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
                let mint = move |owner: &str| -> Result<String, String> {
                    let out = std::process::Command::new(format!("{}/platform/scripts/chorus-identity-token", root))
                        .arg(owner)
                        .output()
                        .map_err(|e| format!("seed --post: cannot mint an identity for '{}': {}", owner, e))?;
                    if !out.status.success() {
                        return Err(format!("seed --post: identity for '{}' refused: {}", owner, String::from_utf8_lossy(&out.stderr).trim()));
                    }
                    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
                };
                let mut tokens = OwnerTokens::new(&mint);
                let mut rows = Vec::new();
                for (k, triples) in &parsed {
                    rows.extend(post_rows(k, triples)?);
                }
                let (owned, unowned): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.owner.is_some());
                let mut per_kind: Vec<(String, usize)> = Vec::new();
                for r in &unowned {
                    match per_kind.iter_mut().find(|(k, _)| *k == r.kind) {
                        Some(e) => e.1 += 1,
                        None => per_kind.push((r.kind.clone(), 1)),
                    }
                }
                let unowned_note = per_kind.iter().map(|(k, n)| format!("{}={}", k, n)).collect::<Vec<_>>().join(",");
                if !unowned.is_empty() && unowned_mode != "load" {
                    return Err(format!(
                        "seed --post: {} row(s) carry no ownedBy, so no owner can sign them ({}). \
                         Give those kinds an owner, or say --unowned load to load them through the file loader for now.",
                        unowned.len(), unowned_note
                    ));
                }
                if unowned.is_empty() {
                    let rep = post_all(&owned, &api, &mut tokens, &curl_http, dry)?;
                    for l in &rep.lines { println!("{}", l); }
                    return Ok(format!(
                        "posted: {} rows through {} ({} created, {} replaced){}",
                        owned.len(), api, rep.created, rep.replaced,
                        if dry { " — dry-run, nothing sent" } else { "" }
                    ));
                }
                // --unowned load: the ownerless kinds go through the file loader FIRST
                // (the owned rows point at them: ownedBy → role, atStep → step), then
                // the owned rows post. Said out loud every run so the remainder shrinks
                // on purpose.
                println!("unowned rows loaded through the file loader (their shapes carry no owner): {}", unowned_note);
                let posted_line = format!("{} owned rows to post through {} after the loader", owned.len(), api);
                deferred_post = Some((owned, api.clone()));
                let keep: std::collections::HashSet<(String, String)> =
                    unowned.iter().map(|r| (r.kind.clone(), r.name.clone())).collect();
                let mut trimmed: Vec<(String, Vec<(String, String, String)>)> = Vec::new();
                for (k, triples) in &parsed {
                    let kept: Vec<(String, String, String)> = triples.iter().filter(|(s, _, _)| {
                        let local = s.trim_start_matches('<').trim_end_matches('>').rsplit(['#', '/']).next().unwrap_or("");
                        keep.iter().any(|(kk, name)| kk == k && (local == name.as_str() || local == format!("{}-{}", k, name)))
                    }).cloned().collect();
                    if !kept.is_empty() { trimmed.push((k.clone(), kept)); }
                }
                parsed = trimmed;
                deploy = true;
                if dry {
                    return Ok(format!("{} — dry-run; {} unowned rows would load through the file loader first", posted_line, unowned.len()));
                }
            }
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651 — same gate as every verb
            let kind = pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join("+");
            // #3895 — output-verify (deploy only): every SUBJECT declared in the
            // manifest must be present in the live graph. Asks the store, names
            // any absentee — a verify that cannot fail is worse than none
            // (#3839's staging-compare verify was exactly that).
            if deploy {
                // #4089 — each manifest kind lands in ITS SHAPE'S home graph (a
                // declared instancesGraph), else the legacy bucket. One batch per
                // home, in manifest order, so an earlier home's subjects are in
                // the store when a later home's edges point at them.
                let default_home = graph.as_deref().unwrap_or("urn:chorus:instances");
                let mut homes: Vec<String> = Vec::new();
                for (k, _) in &parsed {
                    homes.push(deploy_home(&store, k, default_home)?);
                }
                // ONE batch (referential integrity spans homes, #3839), each
                // group written to its own home; verify per home afterwards.
                let groups: Vec<SeedGroup> = parsed
                    .iter()
                    .map(|(k, t)| SeedGroup { kind: k.as_str(), triples: t.as_slice() })
                    .collect();
                let report = seed_multi_at(&store, &groups, &provenance, None, Some(&homes), &id)?;
                let parts = deploy_partitions(&homes);
                let (total_subjects, total_triples) = (report.subjects, report.triples);
                let mut verified: Vec<String> = Vec::new();
                for (g, idx) in &parts {
                    // parse_ntriples keeps subjects bracketed (`<iri>`); bare IRIs
                    // for comparison with select_v's unbracketed bindings.
                    let mut declared: Vec<String> = idx
                        .iter()
                        .flat_map(|&i| parsed[i].1.iter().map(|(s, _, _)| s.trim_matches(['<', '>']).to_string()))
                        .collect();
                    declared.sort();
                    declared.dedup();
                    let values = declared.iter().map(|s| format!("<{}>", s)).collect::<Vec<_>>().join(" ");
                    let q = format!(
                        "SELECT DISTINCT ?v WHERE {{ VALUES ?v {{ {} }} GRAPH <{}> {{ ?v ?p ?o }} }}",
                        values, g
                    );
                    let present = store.select_v(&q)?;
                    let missing: Vec<&String> = declared
                        .iter()
                        .filter(|s| !present.iter().any(|p| p == *s))
                        .collect();
                    if !missing.is_empty() {
                        return Err(format!(
                            "seed --deploy VERIFY FAILED: {} of {} declared subjects absent from <{}>: {}",
                            missing.len(),
                            declared.len(),
                            g,
                            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    verified.push(format!("{} in <{}>", declared.len(), g));
                }
                let seeded_line = format!(
                    "seeded: {} subjects / {} triples (kind={}, provenance={}) — declared subjects verified live: {}",
                    total_subjects, total_triples, kind, provenance, verified.join("; ")
                );
                if let Some((owned, api)) = deferred_post.take() {
                    // #4096 — now the owned rows, through the door, as each owner
                    let root = std::env::var("CHORUS_ROOT")
                        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
                    let mint = move |owner: &str| -> Result<String, String> {
                        let out = std::process::Command::new(format!("{}/platform/scripts/chorus-identity-token", root))
                            .arg(owner)
                            .output()
                            .map_err(|e| format!("seed --post: cannot mint an identity for '{}': {}", owner, e))?;
                        if !out.status.success() {
                            return Err(format!("seed --post: identity for '{}' refused: {}", owner, String::from_utf8_lossy(&out.stderr).trim()));
                        }
                        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
                    };
                    let mut tokens = OwnerTokens::new(&mint);
                    let rep = post_all(&owned, &api, &mut tokens, &curl_http, false)?;
                    for l in &rep.lines { println!("{}", l); }
                    println!("{}", seeded_line);
                    return Ok(format!(
                        "posted: {} rows through {} ({} created, {} replaced); {}",
                        owned.len(), api, rep.created, rep.replaced, seeded_line
                    ));
                }
                return Ok(seeded_line);
            }
            let groups: Vec<SeedGroup> = parsed
                .iter()
                .map(|(k, t)| SeedGroup { kind: k.as_str(), triples: t.as_slice() })
                .collect();
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
        // #3573 — governed BATCH: the migration target athena-make's /batch delegates to.
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
// claims from the STORE, serving from athena-make's own route table.
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
    // athena-make's error envelope lists every served route. Unreachable athena-make →
    // EMPTY set → the ClaimServed check can't fire; acceptable fail-open ONLY
    // because the deploy-side ASK re-verifies, and a down athena-make means nothing
    // is being served to protect. Logged loudly either way.
    let out = std::process::Command::new("curl")
        .args(["-s", "-m", "5", "http://localhost:3360/__athena_model_probe__"])
        .output();
    let body = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            eprintln!("athena-model: WARN athena-make unreachable — served-route check running against an EMPTY set (nothing is being served while it is down)");
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
    // date -u: zero-dep, same approach as athena-deploy-model's stamp.
    std::process::Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod pen_write_target_tests_3885 {
    // #3885 — both defects found by USING the pen on 2026-08-21, not by reading it.

    /// A relative --file resolves against CHORUS_ROOT; an absolute one is honored.
    /// Before the fix, a relative path typed from inside a werk wrote to READ-ONLY
    /// canonical and reported the same relative path back — which reads as "your werk".
    /// Six properties and a shape landed in the wrong tree with no refusal.
    fn resolve(file: &str, root: &str) -> String {
        if std::path::Path::new(file).is_absolute() {
            file.to_string()
        } else {
            format!("{}/{}", root, file)
        }
    }

    #[test]
    fn relative_file_resolves_against_chorus_root() {
        assert_eq!(resolve("roles/wren/o.ttl", "/chorus"), "/chorus/roles/wren/o.ttl");
    }

    /// NEGATIVE PROOF — an absolute werk path must NOT be rewritten into canonical.
    /// This is the exact 2026-08-21 miswrite.
    #[test]
    fn absolute_werk_path_is_not_redirected_to_canonical() {
        let werk = "/Users/j/CascadeProjects/chorus-werk/wren-3885/roles/wren/o.ttl";
        assert_eq!(resolve(werk, "/Users/j/CascadeProjects/chorus"), werk);
    }

    /// Every --required is collected. `get()` returns ONE occurrence, so three flags
    /// silently became one and the shape emitted a one-property floor while the caller
    /// believed they had four. The writer refuses a shape with NO floor; accepting a
    /// floor narrower than requested is the same failure one step quieter.
    fn required_from(rest: &[String]) -> Vec<String> {
        rest.iter()
            .enumerate()
            .filter(|(_, a)| a.as_str() == "--required")
            .filter_map(|(i, _)| rest.get(i + 1))
            .flat_map(|s| s.split(','))
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn every_required_flag_is_collected() {
        let args: Vec<String> = ["--required", "a", "--required", "b", "--required", "c"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(required_from(&args), vec!["a", "b", "c"]);
    }

    #[test]
    fn comma_form_still_works_and_mixes() {
        let args: Vec<String> = ["--required", "a,b", "--required", "c"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(required_from(&args), vec!["a", "b", "c"]);
    }

    /// NEGATIVE PROOF — no --required yields an EMPTY floor, so the writer's
    /// shape-with-no-floor refusal still fires. A silent default here would defeat it.
    #[test]
    fn absent_required_yields_no_floor_so_the_refusal_still_fires() {
        assert!(required_from(&[]).is_empty());
    }
}
