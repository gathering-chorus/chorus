// @domain: cicd — placed by chorus-crawl's domain rules (#4538)
// #4771 — the ONE place a werk verb finds a card's trace carrier file
// (ADR-032 §3: CHORUS_TRACE_ID env → <dir>/<card>-trace → mint+persist).
// <dir> is $CHORUS_TRACE_DIR, else /tmp (unchanged in prod). Every reader and
// writer goes through here, so moving the dir can never split a card's trace.
// BOUNDARY (shared/, #3513): pure, dep-free, fully-qualified paths.

pub fn trace_carrier_dir() -> std::path::PathBuf {
    std::env::var_os("CHORUS_TRACE_DIR")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
}

pub fn trace_carrier(card: u64) -> std::path::PathBuf {
    trace_carrier_dir().join(format!("{}-trace", card))
}
