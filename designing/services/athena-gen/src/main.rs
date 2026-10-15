// @domain: domains
//! athena-gen (#4490):
//!   athena-gen model    --root <repo> --product <name>              prints the product's slice of the model
//!   athena-gen generate --root <repo> --product <name> --out <dir> [--verifier <chorus-oidc dir>]
//!                                                                    writes the product's API crate
//!   athena-gen generate --card <n> [--out <dir>] [--verifier <dir>]  #4515: every product the card's model
//!                                                                    change touches, from the card's werk,
//!                                                                    each to <out>/<product>/api
//!                                                                    (default <werk>/directing/products)
//!   athena-gen model    --card <n>                                   #4515: which products the card touches
//!   athena-gen tool-inputs --root <repo> --out <file>                writes the tool runner's input table (#4513)
//!   athena-gen runtimes --root <repo> --out <file>                   writes the agent runtime table (#4444)

use std::path::{Path, PathBuf};

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("athena-gen: {msg}");
    std::process::exit(1);
}

/// Load the whole model of a tree. A model file that does not parse would
/// silently drop classes or fields from the generated API (Kade's review, #4490
/// run 1): refuse instead, naming each file.
fn load_model(root: &Path) -> oxigraph::store::Store {
    let files = athena_gen::model::model_files(root);
    let (store, skipped) = athena_gen::model::load(&files).unwrap_or_else(|e| die(e));
    if !skipped.is_empty() {
        die(format!("{} model file(s) do not parse; nothing generated:\n  {}", skipped.len(), skipped.join("\n  ")));
    }
    store
}

/// Write one product's API crate into `out`.
fn generate(store: &oxigraph::store::Store, root: &Path, product: &str, out: &Path, verifier: Option<&String>) {
    let p = athena_gen::model::product(store, product).unwrap_or_else(|e| die(e));
    let mv = athena_gen::model::model_version(store).unwrap_or_else(|e| die(e));
    let settings = athena_gen::model::settings(store, product).unwrap_or_else(|e| die(e));
    let missing = athena_gen::model::missing_settings(&settings);
    if !missing.is_empty() {
        die(format!("{product} declares no setting for {missing:?}: add a Property row (c:{product} c:hasProperty …) for each"));
    }
    // #4519 — a product repo outside Chorus names the verifier checkout with --verifier
    let oidc = match verifier {
        Some(v) => athena_gen::emit::verifier_path(&PathBuf::from(v), out),
        None => athena_gen::emit::oidc_path(root, out),
    }
    .unwrap_or_else(|e| die(e));
    for (path, text) in athena_gen::emit::crate_files_with(&p, &mv, &settings, &oidc) {
        let f = out.join(&path);
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| die(e));
        }
        std::fs::write(&f, text).unwrap_or_else(|e| die(e));
        println!("wrote {}", f.display());
    }
}

/// #4547 — the touched products the generator builds: those that declare their own
/// settings. The rest are served by the v1 door and are named, never generated.
fn generated_products<'a>(store: &oxigraph::store::Store, touched: impl Iterator<Item = &'a String>, card: &str) -> Vec<String> {
    let mut built = Vec::new();
    for p in touched {
        let declared = athena_gen::model::settings(store, p).unwrap_or_else(|e| die(e));
        if declared.iter().any(|(k, _)| k == "model.namespace") {
            built.push(p.clone());
        } else {
            println!("card #{card}: {p} declares no settings — served by the v1 door, nothing to generate");
        }
    }
    built
}

