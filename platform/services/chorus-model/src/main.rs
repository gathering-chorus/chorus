//! chorus-model CLI — the governed writer's command surface (#3257).
//!
//!   chorus-model add --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]
//!   chorus-model mint --kind <kind> --name <name>
//!   chorus-model kinds
//!
//! Callers never pass IRIs — fields are literals, edges are (property, kind:name)
//! pairs the mint resolves. --dry-run prints the Turtle and writes nothing.

use chorus_model::{add_edge, batch, delete_entity, mint, remove_edge, to_turtle, write, FusekiStore, Identity, WriteReq};
use std::process::ExitCode;

fn usage() -> String {
    "chorus-model — the governed RDF/OWL writer (ADR-040 Rule 0; #3257)\n\
     usage:\n\
       chorus-model add    --kind <kind> --name <name> [--field k=v]... [--edge prop=kind:name]... [--dry-run]\n\
       chorus-model delete --kind <kind> --name <name>\n\
       chorus-model link   --kind <kind> --name <name> --edge prop=kind:name\n\
       chorus-model unlink --kind <kind> --name <name> --edge prop=kind:name\n\
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
            let (req, _) = parse_req(&args[1..])?;
            let store = FusekiStore::new();
            let id = Identity::resolve(&store)?; // #3651
            Ok(format!("deleted: {}", delete_entity(&store, &req.kind, &req.name, req.graph.as_deref(), &id)?))
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
            eprintln!("chorus-model: {}", e);
            ExitCode::FAILURE
        }
    }
}
