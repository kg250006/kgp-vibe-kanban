import type { Workspace } from 'shared/types';
import { workspacesApi } from '@/shared/lib/api';

/**
 * Build the block a user pastes as their FIRST message in a Claude Remote
 * Control session.
 *
 * `claude remote-control` has no way to accept an initial prompt — its
 * positional argument is a session name — so the user attaches to a completely
 * empty session. Without this, they arrive on their phone with no idea what the
 * workspace was for.
 */
export async function buildClaudeHandoffContext(
  workspaceId: string,
  workspace: Workspace
): Promise<string> {
  let firstMessage: string | null = null;
  try {
    firstMessage = await workspacesApi.getFirstUserMessage(workspaceId);
  } catch {
    // A workspace with no agent history yet — still worth handing over the
    // path and branch.
    firstMessage = null;
  }

  const name = workspace.name ?? workspace.branch;
  const lines = [
    "I'm continuing work in this worktree.",
    '',
    `Working directory: ${workspace.container_ref ?? '(not yet created)'}`,
    `Branch: ${workspace.branch}`,
    `Workspace: ${name}`,
    '',
  ];

  if (firstMessage) {
    lines.push('## Original task', firstMessage, '');
  }

  lines.push(
    'Please read the current state of the repo before making changes.'
  );

  return lines.join('\n');
}

/** True when the workspace has no recorded task description to hand over. */
export function hasTaskContext(firstMessage: string | null): boolean {
  return Boolean(firstMessage && firstMessage.trim().length > 0);
}
