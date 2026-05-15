//! Deploy probe check (#2925 AC3)
//!
//! PreToolUse on `mcp__chorus-api__chorus_cards_add` — inspects the new card's
//! description for daemon-runtime trigger paths. If the card touches
//! daemon-runtime files (platform/api/src/, mcp/server.ts, platform/services/
//! chorus-hooks/) but is missing a `## Deploy Probe` section, refuse the add.
//!
//! Why: daemon-runtime cards must ship through deploy-daemon-card.sh, which
//! requires a smoke probe. The probe verifies the new behavior is live in
//! the restarted daemon. A daemon-runtime card without a probe is undeployable
//! through the sanctioned path.

use crate::types::{HookInput, HookResponse};
use tracing::info;

const DAEMON_RUNTIME_TRIGGERS: &[&str] = &[
    "platform/api/src/",
    "mcp/server.ts",
    "platform/services/chorus-hooks/",
];

pub async fn check(input: &HookInput) -> HookResponse {
    let description = input.get_tool_input_str("description");
    match classify(&description) {
        ProbeCheck::Allow => HookResponse::allow(),
        ProbeCheck::Block(reason) => {
            info!(
                reason = %reason,
                "card-add-probe: blocking daemon-runtime card without probe"
            );
            HookResponse::block_with_stderr(&format!(
                "card-add refused: this card claims to touch daemon-runtime paths \
                 but is missing a '## Deploy Probe' section.\n\n\
                 {reason}\n\n\
                 Daemon-runtime cards must ship through deploy-daemon-card.sh, \
                 which requires a smoke probe verifying the new behavior is live \
                 in the restarted daemon. Add a '## Deploy Probe' section to the \
                 description with a shell command that will verify the change, \
                 e.g.:\n\n## Deploy Probe\n\n\
                 curl -sf http://localhost:3340/some-endpoint | grep -q expected"
            ))
        }
    }
}

#[derive(Debug, PartialEq)]
enum ProbeCheck {
    Allow,
    Block(String),
}

fn classify(description: &str) -> ProbeCheck {
    let touched: Vec<&str> = DAEMON_RUNTIME_TRIGGERS
        .iter()
        .copied()
        .filter(|t| description.contains(*t))
        .collect();
    if touched.is_empty() {
        return ProbeCheck::Allow;
    }
    if description.contains("## Deploy Probe") {
        return ProbeCheck::Allow;
    }
    ProbeCheck::Block(format!(
        "Detected daemon-runtime path(s) in description: {}",
        touched.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_non_daemon_runtime_card() {
        let desc = "## Experience\nA simple bug fix in docs.\n## Files\n- docs/readme.md";
        assert_eq!(classify(desc), ProbeCheck::Allow);
    }

    #[test]
    fn block_daemon_runtime_card_without_probe() {
        let desc = "## Experience\nChanges to chorus-api MCP.\n## Files\n- platform/api/src/mcp/server.ts";
        match classify(desc) {
            ProbeCheck::Block(reason) => assert!(reason.contains("platform/api/src/")),
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn allow_daemon_runtime_card_with_probe() {
        let desc = "## Files\n- platform/api/src/foo.ts\n## Deploy Probe\ncurl -sf localhost:3340/x | grep ok";
        assert_eq!(classify(desc), ProbeCheck::Allow);
    }

    #[test]
    fn detects_chorus_hooks_trigger() {
        let desc = "## Files\n- platform/services/chorus-hooks/src/hooks/foo.rs";
        match classify(desc) {
            ProbeCheck::Block(reason) => assert!(reason.contains("chorus-hooks")),
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn detects_mcp_server_ts_trigger() {
        let desc = "## Files\n- mcp/server.ts changes";
        match classify(desc) {
            ProbeCheck::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn probe_section_recognized_anywhere_in_description() {
        let desc = "## Files\n- platform/api/src/foo.ts\n\n## Other\nstuff\n\n## Deploy Probe\nprobe-cmd";
        assert_eq!(classify(desc), ProbeCheck::Allow);
    }

    #[test]
    fn empty_description_allows() {
        assert_eq!(classify(""), ProbeCheck::Allow);
    }

    #[test]
    fn multiple_triggers_listed_in_block_reason() {
        let desc = "## Files\n- platform/api/src/x\n- mcp/server.ts";
        match classify(desc) {
            ProbeCheck::Block(reason) => {
                assert!(reason.contains("platform/api/src/"));
                assert!(reason.contains("mcp/server.ts"));
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }
}
