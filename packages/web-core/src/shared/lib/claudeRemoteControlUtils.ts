import type { ExecutionProcess } from 'shared/types';

/**
 * Matches the per-session URL printed by `claude remote-control`
 * (verified 2.1.233: `https://claude.ai/code/session_...`, wrapped in an OSC 8
 * hyperlink escape — matching RAW log text is required, since the URL is part
 * of the escape sequence itself). The `/code/` segment is required so a
 * `claude.ai/chat/...` URL never matches.
 */
export const CLAUDE_SESSION_URL_RE =
  /https:\/\/claude\.ai\/code\/[A-Za-z0-9_-]+/;

/**
 * The server-mode environment link, printed as plain text before any session
 * attaches: `https://claude.ai/code?environment=env_...`.
 */
export const CLAUDE_ENVIRONMENT_URL_RE =
  /https:\/\/claude\.ai\/code\?environment=[A-Za-z0-9_-]+/;

/**
 * Extract the best claude.ai URL from a chunk of process output — per-session
 * link preferred (opens straight into the session), environment link as
 * fallback.
 *
 * Fallback path only: the backend records the URL on the process row. This
 * exists so the dialog can still recover it if the row has not been patched yet.
 */
export function extractClaudeSessionUrl(text: string): string | null {
  const session = text.match(CLAUDE_SESSION_URL_RE);
  if (session) return session[0];
  const environment = text.match(CLAUDE_ENVIRONMENT_URL_RE);
  return environment ? environment[0] : null;
}

/**
 * Filter processes to only include Claude Remote Control sessions.
 */
export function filterClaudeRemoteControlProcesses(
  processes: ExecutionProcess[]
): ExecutionProcess[] {
  return processes.filter((process) => process.run_reason === 'remotecontrol');
}

/**
 * The newest running Claude Remote Control session, if any.
 */
export function filterRunningClaudeRemoteControl(
  processes: ExecutionProcess[]
): ExecutionProcess | null {
  const running = processes.filter(
    (process) =>
      process.run_reason === 'remotecontrol' && process.status === 'running'
  );
  if (running.length === 0) return null;
  return running.reduce((latest, process) =>
    new Date(process.started_at) > new Date(latest.started_at)
      ? process
      : latest
  );
}

export type RemoteControlFailureKind =
  | 'auth'
  | 'trust'
  | 'notInstalled'
  | 'unknown';

/**
 * Classify why a Remote Control process failed, from the tail of its output.
 *
 * Order matters: `notInstalled` is checked before `trust` because a Windows
 * "is not recognized as an internal or external command" message also contains
 * the word "command", and we want the more specific diagnosis to win.
 */
export function classifyRemoteControlFailure(
  logTail: string
): RemoteControlFailureKind {
  if (
    /ENOENT|command not found|is not recognized as an internal or external/i.test(
      logTail
    )
  ) {
    return 'notInstalled';
  }
  if (
    /api key|ANTHROPIC_API_KEY|subscription|\/login|not supported with an api key|log in/i.test(
      logTail
    )
  ) {
    return 'auth';
  }
  if (/trust|do you trust|hasTrustDialogAccepted/i.test(logTail)) {
    return 'trust';
  }
  return 'unknown';
}
