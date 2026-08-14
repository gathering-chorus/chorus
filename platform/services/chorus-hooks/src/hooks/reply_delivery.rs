//! #3864 — reply-delivery correlation, emit leg.
//!
//! Jeff, 2026-08-13: "we should be able to correlate delivery to both claude
//! code and clearing." The spine already chains nudges (requested → emitted →
//! surfaced) and Jeff's inbound input (delivered/surfaced/failed); role
//! REPLIES — the surface Jeff actually reads — emit nothing. This module is
//! the missing first event: `reply.emitted`, stamped at the Stop hook the
//! moment a final reply stands (the transcript IS Claude Code delivery).
//! Clearing's tailer stamps `reply.rendered` with the same content hash;
//! pulse joins the pair and fires `reply.delivery.gap` when the render never
//! comes.
//!
//! The hash is the cross-language join key, contract in
//! `config/reply-hash-fixtures.json` (the word-cap-fixtures pattern). The two
//! sides read DIFFERENT bytes for the same reply — this extractor joins text
//! blocks with '\n' and keeps the chorus header; the tailer joins with spaces
//! and strips it — so the hash canonicalizes both to one form: strip one
//! leading `--- ... ---` header, collapse whitespace runs, trim, then
//! sha256 hex[..16].

use sha2::{Digest, Sha256};

/// Canonical form both surfaces can reach from their own bytes.
fn canonicalize(text: &str) -> String {
    // (1) strip one LEADING chorus header: optional whitespace, '---',
    // anything (incl. newlines) up to the next '---', trailing whitespace.
    let t = text.trim_start();
    let stripped: &str = if let Some(rest) = t.strip_prefix("---") {
        match rest.find("---") {
            Some(i) => rest[i + 3..].trim_start(),
            None => t,
        }
    } else {
        t
    };
    // (2) collapse every whitespace run to a single space; (3) trim.
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Join key for one reply across surfaces. sha256(canonicalize(text)), hex[..16].
pub fn content_hash(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(canonicalize(text).as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex[..16].to_string()
}
