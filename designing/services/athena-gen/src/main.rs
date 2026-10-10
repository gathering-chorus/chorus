<<<<<<< HEAD
<<<<<<< HEAD
// @domain: domains
//! athena-gen (#4490):
//!   athena-gen model    --root <repo> --product <name>              prints the product's slice of the model
//!   athena-gen generate --root <repo> --product <name> --out <dir>
//!                                                                    writes the product's API crate
=======
//! athena-gen — `athena-gen model --root <repo> --product <name>` prints the product's
//! slice of the model; `generate` (next) writes its API crate.
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
=======
// @domain: domains
//! athena-gen (#4490):
//!   athena-gen model    --root <repo> --product <name>              prints the product's slice of the model
//!   athena-gen generate --root <repo> --product <name> --out <dir>
//!                                                                    writes the product's API crate
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)

use std::path::PathBuf;

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

<<<<<<< HEAD
<<<<<<< HEAD
=======
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("athena-gen: {msg}");
    std::process::exit(1);
}

<<<<<<< HEAD
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    let Some(product) = arg(&args, "--product") else {
        eprintln!("usage: athena-gen model|generate --root <repo> --product <name> [--out <dir>]");
        std::process::exit(2);
    };
    let files = athena_gen::model::model_files(&root);
    let (store, skipped) = athena_gen::model::load(&files).unwrap_or_else(|e| die(e));
    for s in &skipped {
        eprintln!("athena-gen: skipped (does not parse): {s}");
    }
    let p = athena_gen::model::product(&store, &product).unwrap_or_else(|e| die(e));
    match cmd.as_str() {
        "model" => println!("{p:#?}"),
        "generate" => {
            let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("generate needs --out <dir>")));
            let mv = athena_gen::model::model_version(&store).unwrap_or_else(|e| die(e));
            let settings = athena_gen::model::settings(&store, &product).unwrap_or_else(|e| die(e));
            let missing = athena_gen::model::missing_settings(&settings);
            if !missing.is_empty() {
                die(format!("{product} declares no setting for {missing:?}: add a Property row (c:{product} c:hasProperty …) for each"));
            }
            for (path, text) in athena_gen::emit::crate_files(&p, &mv, &settings) {
                let f = out.join(&path);
                if let Some(dir) = f.parent() {
                    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(e));
                }
                std::fs::write(&f, text).unwrap_or_else(|e| die(e));
                println!("wrote {}", f.display());
            }
        }
        other => die(format!("unknown command '{other}' (model | generate)")),
=======
=======
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    let Some(product) = arg(&args, "--product") else {
        eprintln!("usage: athena-gen model|generate --root <repo> --product <name> [--out <dir>]");
        std::process::exit(2);
    };
    let files = athena_gen::model::model_files(&root);
    let (store, skipped) = athena_gen::model::load(&files).unwrap_or_else(|e| die(e));
    for s in &skipped {
        eprintln!("athena-gen: skipped (does not parse): {s}");
    }
    let p = athena_gen::model::product(&store, &product).unwrap_or_else(|e| die(e));
    match cmd.as_str() {
        "model" => println!("{p:#?}"),
        "generate" => {
            let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("generate needs --out <dir>")));
            let mv = athena_gen::model::model_version(&store).unwrap_or_else(|e| die(e));
            let settings = athena_gen::model::settings(&store, &product).unwrap_or_else(|e| die(e));
            let missing = athena_gen::model::missing_settings(&settings);
            if !missing.is_empty() {
                die(format!("{product} declares no setting for {missing:?}: add a Property row (c:{product} c:hasProperty …) for each"));
            }
            for (path, text) in athena_gen::emit::crate_files(&p, &mv, &settings) {
                let f = out.join(&path);
                if let Some(dir) = f.parent() {
                    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(e));
                }
                std::fs::write(&f, text).unwrap_or_else(|e| die(e));
                println!("wrote {}", f.display());
            }
        }
<<<<<<< HEAD
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
=======
        other => die(format!("unknown command '{other}' (model | generate)")),
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
    }
}
