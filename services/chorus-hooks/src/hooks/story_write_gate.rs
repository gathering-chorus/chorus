use crate::state::chorus_log;
use crate::types::{HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

/// Blocks story content from being written to markdown files.
/// Redirects roles to use scripts/write-story.sh for TTL write path.
/// Card #1562: Stories write-path enforcement

static STORY_MARKER_SAID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)what he said").unwrap()
});

static STORY_MARKER_TELLS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)what it tells us").unwrap()
});

static STORY_MARKER_APPLIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)where it applies").unwrap()
});

pub async fn check(input: &HookInput) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Write" && tool != "Edit" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() || !file_path.ends_with(".md") {
        return HookResponse::allow();
    }

    let content = if tool == "Write" {
        input.get_tool_input_str("content")
    } else {
        input.get_tool_input_str("new_string")
    };

    if content.is_empty() {
        return HookResponse::allow();
    }

    // Check 1: Direct write to stories.md with story markers
    let is_stories_file = file_path.to_lowercase().ends_with("stories.md");
    if is_stories_file && STORY_MARKER_SAID.is_match(&content) {
        chorus_log(
            "guard.story.blocked",
            "system",
            &[("file", &file_path)],
        )
        .await;
        return HookResponse::block_with_stderr(
            "BLOCKED: New stories must go to TTL, not markdown.\n\
             Use: bash scripts/write-story.sh \"Title\" \"What he said\" \"What it tells us\" \"Where it applies\"\n\
             stories.md is deprecated for new stories (card #1562)."
        );
    }

    // Check 2: Story content in any markdown file (all three markers required)
    if STORY_MARKER_SAID.is_match(&content)
        && STORY_MARKER_TELLS.is_match(&content)
        && STORY_MARKER_APPLIES.is_match(&content)
    {
        chorus_log(
            "guard.story.blocked",
            "system",
            &[("file", &file_path)],
        )
        .await;
        return HookResponse::block_with_stderr(
            "BLOCKED: This looks like a story. Stories go to TTL, not markdown.\n\
             Use: bash scripts/write-story.sh \"Title\" \"What he said\" \"What it tells us\" \"Where it applies\""
        );
    }

    HookResponse::allow()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_input(path: &str, content: &str) -> HookInput {
        serde_json::from_value(json!({
            "tool_name": "Write",
            "tool_input": {"file_path": path, "content": content},
            "cwd": "/tmp"
        })).unwrap()
    }

    #[tokio::test]
    async fn allows_normal_markdown() {
        let r = check(&write_input("/tmp/notes.md", "Some normal content")).await;
        assert_eq!(r.exit_code, 0);
        assert!(r.stderr.is_none());
    }

    #[tokio::test]
    async fn blocks_stories_md_with_marker() {
        let r = check(&write_input("/tmp/stories.md", "## A Story\nWhat he said: blah")).await;
        assert_eq!(r.exit_code, 2);
        assert!(r.stderr.unwrap().contains("BLOCKED"));
    }

    #[tokio::test]
    async fn blocks_three_markers_any_md() {
        let content = "What he said: x\nWhat it tells us: y\nWhere it applies: z";
        let r = check(&write_input("/tmp/briefs/test.md", content)).await;
        assert_eq!(r.exit_code, 2);
    }

    #[tokio::test]
    async fn allows_partial_markers() {
        let r = check(&write_input("/tmp/test.md", "What he said: something")).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn allows_non_md_files() {
        let content = "What he said: x\nWhat it tells us: y\nWhere it applies: z";
        let r = check(&write_input("/tmp/test.txt", content)).await;
        assert_eq!(r.exit_code, 0);
    }

    #[tokio::test]
    async fn ignores_non_write_tools() {
        let input: HookInput = serde_json::from_value(json!({
            "tool_name": "Bash",
            "tool_input": {"command": "echo hello"},
            "cwd": "/tmp"
        })).unwrap();
        let r = check(&input).await;
        assert_eq!(r.exit_code, 0);
    }
}
