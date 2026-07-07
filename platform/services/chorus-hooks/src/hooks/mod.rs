pub mod jdi_detector;
pub mod card_approval_responder;
pub mod autonomy_guard;
pub mod bedroom_nfs_guard;
pub mod clock_sync;
pub mod handoff_logger;
pub mod infra_guardrails;
pub mod observer;
pub mod search_hierarchy;
pub mod sensitive_paths;
pub mod session_init_gate;
pub mod sparql_guard;
pub mod tool_telemetry;
pub mod story_write_gate;
pub mod write_scrubber;
pub mod icd_write_gate;
// #3046: demo_preflight / demo_show / demo_provenance / demo_gate retired —
// the /demo skill is now a thin wrapper around the `werk-demo` binary, which
// posts the demo:preflight-pass evidence comment + emits the demo.show.completed
// spine event directly. accept_gate still reads those (it's the consumer).
pub mod accept_gate;
pub mod pair_enforcement;
pub mod nudge_blast_radius;
pub mod input_classifier;
pub mod csc_guard;
pub mod icd_pre_read;
pub mod nifi_discipline;
pub mod memory_gate;
pub mod tdd_gate;
pub mod test_quality_gate;
pub mod pair_gate;
pub mod context_inject;
pub mod inject_force; // #3203 — Stop-hook engagement verdict (forcing pattern / HIP-001)
pub mod stop_on_error;
// #3000 — mcp_health_gate retired (wrong-layer, never fired for MCP tools).
// Server-side capture in chorus-mcp replaces it. Source file deleted.
pub mod log_first_gate;
pub mod quality_gate;
pub mod memory_first;
pub mod e2e_responder;
pub mod interaction_pattern;
pub mod chrome_tab_gate;
pub mod nudge_poll;
pub mod nudge_drain; // #3218 — reliable FIFO drain off messages.db (PreToolUse + Stop)
pub mod canonical_write_guard;
pub mod card_add_probe;
pub mod memory_pressure_guard; // #3625 — refuse Task/Agent fanout under memory pressure
