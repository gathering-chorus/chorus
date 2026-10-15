//! #4530 — a demo hooks daemon must be able to bind its socket. macOS caps a
//! unix socket path at 103 bytes; abby-normal-4530's was 105 and env-up timed
//! out (run 14, 2026-10-15 06:41, "path must be shorter than SUN_LEN").
use werk_deploy::demo_env::hooks_socket_path;

const MAX: usize = 103;
const WERKS: &str = "/Users/jeffbridwell/CascadeProjects/chorus-werk";

#[test]
fn hooks_socket_fits_the_longest_role() {
    let p = hooks_socket_path(&format!("{WERKS}/abby-normal-99999"));
    assert!(p.len() <= MAX, "{} bytes: {p}", p.len());
}

