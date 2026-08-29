# Live-only shapes — urn:chorus:ontology vs MODEL_SET (2026-08-28 17:18)

Measured: 64 NodeShapes in the live graph after the 16:22 restore; MODEL_SET = 32 files. A shape listed here exists ONLY in the store: a source-only redeploy cannot restore it. This is #3587 measured, not described.

## Lost today (served this morning, no shape now, never in source)
- chorus:DocumentShape
- chorus:DomainShape
- chorus:ProductShape
- chorus:ValueStreamShape
- chorus:ValueStreamStepShape
- chorus:EmitContractShape

## Still live, not in source (next to go)
- chorus:RoleShape
- chorus:PrincipalWorkerHoldsNoRole
- chorus:PrincipalWorkerCannotSignIn
- chorus:PrincipalSignInNeedsWebId
- chorus:PrincipleShape
- chorus:NostrCredentialShape
- chorus:DomainOrderShape
- chorus:ProductOrderShape
- chorus:ChunkMembershipShape
- chorus:ChunkShape
- chorus:CardShape
- chorus:GateShape
- chorus:APISurfaceShape
- chorus:KeyRegistryEntryShape
- chorus:PrincipalShape
- chorus:FileShape
- chorus:QuarantineShape
- chorus:TestResultEdgesShape
- chorus:ScenarioVerifiedByShape
- chorus:VerbRunEdgesShape
- chorus:TestEdgesShape
- chorus:TestResultShape
- chorus:TestSuiteRunShape
- chorus:TestShape
- chorus:SourceFileShape
- chorus:QualityFindingShape
- chorus:CardDiffShape
- chorus:RecoveryActionShape
- chorus:ConflictHoldShape
- chorus:MergeEventShape
- chorus:WerkSlotShape
- chorus:DeploymentShape
- chorus:DemoVerdictShape
- chorus:ReviewVerdictShape
- chorus:GateResultShape
- chorus:PipelineRunShape
- chorus:WitnessShape
- chorus:TypedRefusalShape
- chorus:VerbRunShape
- chorus:ValueStreamShape
- chorus:StepShape
- chorus:MetricShape
- chorus:EmitContractShape
- chorus:PropertyKeyShape
- chorus:PropertyCarrierShape
- chorus:PropertyShape
- chorus:ServiceShape
- chorus:DomainShape
- chorus:AuthBoundaryShape
- chorus:SecurityProbeShape
- chorus:PermissionShape
- chorus:CredentialShape
- chorus:DocumentShape
- chorus:ProductShape
- chorus:GovernanceCheckShape
- chorus:ServiceInstanceShape
- chorus:ScheduledJobShape
- chorus:StreamEventShape
- chorus:ValueShape
- chorus:PracticeShape
- chorus:SpineEventShape
- chorus:MessageShape
- chorus:MemoryShape
- chorus:KnowledgeShape

## In source (safe)
