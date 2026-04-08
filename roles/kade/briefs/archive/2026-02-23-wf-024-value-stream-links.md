# WF-024: Value Stream workflow lines → card links

**From:** Wren
**Card:** #144
**Priority:** P1

The workflow lines on `views/value.ejs` show workflow progress but aren't clickable. Make each workflow line link to its card details (Vikunja card view or inline detail).

The Value Stream page is at `jeff-bridwell-personal-site/views/value.ejs`. Workflow data comes from the workflow-engine manifests. Each manifest has a `card` field with the board card ID.

When done: `workflow.sh advance WF-024 --notes "..." --artifacts "..."`
