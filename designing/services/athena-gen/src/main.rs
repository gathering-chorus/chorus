//! athena-gen — `athena-gen model --root <repo> --product <name>` prints the product's
//! slice of the model; `generate` (next) writes its API crate.

use std::path::PathBuf;

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    let Some(product) = arg(&args, "--product") else {
        eprintln!("usage: athena-gen model --root <repo> --product <name>");
        std::process::exit(2);
    };
    let files = athena_gen::model::model_files(&root);
    let (store, skipped) = match athena_gen::model::load(&files) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("athena-gen: {e}");
            std::process::exit(1);
        }
    };
    for s in &skipped {
        eprintln!("athena-gen: skipped (does not parse): {s}");
    }
    match athena_gen::model::product(&store, &product) {
        Ok(p) => println!("{p:#?}"),
        Err(e) => {
            eprintln!("athena-gen: {e}");
            std::process::exit(1);
        }
    }
}
