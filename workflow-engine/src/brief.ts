import { WorkflowManifest, Step } from './types';

export function generateHandoffBrief(
  manifest: WorkflowManifest,
  completedStep: Step,
  nextStep: Step,
): string {
  const progressLines = manifest.steps.map(s => {
    let icon: string;
    if (s.status === 'completed') icon = '[x]';
    else if (s.status === 'ready' || s.status === 'in_progress') icon = '[>]';
    else icon = '[ ]';
    return `${icon} Step ${s.seq}: ${s.role} — ${s.action}`;
  });

  const prevNotes = completedStep.notes || 'none';
  const prevArtifacts = completedStep.artifacts.length > 0
    ? completedStep.artifacts.join(', ')
    : 'none';

  return `# Workflow Handoff: ${manifest.id} — Step ${nextStep.seq}

**Workflow**: ${manifest.id}
**Decision**: ${manifest.decision}
**Your action**: ${nextStep.action}

## Context

Previous step (completed by ${completedStep.role}):
- **Action**: ${completedStep.action}
- **Notes**: ${prevNotes}
- **Artifacts**: ${prevArtifacts}

## What You Need To Do

${nextStep.action}

When complete, run:
\`\`\`bash
../messages/scripts/workflow-ts advance ${manifest.id} --notes "..." --artifacts "file1,file2"
\`\`\`

## Workflow Progress

${progressLines.join('\n')}
`;
}
