---
name: design-refresh
description: Refresh the cite-density layers of a service-design HTML from current card statuses. Substrate-enforced; skill body is one MCP call.
user-invocable: true
---

# /design-refresh — Refresh service-design cite-density layers

Invoke `chorus_design_refresh` with the design's filename stem. Print the result.

```
/design-refresh build-and-deploy-service-design
→ mcp__chorus-api__chorus_design_refresh({ role: "<your-role>", design_name: "build-and-deploy-service-design" })
```

That's the entire skill. The MCP enforces template compliance (refuses `summary-missing` if the design lacks the mandatory `<div class="summary-block">` skim layer), pulls current Done/WIP/Next/Later/Won't-Do status for every `#NNNN` reference in the doc, and regenerates the `data-section`-tagged headings (References, Path-to-close, Gaps) without touching human-authored sections (Summary block, Overview, As-Is, To-Be, per-domain blocks).

Refusal taxonomy: `design-not-found | template-violation | summary-missing | manifest-missing | regenerate-fail`. Each refusal carries the specific section / detail that triggered it. See the MCP tool description for the full return shape (`sections_regenerated`, `cards_referenced`, `diff_lines`, `cards_by_status`).

If a design doesn't yet match the canonical template (no summary block), the first refresh refuses with `summary-missing`. Author the summary block manually using `designing/templates/service-design.html` as the reference, then re-run.

If `chorus-api` is unreachable, escalate to ops. Do not improvise raw HTML edits to regenerate cite-density sections — PreToolUse hooks (#2900 follow-on) will refuse those subprocess paths from agent sessions once shipped.
