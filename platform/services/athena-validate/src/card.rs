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
        for class in card_scope::declared_classes(&nt) {
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
