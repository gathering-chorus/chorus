//! #2504 — file retired. See nudge.rs:304-308 for the #2435 atomic cutover
//! that deleted queue_message() + inject_by_tab_name() — the functions this
//! suite source-grepped for. The double-delivery regression is structurally
//! impossible now (no queue, no sender-side inject); guarding against it
//! by string-grep on retired symbols is dead code.
//!
//! Origin: #1811 (Silas, 77eb822c) — duplicate-nudge fix; updated #2283
//! (Silas, 45596a6f) for the nudge consolidation; missed in #2435 cleanup.
//!
//! Kept as an empty file (rather than deleted) so git-queue's
//! atomic-add-and-commit can serialize the change cleanly.
