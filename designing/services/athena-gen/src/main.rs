// @domain: domains
//! athena-gen (#4490):
//!   athena-gen model    --root <repo> --product <name>              prints the product's slice of the model
//!   athena-gen generate --root <repo> --product <name> --out <dir>
//!                                                                    writes the product's API crate
//!   athena-gen tool-inputs --root <repo> --out <file>                writes the tool runner's input table (#4513)

use std::path::PathBuf;

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("athena-gen: {msg}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    if cmd == "tool-inputs" {
        let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("tool-inputs needs --out <file>")));
        let files = athena_gen::model::model_files(&root);
        let (store, skipped) = athena_gen::model::load(&files).unwrap_or_else(|e| die(e));
        if !skipped.is_empty() {
            die(format!("{} model file(s) do not parse; nothing generated:\n  {}", skipped.len(), skipped.join("\n  ")));
        }
        let inputs = athena_gen::model::tool_inputs(&store).unwrap_or_else(|e| die(e));
        std::fs::write(&out, athena_gen::emit::tool_inputs_json(&inputs)).unwrap_or_else(|e| die(e));
        println!("wrote {} ({} tool input shape(s))", out.display(), inputs.len());
        return;
    }
    let Some(product) = arg(&args, "--product") else {
        eprintln!("usage: athena-gen model|generate --root <repo> --product <name> [--out <dir>]");
        std::process::exit(2);
    };
    let files = athena_gen::model::model_files(&root);
    let (store, skipped) = athena_gen::model::load(&files).unwrap_or_else(|e| die(e));
    // a model file that does not parse would silently drop classes or fields from the
    // generated API (Kade's review, #4490 run 1): refuse instead, naming each file
    if !skipped.is_empty() {
        die(format!("{} model file(s) do not parse; nothing generated:\n  {}", skipped.len(), skipped.join("\n  ")));
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
            let oidc = athena_gen::emit::oidc_path(&root, &out).unwrap_or_else(|e| die(e));
            for (path, text) in athena_gen::emit::crate_files_with(&p, &mv, &settings, &oidc) {
                let f = out.join(&path);
                if let Some(dir) = f.parent() {
                    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(e));
                }
                std::fs::write(&f, text).unwrap_or_else(|e| die(e));
                println!("wrote {}", f.display());
            }
        }
        other => die(format!("unknown command '{other}' (model | generate)")),
    }
}
