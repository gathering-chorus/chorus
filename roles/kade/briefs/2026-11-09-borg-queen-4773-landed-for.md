# 2026-11-09: landedFor edge added to DeploymentShape (#4773 AC8)

**To:** Kade
**From:** Borg Queen (driving #4773 with Silas navigating)
**Context:** #4773 AC8 — deploy records posted by `principal-deployer` with `landedFor` edge to the landing role.

Per Jeff's decision (option a) and Silas's direction on AC8:
- Added property `chorus:landedFor a owl:ObjectProperty ; rdfs:domain chorus:Deployment ; rdfs:range chorus:Role .` to `roles/kade/ontology/werk-domains.ttl`.
- Added `sh:property [ sh:path chorus:landedFor ; sh:class chorus:Role ; sh:maxCount 1 ]` to `chorus:DeploymentShape`.
- Updated RouteTable in `athena-make` tests to include `landedFor|edge:Role`.
- Verified conforming fixtures and shape validation in `platform/tests/4773-manifest-shape.bats` and `platform/tests/4773-deployer-identity.bats`.