/// #4515 — the card's model change → the products it touches. Refuses (exit 1,
/// named) on a card with no werk or no model change, on a touched class no
/// product serves, and on a change that touches no product at all.
fn card_mode(cmd: &str, card_id: &str, args: &[String]) {
    use athena_gen::card::{card_scope, product_out, products_for, touched};
    let card = card_scope::resolve(card_id).unwrap_or_else(|e| die(e));
    let ttl: Vec<PathBuf> = card.ttl().iter().map(|r| PathBuf::from(card.abs(r))).collect();
    println!("card #{}: {} model file(s) in {}", card.card, ttl.len(), card.werk);
    let t = touched(&ttl).unwrap_or_else(|e| die(format!("card #{}: {e}", card.card)));
    let root = PathBuf::from(&card.werk);
    let store = load_model(&root);
    let (products, unserved) = products_for(&store, &t).unwrap_or_else(|e| die(e));
    for (p, classes) in &products {
        if classes.is_empty() {
            println!("card #{}: {p} (its own rows changed)", card.card);
        } else {
            println!("card #{}: {p} <- {}", card.card, classes.join(", "));
        }
    }
    if !unserved.is_empty() {
        die(format!(
            "card #{}: {} class(es) the card touches are in no product's domain — nothing would generate them: {}",
            card.card,
            unserved.len(),
            unserved.join(", ")
        ));
    }
    if products.is_empty() {
        die(format!("card #{}: the card's model change touches no product — nothing to generate", card.card));
    }
    // #4547 — a product that declares no settings is served by the v1 door (athena-make),
    // not generated. A card changing its model skips it here; naming it with --product
    // still refuses (#4519: never read as Chorus by default).
    let built = generated_products(&store, products.keys(), &card.card);
    if cmd == "model" {
        return;
    }
    if built.is_empty() {
        println!("card #{}: every touched product is served by the v1 door — nothing to generate", card.card);
        return;
    }
    let out_base = arg(args, "--out").map(PathBuf::from).unwrap_or_else(|| root.join("directing/products"));
    let verifier = arg(args, "--verifier");
    for p in &built {
        generate(&store, &root, p, &product_out(&out_base, p), verifier.as_ref());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).cloned().unwrap_or_default();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    if cmd == "tool-inputs" {
        let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("tool-inputs needs --out <file>")));
        let store = load_model(&root);
        let inputs = athena_gen::model::tool_inputs(&store).unwrap_or_else(|e| die(e));
        std::fs::write(&out, athena_gen::emit::tool_inputs_json(&inputs)).unwrap_or_else(|e| die(e));
        println!("wrote {} ({} tool input shape(s))", out.display(), inputs.len());
        return;
    }
    if cmd == "runtimes" {
        let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("runtimes needs --out <file>")));
        let (store, skipped) = athena_gen::model::load(&athena_gen::model::model_files(&root)).unwrap_or_else(|e| die(e));
        if !skipped.is_empty() {
            die(format!("{} model file(s) do not parse; nothing generated:\n  {}", skipped.len(), skipped.join("\n  ")));
        }
        let t = athena_gen::model::agent_runtimes(&store).unwrap_or_else(|e| die(e));
        std::fs::write(&out, athena_gen::emit::agent_runtimes_json(&t)).unwrap_or_else(|e| die(e));
        println!("wrote {} ({} runtime(s), {} principal(s) assigned)", out.display(), t.runtimes.len(), t.runs_on.len());
        return;
    }
    if let Some(c) = athena_gen::card::card_scope::card_arg(&args) {
        let c = c.unwrap_or_else(|e| die(e));
        if !matches!(cmd.as_str(), "model" | "generate") {
            die(format!("--card works with model | generate, not '{cmd}'"));
        }
        if args.iter().any(|a| a == "--product" || a == "--root") {
            die("--card names the product(s) and the tree itself: drop --product / --root");
        }
        card_mode(&cmd, &c, &args);
        return;
    }
    let Some(product) = arg(&args, "--product") else {
        eprintln!("usage: athena-gen model|generate --root <repo> --product <name> [--out <dir>] | --card <n> [--out <dir>]");
        std::process::exit(2);
    };
    let store = load_model(&root);
    match cmd.as_str() {
        "model" => {
            let p = athena_gen::model::product(&store, &product).unwrap_or_else(|e| die(e));
            println!("{p:#?}")
        }
        "generate" => {
            let out = PathBuf::from(arg(&args, "--out").unwrap_or_else(|| die("generate needs --out <dir>")));
            generate(&store, &root, &product, &out, arg(&args, "--verifier").as_ref());
        }
        other => die(format!("unknown command '{other}' (model | generate)")),
    }
}
