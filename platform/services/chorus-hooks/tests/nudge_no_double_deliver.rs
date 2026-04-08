//! #1811: Successful inject must not leave message in queue.
//!
//! Root cause: nudge.rs called queue_message() unconditionally BEFORE
//! inject_by_tab_name(). On success, both the inject AND the PostToolUse
//! drain delivered the same message — every nudge arrived twice.
//!
//! Fix: try inject first. Only queue on failure (fallback for drain).

#[test]
fn inject_path_does_not_queue_before_inject() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/nudge.rs"
    ).expect("nudge.rs should exist");

    // The old bug: queue_message was called before the inject attempt,
    // unconditionally inside the `else` (non-dry-run) branch.
    // Look for the pattern: queue_message followed by inject_by_tab_name
    // with no conditional between them.
    let queue_pos = source.find("queue_message(target,");
    let inject_pos = source.find("inject_by_tab_name(target,");

    // Both must exist
    assert!(queue_pos.is_some(), "queue_message call should exist in nudge.rs");
    assert!(inject_pos.is_some(), "inject_by_tab_name call should exist in nudge.rs");

    // Fix verification: queue_message must come AFTER inject_by_tab_name,
    // not before. In the fixed code, queue only appears in the Err branch
    // (inject failed), so its position in the source is after the inject call.
    let queue_pos = queue_pos.unwrap();
    let inject_pos = inject_pos.unwrap();
    assert!(
        queue_pos > inject_pos,
        "queue_message must come AFTER inject_by_tab_name (only queue on inject failure). \
         Found queue at byte {}, inject at byte {}. \
         Fix: move queue_message into the Err arm of inject_by_tab_name match.",
        queue_pos, inject_pos
    );
}

#[test]
fn queue_only_in_inject_error_branch() {
    let source = std::fs::read_to_string(
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/nudge.rs"
    ).expect("nudge.rs should exist");

    // In the fixed code, queue_message(target, ...) should only appear
    // inside an Err branch or in the non-inject paths (warn/info level).
    // It must NOT appear as a standalone call before the inject match.

    // Find the critical/force block
    let force_block_start = source.find("if level == \"critical\" || force {")
        .expect("force branch should exist");
    let force_block = &source[force_block_start..];

    // Within the force block, queue_message should not appear before inject_by_tab_name
    let queue_in_force = force_block.find("queue_message(target,");
    let inject_in_force = force_block.find("inject_by_tab_name(target,");

    if let (Some(q), Some(i)) = (queue_in_force, inject_in_force) {
        assert!(
            q > i,
            "In the force/critical block, queue_message must come after inject_by_tab_name \
             (only queue as fallback on inject failure)"
        );
    }
    // If queue_message doesn't appear in the force block at all, that's also valid
    // (means it was fully removed from the inject-success path)
}
