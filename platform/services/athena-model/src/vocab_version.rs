//! vocab_version (#3902) — semver for the model's vocabulary, bumped AT THE PEN.
//!
//! Jeff, 2026-08-15/16: "are our owl and athena-make domains semantically
//! versioned?" → probe said no (two stale hand-stamps, envelopes serving a
//! content hash). The contract this module implements: athena-model DECLARES
//! versions — every TBox write classifies itself and bumps here — and
//! athena-make (athena-make) IMPLEMENTS them by projecting the version it serves.
//!
//! One home, ledger-shaped (the #3288 claudemd pattern, deliberately copied):
//! designing/schemas/model-version-ledger.jsonl is committed and append-only;
//! each entry pins {version, bump, verb, target, by, card, date}. The head is
//! ALSO rendered into designing/data/model-version.ttl (a GENERATED artifact in
//! the MODEL_SET) so the store carries `chorus:vocabVersion` and athena-make can
//! serve it. Hand-editing either file is detectable: gc-vocab-versioned
//! compares the SERVED version against the ledger head — mismatch is a red.
//!
//! Classification is deterministic, never judgment:
//!   retire (class/property leaves the vocabulary)      → MAJOR
//!   add class / add property / add-or-tighten shape    → MINOR
//!   (docs/labels ride along; the pen has no patch-only verb today)

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    Major,
    Minor,
}

impl Bump {
    pub fn label(&self) -> &'static str {
        match self {
            Bump::Major => "major",
            Bump::Minor => "minor",
        }
    }
}

/// Deterministic classification by verb — the pen's verbs map to exactly one
/// bump class each. A new verb MUST be added here or the compiler refuses.
pub fn classify(verb: &str) -> Option<Bump> {
    match verb {
        "retire" => Some(Bump::Major),
        "class" | "property" | "shape" => Some(Bump::Minor),
        _ => None,
    }
}

pub fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().split('.');
    let x = it.next()?.parse().ok()?;
    let y = it.next()?.parse().ok()?;
    let z = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((x, y, z))
}

pub fn bump_semver(v: &str, bump: Bump) -> Option<String> {
    let (x, y, z) = parse_semver(v)?;
    let _ = z;
    Some(match bump {
        Bump::Major => format!("{}.0.0", x + 1),
        Bump::Minor => format!("{}.{}.0", x, y + 1),
    })
}

fn ledger_path(root: &str) -> String {
    format!("{}/designing/schemas/model-version-ledger.jsonl", root)
}

fn ttl_path(root: &str) -> String {
    format!("{}/designing/data/model-version.ttl", root)
}

/// Read the ledger head version. No ledger yet = the epoch seed "1.0.0"
/// (versioning starts the day the pen starts declaring — #3902).
pub fn head_version(root: &str) -> String {
    let p = ledger_path(root);
    let body = match fs::read_to_string(&p) {
        Ok(b) => b,
        Err(_) => return "1.0.0".to_string(),
    };
    // last entry's "version" — the ledger is append-only JSON lines inside an
    // entries array; parse defensively (a corrupt ledger must fail LOUD in
    // record(), but a read for display degrades to the seed).
    body.rfind("\"version\"")
        .and_then(|i| body[i..].split('"').nth(3).map(str::to_string))
        .unwrap_or_else(|| "1.0.0".to_string())
}

