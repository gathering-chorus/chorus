## Fuseki & SPARQL Reference

**Fuseki port is 3030 everywhere.** Host, container, scripts — always `http://localhost:3030`. Inside Docker network: `http://fuseki:3030`. One port, no translation.

**Graph URIs use `http://localhost:3000/pods/jeff/<domain>/` prefix. Never `https://jeffbridwell.com/`.** All domains — music, photos, media, stories, notes, blog, ontology — use the localhost scheme. `graph-lint.sh` check #1 enforces this.

**SPARQL working pattern:**
- **Dataset**: `/pods` — NOT `/jeff`, NOT `/ds`, NOT `/dataset`
- **Query endpoint**: `http://localhost:3030/pods/query` (GET with URL-encoded `query` param)
- **Inside Docker**: `http://fuseki:3030/pods/query`

```bash
curl -s 'http://localhost:3030/pods/query' -H 'Accept: text/csv' -G \
  --data-urlencode 'query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT ...'
```

**Common prefixes:** `jb:` = `https://jeffbridwell.com/ontology#`, `dcterms:` = `http://purl.org/dc/terms/`, `schema:` = `https://schema.org/`
**Key types:** `jb:Track`, `jb:Artist`, `jb:Album`, `jb:BlogPost`, `jb:CaptureItem`, `jb:Story`
**Key properties:** `jb:sourceFilePath`, `jb:playCount`, `jb:byArtist`, `jb:inAlbum`, `jb:harvestedIn`
**Graph filter:** `GRAPH ?g { ... } FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/<domain>/"))`
**Do NOT:** POST to `/pods/sparql` (405), use wrong dataset name (404), run SPARQL during demos

## Canonical Script Paths

| Script | Canonical invocation |
|--------|---------------------|
| board-ts | `bash ../messages/scripts/board-ts <command>` |
| git-queue.sh | `cd /Users/jeffbridwell/CascadeProjects && DEPLOY_ROLE=<role> bash messages/scripts/git-queue.sh commit <dirs> -- -m "message"` |
| role-state.sh | `../messages/scripts/role-state.sh <role> <state>` |
| nudge.sh | `bash ../messages/scripts/nudge.sh <target> "message" --from <sender>` |
| chorus-log.sh | `bash ../messages/scripts/chorus-log.sh <event> <role> key=value` |
| werk-init.sh | `bash ../messages/scripts/werk-init.sh <role> [--close]` |

**Never use:** `/Users/jeffbridwell/CascadeProjects/chorus/scripts/nudge.sh` (stale copy), `gathering-team/messages/scripts/` (wrong repo name).
