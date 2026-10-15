//! #4515 — `athena-validate --card <n>`: the Model step of the v2 athena pipeline.
//!
//! Ports the old athena.yml `shape` step (and the land's riot `validate` leg) from
//! "one class someone typed" to "the card's own model change": every TTL the card
//! changed must parse, and every class those files declare must have a shape. One
//! line per file and per class; exit 0 clean, 1 on any failure, 2 when the card
//! could not be scoped or the model could not be read (UNMEASURED, never clean).

#[allow(dead_code)] // each crate uses part of the shared rule
mod card_scope {
    include!("../../shared/card_scope.rs");
}

/// Run the card's Model check. Returns the exit code.
pub fn run(card_id: &str) -> i32 {
    let card = match card_scope::resolve(card_id) {
        Ok(c) => c,
        Err(why) => {
            println!("card-scope|#{}|REFUSED|{why}", card_id.trim_start_matches('#'));
            eprintln!("athena-validate: {why}");
            return 2;
        }
    };
    println!("card-scope|#{}|{}|{} model source(s) since main", card.card, card.werk, card.sources.len());

    // 1 — every changed TTL parses. A file that does not parse names riot's reason.
    let mut fails = 0usize;
    for s in &card.sources {
        if !s.ends_with(".ttl") {
            println!("card-model|{s}|not TTL — read by athena-deploy's retirement leg, nothing to parse");
            continue;
        }
        match card_scope::riot_validate(&card.abs(s)) {
            Ok(()) => println!("card-model|{s}|parses"),
            Err(why) if why.starts_with("riot not runnable") => {
                println!("card-model|{s}|UNMEASURED|{why}");
                println!("card-summary|UNMEASURED|riot");
                return 2;
            }
            Err(why) => {
                fails += 1;
                println!("card-model|{s}|PARSE FAILED|{why}");
            }
        }
    }
    if fails > 0 {
        // a file that does not parse declares nothing we can read: stop here, named
        println!("card-summary|{fails}|dirty — fix the parse failures, then the shapes are checked");
        return 1;
    }

    // 2 — every class the card declares has a shape somewhere in the werk's model.
    let model = match card_scope::ntriples(&card_scope::model_tree_files(&card.werk)) {
        Ok(nt) => nt,
        Err(why) => {
            println!("card-shape|-|UNMEASURED|{why}");
            println!("card-summary|UNMEASURED|model-unreadable");
            return 2;
        }
    };
    let mut classes = 0usize;
    for s in card.ttl() {
        let nt = match card_scope::ntriples(&[card.abs(&s)]) {
            Ok(nt) => nt,
            Err(why) => {
                fails += 1;
                println!("card-shape|{s}|-|UNREADABLE|{why}");
                continue;
            }
        };
        // #4536 — the card answers only for the classes it added or changed:
        // a class declared on main with the same triples is not this card's.
        // Touching a big shared file (chorus.ttl: 28 unshaped classes) must not
        // make the card answer for every gap already in it.
        let main_nt = main_ntriples(&card.werk, &s);
        let changed = changed_classes(&nt, &main_nt);
        let untouched = card_scope::declared_classes(&nt).len() - changed.len();
        if untouched > 0 {
            println!("card-shape|{s}|-|{untouched} class(es) unchanged from main — not this card's");
        }
        for class in changed {
            classes += 1;
            let name = card_scope::local_name(&class);
            match card_scope::shape_for(&class, &model) {
                Some(shape) => println!("card-shape|{s}|{name}|{shape}"),
                None => {
                    fails += 1;
                    println!("card-shape|{s}|{name}|NO SHAPE — a class with no shape validates nothing");
                }
            }
        }
    }
    if classes == 0 {
        println!("card-shape|-|-|the card's model files declare no class — no shape to check");
    }
    if fails > 0 {
        println!("card-summary|{fails}|dirty");
        return 1;
    }
    println!("card-summary|0|clean — {} file(s) parse, {classes} class(es) shaped", card.ttl().len());
    0
}

/// #4536 — main's version of a card file as N-Triples; empty when main has no
/// such file (every class in it is then the card's).
fn main_ntriples(werk: &str, rel: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["-C", werk, "show", &format!("origin/main:{rel}")])
        .output();
    let Ok(out) = out else { return String::new() };
    if !out.status.success() {
        return String::new();
    }
    let tmp = std::env::temp_dir().join(format!("athena-validate-main-{}-{}.ttl", std::process::id(), rel.replace('/', "_")));
    if std::fs::write(&tmp, &out.stdout).is_err() {
        return String::new();
    }
    let nt = card_scope::ntriples(&[tmp.to_string_lossy().into_owned()]).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);
    nt
}

/// #4536 — classes declared in `werk_nt` that main does not declare with the
/// same triples about them: added, or changed. Pure, so the proof needs no git.
pub fn changed_classes(werk_nt: &str, main_nt: &str) -> Vec<String> {
    let about = |nt: &str, subject: &str| -> Vec<String> {
        // declared_classes returns the IRI with its angle brackets
        let key = format!("{subject} ");
        let mut v: Vec<String> = nt.lines().filter(|l| l.starts_with(&key)).map(|l| l.trim().to_string()).collect();
        v.sort();
        v
    };
    card_scope::declared_classes(werk_nt)
        .into_iter()
        .filter(|c| about(werk_nt, c) != about(main_nt, c))
        .collect()
}

#[cfg(test)]
mod changed_classes_4536 {
    use super::changed_classes;
    const OWL: &str = "<http://www.w3.org/2002/07/owl#Class>";
    const TYPE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    const LABEL: &str = "<http://www.w3.org/2000/01/rdf-schema#label>";
    fn class(n: &str, label: &str) -> String {
        format!("<urn:x#{n}> {TYPE} {OWL} .\n<urn:x#{n}> {LABEL} \"{label}\" .\n")
    }

    #[test]
    fn an_unchanged_class_from_main_is_not_the_cards() {
        let main = class("Old", "old");
        let werk = format!("{}{}", class("Old", "old"), class("New", "new"));
        assert_eq!(changed_classes(&werk, &main), vec!["<urn:x#New>".to_string()]);
    }

    #[test]
    fn a_class_whose_triples_changed_is_the_cards() {
        assert_eq!(changed_classes(&class("Old", "renamed"), &class("Old", "old")), vec!["<urn:x#Old>".to_string()]);
    }

    /// NEGATIVE PROOF: with no main version every declared class is the card's,
    /// so a new unshaped class can never hide behind the diff.
    #[test]
    fn negative_proof_a_new_file_answers_for_every_class() {
        let werk = format!("{}{}", class("A", "a"), class("B", "b"));
        assert_eq!(changed_classes(&werk, "").len(), 2);
    }
}
