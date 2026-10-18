// @test-type: unit
// @domain: pipelines
//! #4502 — the demo's 5 gates and 2 peer reviews run in parallel: rows that
//! share a step and an order are siblings. Each sibling depends on the whole
//! group before it; the next row depends on every sibling. Rows with distinct
//! orders still make the linear chain, byte for byte.
use chorus_make::{generate, groups, parse_rows};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/{name}")).expect("fixture")
}

const SIBLINGS: [&str; 7] = [
    "skill-demo-gate-arch", "skill-demo-gate-code", "skill-demo-gate-ops", "skill-demo-gate-product",
    "skill-demo-gate-quality", "skill-demo-review-peer-1", "skill-demo-review-peer-2",
];

/// validate (order 1) → 7 siblings (order 2) → announce (order 3) → Jeff's go (order 4)
fn demo_rows() -> String {
    let mut tsv = String::from("demo\t4\t1\tskill-demo-validate\tvalidate\tdeterministic\twerk-demo\tvalidate {card} {caller}\t\t\t\n");
    for s in SIBLINGS {
        tsv.push_str(&format!("demo\t4\t2\t{s}\t{s}\tdeterministic\twerk-demo\trun\t\t\t\n"));
    }
    tsv.push_str("demo\t4\t3\tskill-demo-announce\tannounce\tdeterministic\twerk-demo\tannounce {card} {caller}\t\t\t\n");
    tsv.push_str("demo\t4\t4\tskill-werk-go\tJeff's go\thuman\t\t\t\t\t\n");
    tsv
}

fn step<'a>(out: &'a str, name: &str) -> &'a str {
    out.split("  - name: ").find(|s| s.starts_with(&format!("{name}\n"))).unwrap_or_else(|| panic!("no step {name} in:\n{out}"))
}

#[test]
fn seven_siblings_each_wait_for_validate_and_announce_waits_for_all_seven() {
    let rows = parse_rows(&demo_rows()).expect("rows parse");
    let out = generate("cicd", &rows).expect("siblings are allowed");
    for s in SIBLINGS {
        assert!(step(&out, s).contains("    depends: [skill-demo-validate]\n"), "{s} waits for validate only:\n{}", step(&out, s));
    }
    let all = SIBLINGS.join(", ");
    assert!(step(&out, "skill-demo-announce").contains(&format!("    depends: [{all}]\n")), "announce waits for all 7:\n{out}");
    assert!(step(&out, "skill-werk-go").contains("    depends: [skill-demo-announce]\n"), "the go follows announce:\n{out}");
    assert_eq!(groups(&rows_sorted(&rows)).iter().map(|g| g.len()).collect::<Vec<_>>(), vec![1, 7, 1, 1]);
}

// the same rows in any input order give the same bytes
#[test]
fn siblings_generate_the_same_bytes_in_any_row_order() {
    let rows = parse_rows(&demo_rows()).unwrap();
    let mut shuffled = rows.clone();
    shuffled.reverse();
    assert_eq!(generate("cicd", &rows).unwrap(), generate("cicd", &shuffled).unwrap());
}

// NEGATIVE PROOF: distinct orders are the linear chain — the cicd golden is
// unchanged byte for byte, and every step in it has at most one dependency.
#[test]
fn negative_proof_distinct_orders_stay_the_linear_golden() {
    let rows = parse_rows(&fixture("fixtures/cicd-rows.tsv")).unwrap();
    let out = generate("cicd", &rows).unwrap();
    assert_eq!(out, fixture("fixtures/cicd-golden.yaml"));
    assert!(out.lines().filter(|l| l.starts_with("    depends: [")).all(|l| !l.contains(", ")), "no fan-in:\n{out}");
}

fn rows_sorted(rows: &[chorus_make::Row]) -> Vec<chorus_make::Row> {
    let mut r = rows.to_vec();
    r.sort_by(|a, b| (a.step_order, &a.step, a.skill_order, &a.skill).cmp(&(b.step_order, &b.step, b.skill_order, &b.skill)));
    r
}
