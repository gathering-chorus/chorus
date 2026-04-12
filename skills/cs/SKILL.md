---
name: cs
description: Check seeds — read what Jeff sent, act on it
user-invocable: true
---

# cs — Check Seeds

Jeff sends seeds (SMS) every day. Your job is to **read them**, not count them.

## Step 1: Pipeline health

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/check-seeds.sh
```

If pipeline is DOWN, say so and stop. Otherwise continue.

## Step 2: Read every recent seed

Query SPARQL for the last 10 seeds with full content:

```bash
curl -sf --max-time 5 \
  -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?content ?type ?status ?created ?url ?media WHERE {
  GRAPH <urn:jb:seeds/> {
    ?seed jb:hasSeedType ?type ;
          jb:hasSeedStatus ?status .
    OPTIONAL { ?seed jb:seedContent ?content }
    OPTIONAL { ?seed jb:seededAt ?created }
    OPTIONAL { ?seed jb:seedUrl ?url }
    OPTIONAL { ?seed jb:seedMediaPath ?media }
  }
} ORDER BY DESC(?created) LIMIT 10" \
  "http://localhost:3030/pods/query"
```

**For every seed, regardless of status (pending OR routed):**

- **Photo seeds:** Download and view the image. Describe what you see. If it's actionable (like a security alert), act on it immediately.
  ```bash
  curl -sf http://localhost:3340/api/chorus/seed-media/<filename> -o ~/Desktop/seed-photo.png
  ```
  Then use the Read tool on the downloaded file.

- **Link seeds:** Fetch the URL and summarize what it's about. Note if it connects to current work.

- **Text seeds:** Read the full content. If it's a request, act on it.

**Do not skip routed seeds.** "Routed" means filed, not read. Read every one.

## Step 3: Act

- If a seed is a security alert → fix it now
- If a seed is relevant to your current work → note the connection
- If a seed is for another role → nudge that role with the content
- If a seed needs discussion → bring it to Jeff

## Step 4: Check local seed briefs

Glob for `briefs/*seed*.md` in the current role's directory. Read and assess any found.

## Rules

- **Read every seed.** Not just pending. Not just a count. Read them.
- Never say "inbox clear" without having read the recent seeds.
- If pipeline is DOWN, say so first.
- Empty/test seeds (containing `[SEED-PROBE]`) get discarded silently.
- Photo seeds MUST be opened and described.
- Link seeds MUST be fetched and summarized.