/// Append a ledger entry and re-render the TTL projection. Returns the new
/// version. Fail-loud: a pen write whose version bump cannot be recorded must
/// not pretend it was.
pub fn record(root: &str, verb: &str, target: &str, by: &str, card: &str) -> Result<String, String> {
    let bump = classify(verb).ok_or_else(|| format!("vocab-version: unclassifiable verb '{verb}' — add it to classify()"))?;
    let prev = head_version(root);
    let next = bump_semver(&prev, bump).ok_or_else(|| format!("vocab-version: ledger head '{prev}' is not semver"))?;

    let lp = ledger_path(root);
    // JSONL, appended zero-dep (ADR-032 discipline: the verbs carry no JSON
    // crate). Corruption check: every existing line must look like an entry —
    // refuse to append after a line that does not.
    if let Ok(body) = fs::read_to_string(&lp) {
        for (i, line) in body.lines().enumerate() {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if !(l.starts_with('{') && l.ends_with('}') && l.contains("\"version\"")) {
                return Err(format!(
                    "vocab-version: ledger line {} unparseable — refusing to append blind",
                    i + 1
                ));
            }
        }
    }
    if let Some(dir) = Path::new(&lp).parent() {
        fs::create_dir_all(dir).map_err(|e| format!("vocab-version: mkdir {}: {e}", dir.display()))?;
    }
    let date = date_stamp();
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let entry = format!(
        "{{\"version\": \"{}\", \"bump\": \"{}\", \"verb\": \"{}\", \"target\": \"{}\", \"by\": \"{}\", \"card\": \"{}\", \"date\": \"{}\"}}\n",
        next, bump.label(), esc(verb), esc(target), esc(by), esc(card), date
    );
    use std::io::Write;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&lp)
        .map_err(|e| format!("vocab-version: ledger open failed: {e}"))?;
    f.write_all(entry.as_bytes())
        .and_then(|_| f.flush())
        .map_err(|e| format!("vocab-version: ledger write failed: {e}"))?;

    // GENERATED projection — rides the MODEL_SET so the STORE carries the
    // version and athena-make serves it. Rewritten whole each time (generated
    // artifact: rewriting is correct here, unlike the hand-curated model files).
    let ttl = format!(
        "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix owl:    <http://www.w3.org/2002/07/owl#> .\n\n\
# GENERATED by athena-model (vocab_version.rs, #3902) — do not hand-edit.\n\
# One home: designing/schemas/model-version-ledger.jsonl; this file is its\n\
# rendered projection into the store. gc-vocab-versioned reds on any mismatch\n\
# between the SERVED version and the ledger head (hand-edit detector).\n\
chorus:model a owl:Ontology ;\n    chorus:vocabVersion \"{next}\" .\n"
    );
    let tp = ttl_path(root);
    if let Some(dir) = Path::new(&tp).parent() {
        fs::create_dir_all(dir).map_err(|e| format!("vocab-version: mkdir {}: {e}", dir.display()))?;
    }
    fs::write(&tp, ttl).map_err(|e| format!("vocab-version: ttl projection write failed: {e}"))?;
    Ok(next)
}

fn date_stamp() -> String {
    // Boston-offset date via the system clock; seconds precision is enough for
    // a ledger a human reads.
    use std::process::Command;
    Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_is_total_over_pen_verbs() {
        assert_eq!(classify("retire"), Some(Bump::Major));
        assert_eq!(classify("class"), Some(Bump::Minor));
        assert_eq!(classify("property"), Some(Bump::Minor));
        assert_eq!(classify("shape"), Some(Bump::Minor));
        assert_eq!(classify("seed"), None); // ABox writes never bump the vocabulary
    }

    #[test]
    fn negative_proof_retire_bumps_major_add_bumps_minor() {
        // #3902 AC negative proof: a breaking change (retire) → MAJOR;
        // an additive one → MINOR. Proven on the real record() path.
        let dir = std::env::temp_dir().join(format!("vocab-ver-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let v1 = record(root, "property", "hasThing", "silas", "3902").unwrap();
        assert_eq!(v1, "1.1.0", "additive bumps MINOR from the 1.0.0 epoch");
        let v2 = record(root, "retire", "Vertebra", "silas", "3902").unwrap();
        assert_eq!(v2, "2.0.0", "retire bumps MAJOR");
        assert_eq!(head_version(root), "2.0.0", "ledger head is the version");

        // and the projection carries the head:
        let ttl = std::fs::read_to_string(ttl_path(root)).unwrap();
        assert!(ttl.contains("chorus:vocabVersion \"2.0.0\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_ledger_refuses_loudly() {
        let dir = std::env::temp_dir().join(format!("vocab-corrupt-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        std::fs::create_dir_all(dir.join("designing/schemas")).unwrap();
        std::fs::write(dir.join("designing/schemas/model-version-ledger.jsonl"), "not json").unwrap();
        let e = record(root, "class", "X", "silas", "3902").unwrap_err();
        assert!(e.contains("refusing to append blind"), "{}", e);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
