import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { BaseCodingAgent, type ExecutionProcess } from 'shared/types';
import {
  workspacesApi,
  executionProcessesApi,
  configApi,
} from '@/shared/lib/api';
import { useWorkspaceExecution } from '@/shared/hooks/useWorkspaceExecution';
import { useLogStream } from '@/shared/hooks/useLogStream';
import {
  classifyRemoteControlFailure,
  extractClaudeSessionUrl,
  filterClaudeRemoteControlProcesses,
  filterRunningClaudeRemoteControl,
  type RemoteControlFailureKind,
} from '@/shared/lib/claudeRemoteControlUtils';
import { workspaceSummaryKeys } from '@/shared/hooks/workspaceSummaryKeys';
import type { ClaudeRemoteControlState } from '@/shared/types/actions';

interface UseClaudeRemoteControlOptions {
  /**
   * Resolve the session URL, opening a log stream if the process row has not
   * been patched with it yet.
   *
   * Defaults to false on purpose. useActionVisibilityContext is instantiated by
   * NavbarContainer, CommandBarDialog AND ContextBarContainer — without this
   * gate those three would each open a log websocket. Only the dialog needs it.
   */
  withSessionUrl?: boolean;
  onStartError?: (err: unknown) => void;
  onStopError?: (err: unknown) => void;
}

