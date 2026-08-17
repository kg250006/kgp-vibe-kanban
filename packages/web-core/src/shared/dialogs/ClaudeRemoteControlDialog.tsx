import { useCallback, useEffect, useState } from 'react';
import { create, useModal } from '@ebay/nice-modal-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@vibe/ui/components/Button';
import { Alert, AlertDescription, AlertTitle } from '@vibe/ui/components/Alert';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@vibe/ui/components/KeyboardDialog';
import { useClaudeRemoteControl } from '@/shared/hooks/useClaudeRemoteControl';
import { useWorkspaceContext } from '@/shared/hooks/useWorkspaceContext';
import { QrCode } from '@/shared/components/QrCode';
import { buildClaudeHandoffContext } from '@/shared/lib/claudeHandoffContext';
import { defineModal } from '@/shared/lib/modals';

export interface ClaudeRemoteControlDialogProps {
  workspaceId?: string;
}

const ClaudeRemoteControlDialogImpl = create<ClaudeRemoteControlDialogProps>(
  ({ workspaceId }) => {
    const modal = useModal();
    const { t } = useTranslation('common');
    const { workspace } = useWorkspaceContext();
    const { state, process, sessionUrl, failure, start, stop } =
      useClaudeRemoteControl(workspaceId, { withSessionUrl: true });

    const [copiedLink, setCopiedLink] = useState(false);
    const [copiedContext, setCopiedContext] = useState(false);
    const [slowStart, setSlowStart] = useState(false);
    const [trusting, setTrusting] = useState(false);
    const [trustError, setTrustError] = useState<string | null>(null);

    // "Trust this folder and start": records the same consent as running
    // `claude` in the worktree and accepting the prompt, then retries. The
    // user's click is the consent — this is only reachable from the explicit
    // trust-failure state.
    const trustAndStart = useCallback(async () => {
      if (!workspaceId) return;
      setTrusting(true);
      setTrustError(null);
      try {
        const { workspacesApi } = await import('@/shared/lib/api');
        await workspacesApi.trustClaudeRemoteControl(workspaceId);
        start();
      } catch (err) {
        setTrustError(err instanceof Error ? err.message : String(err));
      } finally {
        setTrusting(false);
      }
    }, [workspaceId, start]);

    // After ~15s of "starting" with no link, the most likely cause is Claude
    // asking to trust this worktree — surface that rather than spinning forever.
    useEffect(() => {
      if (state !== 'starting') {
        setSlowStart(false);
        return;
      }
      const timer = setTimeout(() => setSlowStart(true), 15_000);
      return () => clearTimeout(timer);
    }, [state]);

    const copyLink = useCallback(async () => {
      if (!sessionUrl) return;
      await navigator.clipboard.writeText(sessionUrl);
      setCopiedLink(true);
      setTimeout(() => setCopiedLink(false), 2000);
    }, [sessionUrl]);

    const copyContext = useCallback(async () => {
      if (!workspaceId || !workspace) return;
      const blob = await buildClaudeHandoffContext(workspaceId, workspace);
      await navigator.clipboard.writeText(blob);
      setCopiedContext(true);
      setTimeout(() => setCopiedContext(false), 2000);
    }, [workspaceId, workspace]);

    const worktreePath = workspace?.container_ref ?? '';

    const contextButton = (
      <div className="space-y-1">
        <Button variant="outline" onClick={copyContext} className="w-full">
          {copiedContext
            ? t('claudeRemoteControl.linkCopied')
            : t('claudeRemoteControl.context.button')}
        </Button>
        <p className="text-xs text-muted-foreground">
          {t('claudeRemoteControl.context.helper')}
        </p>
      </div>
    );

    const renderBody = () => {
      if (state === 'unavailable') {
        return (
          <Alert variant="destructive">
            <AlertTitle>
              {t('claudeRemoteControl.error.notInstalled.title')}
            </AlertTitle>
            <AlertDescription>
              {t('claudeRemoteControl.error.notInstalled.body')}
            </AlertDescription>
          </Alert>
        );
      }

      if (state === 'failed' && failure) {
        // Static t() literals only — scripts/check-unused-i18n-keys.mjs scans
        // for them and a template-literal key would fail the lint gate.
        const copy = {
          auth: {
            title: t('claudeRemoteControl.error.auth.title'),
            body: t('claudeRemoteControl.error.auth.body'),
          },
          trust: {
            title: t('claudeRemoteControl.error.trust.title'),
            body: t('claudeRemoteControl.error.trust.body'),
          },
          notInstalled: {
            title: t('claudeRemoteControl.error.notInstalled.title'),
            body: t('claudeRemoteControl.error.notInstalled.body'),
          },
          unknown: {
            title: t('claudeRemoteControl.error.unknown.title'),
            body: t('claudeRemoteControl.error.unknown.body'),
          },
        }[failure.kind];

        return (
          <div className="space-y-3">
            <Alert variant="destructive">
              <AlertTitle>{copy.title}</AlertTitle>
              <AlertDescription>{copy.body}</AlertDescription>
            </Alert>
            {failure.kind === 'trust' && (
              <div className="space-y-2">
                {worktreePath && (
                  <pre className="select-all rounded bg-muted p-2 text-xs">
                    {worktreePath}
                  </pre>
                )}
                <Button
                  className="w-full"
                  disabled={trusting}
                  onClick={() => void trustAndStart()}
                >
                  {trusting
                    ? t('claudeRemoteControl.starting')
                    : t('claudeRemoteControl.action.trustAndStart')}
                </Button>
                <p className="text-xs text-muted-foreground">
                  {t('claudeRemoteControl.action.trustExplainer')}
                </p>
                {trustError && (
                  <p className="text-xs text-destructive">{trustError}</p>
                )}
              </div>
            )}
            {failure.kind === 'unknown' && failure.detail && (
              <pre className="max-h-32 overflow-auto rounded bg-muted p-2 text-xs">
                {failure.detail.split('\n').slice(-5).join('\n')}
              </pre>
            )}
          </div>
        );
      }

      if (state === 'running' && sessionUrl) {
        return (
          <div className="space-y-4">
            <div className="flex items-center gap-2 text-sm">
              <span className="inline-block h-2 w-2 rounded-full bg-success" />
              <span className="text-brand">
                {t('claudeRemoteControl.live')}
              </span>
            </div>

            <div>
              <p className="mb-1 text-xs text-muted-foreground">
                {t('claudeRemoteControl.sessionLink')}
              </p>
              <div className="flex items-center gap-2">
                <code className="flex-1 select-all truncate rounded bg-muted px-2 py-1 text-xs">
                  {sessionUrl}
                </code>
                <Button size="sm" variant="outline" onClick={copyLink}>
                  {copiedLink
                    ? t('claudeRemoteControl.linkCopied')
                    : t('claudeRemoteControl.copyLink')}
                </Button>
              </div>
            </div>

            <div className="flex flex-col items-center gap-1">
              <QrCode
                value={sessionUrl}
                fallback={t('claudeRemoteControl.qrUnavailable')}
              />
              <p className="text-xs text-muted-foreground">
                {t('claudeRemoteControl.qrLabel')}
              </p>
            </div>

            {contextButton}

            <p className="text-xs text-muted-foreground">
              {t('claudeRemoteControl.explainer.keepRunning')}
            </p>
          </div>
        );
      }

      if (state === 'starting' || state === 'stopping') {
        return (
          <div className="space-y-2">
            <p className="text-sm">{t('claudeRemoteControl.starting')}</p>
            <p className="text-xs text-muted-foreground">
              {slowStart
                ? t('claudeRemoteControl.startingSlow')
                : t('claudeRemoteControl.startingHint')}
            </p>
          </div>
        );
      }

      if (state === 'stopped') {
        return (
          <div className="space-y-3">
            <Alert>
              <AlertTitle>{t('claudeRemoteControl.ended.title')}</AlertTitle>
              <AlertDescription>
                {t('claudeRemoteControl.ended.body')}
              </AlertDescription>
            </Alert>
            {contextButton}
          </div>
        );
      }

      // idle
      return (
        <div className="space-y-3">
          <ul className="space-y-1 text-sm text-muted-foreground">
            <li>{t('claudeRemoteControl.explainer.runsLocally')}</li>
            <li>{t('claudeRemoteControl.explainer.subscription')}</li>
            <li>{t('claudeRemoteControl.explainer.keepRunning')}</li>
          </ul>
          {contextButton}
        </div>
      );
    };

    const isRunning = state === 'running';
    const canStart =
      state === 'idle' || state === 'stopped' || state === 'failed';

    return (
      <Dialog open={modal.visible} onOpenChange={() => modal.hide()}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('claudeRemoteControl.title')}</DialogTitle>
            <DialogDescription>
              {t('claudeRemoteControl.subtitle')}
            </DialogDescription>
          </DialogHeader>

          <Alert>
            <AlertDescription className="text-xs">
              {t('claudeRemoteControl.previewNotice')}
            </AlertDescription>
          </Alert>

          {renderBody()}

          <DialogFooter>
            {isRunning ? (
              <div className="w-full space-y-1">
                <Button
                  variant="destructive"
                  className="w-full"
                  onClick={() => stop()}
                >
                  {t('claudeRemoteControl.stop')}
                </Button>
                <p className="text-xs text-muted-foreground">
                  {t('claudeRemoteControl.stopWarning')}
                </p>
              </div>
            ) : (
              <Button
                className="w-full"
                // Rendered but disabled when unavailable — hiding it would make
                // the feature undiscoverable.
                disabled={!canStart || !workspaceId}
                onClick={() => start()}
              >
                {state === 'stopped' || state === 'failed'
                  ? t('claudeRemoteControl.startAgain')
                  : t('claudeRemoteControl.start')}
              </Button>
            )}
          </DialogFooter>

          {process && (
            <p className="text-[10px] text-muted-foreground">
              {t('claudeRemoteControl.processLabel')}
            </p>
          )}
        </DialogContent>
      </Dialog>
    );
  }
);

export const ClaudeRemoteControlDialog = defineModal<
  ClaudeRemoteControlDialogProps,
  void
>(ClaudeRemoteControlDialogImpl);
