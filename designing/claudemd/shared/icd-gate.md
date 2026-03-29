## ICD Gate (DEC-095)

No harvester code without a matching ICD provider section. The `icd-gate-hook.sh` PreToolUse hook enforces this — blocks writes to `*-harvester.service.ts` and `harvest-*.sh` if the domain has no provider in `urn:gathering:icd/current`. To add a new domain: populate `icd-instance-{domain}.ttl` first, then write harvester code.

Reference templates: `architect/reference-templates.md`