export function useClaudeRemoteControl(
  workspaceId: string | undefined,
  options?: UseClaudeRemoteControlOptions
) {
  const queryClient = useQueryClient();
  const { attemptData } = useWorkspaceExecution(workspaceId);

  const runningProcess = useMemo(
    () => filterRunningClaudeRemoteControl(attemptData.processes),
    [attemptData.processes]
  );

  // Most recent session of any status, so a stopped/failed one can still be
  // explained rather than silently disappearing back to idle.
  const latestProcess = useMemo(() => {
    const all = filterClaudeRemoteControlProcesses(attemptData.processes);
    if (all.length === 0) return null;
    return all.reduce((latest, p) =>
      new Date(p.started_at) > new Date(latest.started_at) ? p : latest
    );
  }, [attemptData.processes]);

  // Session-independent read path. The execution-process stream above is
  // scoped to the UI's SELECTED session, but a remote-control start on a fresh
  // workspace creates a NEW session — without this query a fast failure
  // (trust, auth) would be invisible and the dialog would spin forever.
  const { data: queriedProcess } = useQuery({
    queryKey: ['claudeRemoteControl', workspaceId],
    queryFn: () => workspacesApi.getClaudeRemoteControl(workspaceId ?? ''),
    enabled: Boolean(workspaceId),
    // The dialog (withSessionUrl) needs to observe a start converging within
    // seconds; passive consumers (context bar state) can be lazy.
    refetchInterval: options?.withSessionUrl ? 2500 : 15000,
  });

  const process: ExecutionProcess | null =
    runningProcess ?? latestProcess ?? queriedProcess ?? null;
  const isProcessRunning = process?.status === 'running';

  const [pendingStart, setPendingStart] = useState(false);
  // The id returned by the start call. pendingStart must clear as soon as THAT
  // process shows up in the stream in ANY state — a trust/auth failure kills
  // the child within a couple of seconds, and without this the dialog would
  // sit on "Starting…" forever instead of showing the actionable error.
  const startedIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!pendingStart) return;
    const startedArrived =
      startedIdRef.current != null &&
      (attemptData.processes.some((p) => p.id === startedIdRef.current) ||
        queriedProcess?.id === startedIdRef.current);
    if (runningProcess || startedArrived) setPendingStart(false);
  }, [runningProcess, pendingStart, attemptData.processes, queriedProcess]);

  // ─── SESSION URL SOURCE — SINGLE POINT OF CHANGE ─────────────────────────
  // The backend records the URL on the process row (remote_control_url). The
  // log-stream branch is a fallback for the window before that patch arrives.
  //
  // If the backend ever makes the URL deterministic at start time, delete the
  // useLogStream branch and the regex helper — nothing outside this file changes.
  const fromProcess = process?.remote_control_url ?? null;

  const logProcessId =
    options?.withSessionUrl && !fromProcess ? (process?.id ?? '') : '';
  // useLogStream('') early-returns, so this is a genuine no-op when not needed.
  const { logs } = useLogStream(logProcessId);

  const fromLogs = useMemo(() => {
    if (!logs || logs.length === 0) return null;
    return extractClaudeSessionUrl(logs.map((l) => l.content).join('\n'));
  }, [logs]);

  // Keyed by process id so the URL does not flicker away when the log buffer
  // clears or the process transitions out of running.
  const stickyUrl = useRef<{ id: string; url: string } | null>(null);
  const resolvedUrl = fromProcess ?? fromLogs ?? null;
  if (resolvedUrl && process && stickyUrl.current?.id !== process.id) {
    stickyUrl.current = { id: process.id, url: resolvedUrl };
  }
  const sessionUrl =
    resolvedUrl ??
    (process && stickyUrl.current?.id === process.id
      ? stickyUrl.current.url
      : null);
  // ─────────────────────────────────────────────────────────────────────────

  // NOT_FOUND means Claude Code has never run here. It cannot tell subscription
  // auth from API-key auth (it only stats ~/.claude.json), so this is a negative
  // gate only — never treat LOGIN_DETECTED as proof Remote Control will work.
  const { data: availability } = useQuery({
    queryKey: ['agentAvailability', 'CLAUDE_CODE'],
    queryFn: () =>
      configApi.checkAgentAvailability(BaseCodingAgent.CLAUDE_CODE),
    staleTime: 60_000,
    enabled: Boolean(workspaceId),
  });

  const startMutation = useMutation({
    mutationKey: ['startClaudeRemoteControl', workspaceId],
    mutationFn: async () => {
      if (!workspaceId) return;
      const process = await workspacesApi.startClaudeRemoteControl(workspaceId);
      startedIdRef.current = process?.id ?? null;
    },
    onMutate: () => {
      startedIdRef.current = null;
      setPendingStart(true);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ['executionProcesses', workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ['claudeRemoteControl', workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
    },
    onError: (err) => {
      setPendingStart(false);
      console.error('Failed to start Claude Remote Control:', err);
      options?.onStartError?.(err);
    },
  });

  const stopMutation = useMutation({
    mutationKey: ['stopClaudeRemoteControl', workspaceId],
    mutationFn: async () => {
      if (!process || process.status !== 'running') return;
      // Per-process stop, NOT workspacesApi.stop: the workspace-level stop
      // deliberately skips Remote Control so "stop the agent" cannot end a
      // session the user is driving from another device.
      await executionProcessesApi.stopExecutionProcess(process.id);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ['executionProcesses', workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ['claudeRemoteControl', workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: workspaceSummaryKeys.all });
    },
    onError: (err) => {
      console.error('Failed to stop Claude Remote Control:', err);
      options?.onStopError?.(err);
    },
  });

  const failure = useMemo((): {
    kind: RemoteControlFailureKind;
    detail: string;
  } | null => {
    if (!process || process.status !== 'failed') return null;
    const detail = (logs ?? [])
      .map((l) => l.content)
      .slice(-40)
      .join('\n');
    return { kind: classifyRemoteControlFailure(detail), detail };
  }, [process, logs]);

  const state: ClaudeRemoteControlState = useMemo(() => {
    if (availability?.type === 'NOT_FOUND') return 'unavailable';
    if (stopMutation.isPending) return 'stopping';
    if (startMutation.isPending || pendingStart) return 'starting';
    if (isProcessRunning) return sessionUrl ? 'running' : 'starting';
    if (process?.status === 'failed') return 'failed';
    if (
      process &&
      (process.status === 'completed' || process.status === 'killed')
    )
      return 'stopped';
    return 'idle';
  }, [
    availability,
    stopMutation.isPending,
    startMutation.isPending,
    pendingStart,
    isProcessRunning,
    sessionUrl,
    process,
  ]);

  return {
    state,
    process,
    sessionUrl,
    failure,
    availability,
    start: startMutation.mutate,
    stop: stopMutation.mutate,
    isStarting: startMutation.isPending || pendingStart,
    isStopping: stopMutation.isPending,
  };
}
