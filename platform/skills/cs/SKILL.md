---
name: cs
description: Check seeds — query SPARQL for pending seeds, verify pipeline health, report status
user-invocable: true
---

# cs — Check Seeds

Run the seed pipeline check and assess any pending seeds.

## Step 1: Run the script

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/check-seeds.sh
```

This queries SPARQL for pending seeds, checks pipeline health (tunnel + app + Fuseki), and reports the last seed timestamp. The output is the answer — no further queries needed.

## Step 2: Assess pending seeds

If seeds are pending:
- For each seed, assess: what it is, why it matters, recommended route
- **Photo seeds:** When the output shows `Photo → http://...`, use the Read tool on the media URL to view the image. Describe what you see in 1-2 sentences — what's in the photo, not technical metadata. Jeff wants to know his seed arrived and someone looked at it.
- Route immediately: write a brief to the destination role's `briefs/` directory
- Discard silently: empty/test seeds (containing `[SEED-PROBE]`) get deleted without mention

If no seeds are pending, report the script output and move on.

## Step 3: Check local seed briefs

Glob for `briefs/*seed*.md` in the current role's directory. Read and assess any found.

## Rules

- Never ask "should I check seeds?" — just run the script.
- If pipeline is DOWN, say so. Never say "inbox clear" without a healthy pipeline.
- Empty/test seeds get discarded silently.
- If a seed connects to an open card or active work, call that out.
- Route seeds immediately — write brief to destination role's `briefs/`, delete original.
