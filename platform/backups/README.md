# platform/backups/

Restore artifacts written by destructive operations that the substrate refuses
to perform without one.

`graph-retirements/` — n-triples dumps taken by chorus-model-deploy's retirement
section (#3752) immediately before a graph is dropped, and by hand before a
bounded default-graph clear (#3732). The deploy REFUSES the drop if the backup
is short of the live triple count, so a file here is a verified restore path,
not a hopeful one.

Restore: `curl -u <cred> -X POST -H 'Content-Type: application/n-triples' \
  --data-binary @<file> 'http://localhost:3030/pods/data?graph=<graph-iri>'`

These are committed deliberately. A backup that lives only on one machine is not
a restore path — the 2026-05-30 lesson (destructive op run without one) is why
the guard exists at all.
