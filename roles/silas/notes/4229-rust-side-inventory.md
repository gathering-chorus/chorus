# #4229 — what the Rust verb already covers

Silas, 2026-09-20. Read from `platform/services/athena-deploy/src/` at
commit 90488c29f. Wren has the bash side; this is the other list to diff
against it.

The Rust verb is 379 lines against the bash's 1,482, and the gap is not
polish. It does one graph, with four legs.

## Carried today

| leg | where | note |
|---|---|---|
| model set assembly | `model_set`, lib.rs:92 | **two files**, or one via `TTL=` |
| riot validate per member | `run_athena_deploy`, lib.rs:182 | skipped silently when riot is absent |
| stage → additive merge → drop staging | lib.rs:195–219 | same DELETE-staged-subjects-then-INSERT as the bash |
| Fuseki credential at one door | `fuseki_auth`, lib.rs:128 | names the missing-credential case, which the bash does not |

## Only in the Rust verb

Two subcommands the bash never had. They are not deploy legs and should
survive the merge untouched.

- `scope <root> <range>` — did this land touch the model or the seed
- `prove-trace <trace> <want>` — the run's own legibility, read from the
  spine tail rather than the whole 2.5 GB file

## Three differences that would bite on the day of the swap

**It deploys two files where the bash deploys forty-one.** `model_set`
returns `chorus.ttl` and `werk-domains.ttl`. The bash names 41 distinct
`.ttl` paths across its MODEL_SET and its six instance sets. Pointing the
callers at the Rust verb today would silently stop deploying thirty-nine
files, with no error — the run would report success.

**Its verify cannot fail the way the bash's can.** The Rust check is
`ASK { GRAPH <ontology> { ?s ?p ?o } }` — is the graph non-empty. The bash
asks how many staged subjects are *absent* from the live graph after the
merge and refuses on any. A merge that dropped every staged subject passes
the Rust check as long as one unrelated triple remains. That is a check
that cannot distinguish the two states it exists to separate, so it does
not carry as-is.

**The events have different names.** Rust emits `athena.deployed` /
`athena.deploy.failed`; the bash emits `model.deployed` /
`model.deploy.failed`. Anything watching the bash's names goes blind at
the swap. One pair must win, and the other must keep being emitted until
every reader is repointed.

## Not in the Rust verb at all

Every one of these is bash-only. Each needs a carried-or-retired decision
with a reason, per AC1.

- owner-authoring guard (#4125) — a source file may not author a role as an owner
- source-delete guard (#4125) — a subject deleted from source is named, not silently kept
- staged retirements (#3752), three kinds: subject, class, whole-graph
- whole-graph retirement's verified backup (#3732) — CONSTRUCT to n-triples, refuse the drop when the dump has fewer lines than the graph
- SHACL report-only (#3536 AC2)
- `RETIRE_ABSENT` (default 0) — the only destructive ontology leg
- SECURITY_SET (#3726), INFRA_SET (#4084), PRINCIPLES_SET (#3749),
  VALUES_SET (#4006), SERVICES_SET (#4010), PRACTICES_SET (#3754) — six
  more staged merges, each with its own graph and its own verify
- `TTL=` partial mode gating the six sets off
- per-set spine events carrying the graph and the live count

## A correction to the card

The card says the unload before a delete "ends in `|| true`, so a failed
dump does not stop the delete." Measured: the `|| true` is on the CONSTRUCT
only, and the line-count check on the next line refuses the drop when the
dump is short or empty (`athena-deploy-model.sh:756`). The whole-graph leg
is the one delete path that is safe.

The real gap is the other two. Subject retirement issues
`DELETE WHERE { GRAPH ?g { <subject> ?p ?o } }` and class retirement its
own delete, and **neither takes a backup at all**. Subject retirement is
the leg #4216's 226 rows went through.

## The eight set legs are not the same leg (Wren, measured 2026-09-20)

| leg | verify checks | refusals |
|---|---|---|
| `stage_merge_set()` and the 3 that use it | 3 | 8 |
| VALUES | 3 | 8 |
| PRINCIPLES, SERVICES, PRACTICES, INFRA | 2 | 6 |

Four of the eight are one verify and two refusals short of the others.
Nobody chose that — it is what happens when a leg is copied five times and
only some copies get the later fixes.

This is the argument for driving the port from a manifest rather than
collapsing the copies in bash first. One implementation, eight lines of
data, and the four weak legs get the stronger checks by construction.

**Named decision, not a discovery:** collapsing UPGRADES those four legs.
PRINCIPLES, SERVICES, PRACTICES and INFRA will begin refusing on two
conditions they currently let through. If any of them is refusing after
the port, that is the new check working, not a regression — and each of
the eight refusals ships with a fixture that violates it and is watched
failing (#3734).
