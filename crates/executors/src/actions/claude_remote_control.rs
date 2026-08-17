//! Hand a workspace off to Anthropic's native Claude Code Remote Control.
//!
//! This spawns `claude remote-control`, which runs a persistent server in the
//! workspace worktree and prints a `https://claude.ai/code/<id>` session URL.
//! The user opens that URL from a phone or another machine and steers the
//! session from there while the code continues to execute locally.
//!
//! This is unrelated to Vibe Kanban's own relay-based "Remote Access" feature —
//! the relay is not involved at any point.

use std::{
    path::Path,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;
use workspace_utils::command_ext::GroupSpawnNoWindowExt;

use crate::{
    actions::Executable,
    approvals::ExecutorApprovalService,
    command::{CmdOverrides, CommandBuilder, apply_overrides},
    env::ExecutionEnv,
    executors::{ExecutorError, SpawnedChild},
};

/// Pinned separately from the SDK path in `executors/claude.rs`, which is pinned
/// to 2.1.119. `remote-control`'s `--continue` / `--session-id` require 2.1.200+.
///
/// Do NOT unify these two pins. The SDK path drives the CLI over the stream-json
/// control protocol and normalizes its emitted log shapes across ~2,750 lines;
/// bumping it is a separate piece of work with its own test pass. Remote control
/// is a plain argv invocation with no protocol coupling, so it can move freely.
const REMOTE_CONTROL_BASE_COMMAND: &str = "npx -y @anthropic-ai/claude-code@2.1.233 remote-control";

/// Mirrors `claude remote-control --permission-mode`.
///
/// Serialized with the exact CLI spellings so `as_cli_str` and serde cannot
/// drift apart (a mismatch here would be a runtime-only failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub enum ClaudeRemoteControlPermissionMode {
    #[serde(rename = "default")]
    Default,
    #[default]
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "bypassPermissions")]
    BypassPermissions,
    #[serde(rename = "dontAsk")]
    DontAsk,
    #[serde(rename = "plan")]
    Plan,
}

impl ClaudeRemoteControlPermissionMode {
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::BypassPermissions => "bypassPermissions",
            Self::DontAsk => "dontAsk",
            Self::Plan => "plan",
        }
    }
}

/// Mirrors `claude remote-control --spawn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub enum ClaudeRemoteControlSpawnMode {
    #[default]
    #[serde(rename = "same-dir")]
    SameDir,
    #[serde(rename = "worktree")]
    Worktree,
    #[serde(rename = "session")]
    Session,
}

impl ClaudeRemoteControlSpawnMode {
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Self::SameDir => "same-dir",
            Self::Worktree => "worktree",
            Self::Session => "session",
        }
    }
}

/// Every field except `name` carries `#[serde(default)]` so historical
/// `execution_processes.executor_action` blobs — written before a later field
/// existed — still deserialize. That column is unconstrained TEXT and is
/// re-read by restore flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct ClaudeRemoteControlRequest {
    /// `--name`. Shown in the session list on claude.ai/code.
    pub name: String,
    /// `--permission-mode`.
    #[serde(default)]
    pub permission_mode: ClaudeRemoteControlPermissionMode,
    /// `--spawn`. Always `SameDir` for Vibe Kanban: the worktree already exists,
    /// and `Worktree` would nest worktrees the diff/commit machinery cannot see.
    #[serde(default)]
    pub spawn_mode: ClaudeRemoteControlSpawnMode,
    /// `--capacity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    /// `--session-id`. Cannot be combined with `--continue`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// `-c` / `--continue`. Reattaches to a session previously recorded for this
    /// directory or one of its git worktrees.
    #[serde(default)]
    pub continue_existing: bool,
    /// Optional relative path to run in (relative to container_ref). Same
    /// semantics as `ScriptRequest::working_dir`.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// base_command_override / additional_params / env — the same escape hatch
    /// every other executor exposes.
    #[serde(flatten)]
    pub cmd: CmdOverrides,
}

impl ClaudeRemoteControlRequest {
    /// Build the argv parameter vector. Pure — no IO — so it can be asserted on
    /// in tests without spawning a process. Order is stable.
    pub(crate) fn build_params(&self) -> Vec<String> {
        let mut params: Vec<String> = vec![
            "--name".to_string(),
            self.name.clone(),
            // Emitted explicitly rather than relying on the CLI default, so a
            // future change to that default cannot silently alter behaviour.
            "--spawn".to_string(),
            self.spawn_mode.as_cli_str().to_string(),
            "--permission-mode".to_string(),
            self.permission_mode.as_cli_str().to_string(),
        ];

        if let Some(capacity) = self.capacity {
            params.push("--capacity".to_string());
            params.push(capacity.to_string());
        }

        if let Some(session_id) = &self.session_id {
            params.push("--session-id".to_string());
            params.push(session_id.clone());
            if self.continue_existing {
                tracing::warn!(
                    "claude remote-control: --session-id cannot be combined with --continue; \
                     ignoring continue_existing"
                );
            }
        } else if self.continue_existing {
            params.push("--continue".to_string());
        }

        params
    }
}

static SESSION_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https://claude\.ai/code/[A-Za-z0-9_\-]+").unwrap());

/// The server-mode "environment" link, printed as plain text before any
/// session attaches: `https://claude.ai/code?environment=env_...`
/// (verified against Claude Code 2.1.233 output).
static ENVIRONMENT_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https://claude\.ai/code\?environment=[A-Za-z0-9_\-]+").unwrap());

fn find_url(re: &Regex, raw: &str) -> Option<String> {
    // RAW text first, stripped text second — the order is load-bearing.
    // Claude Code 2.1.233 wraps the per-session URL in an OSC 8 hyperlink
    // escape (`ESC]8;;<url>BEL<label>ESC]8;;BEL`), and ANSI stripping removes
    // the ENTIRE sequence, URL included. Stripping first would delete exactly
    // the thing we are looking for. The stripped pass stays as a fallback for
    // output where a CSI colour code lands mid-URL.
    if let Some(m) = re.find(raw) {
        return Some(trim_url(m.as_str()));
    }
    let clean = strip_ansi_escapes::strip_str(raw);
    re.find(&clean).map(|m| trim_url(m.as_str()))
}

fn trim_url(url: &str) -> String {
    url.trim_end_matches(['.', ',', ')', ']', '"', '\'', ';', ':'])
        .to_string()
}

/// Extract a per-session URL (`https://claude.ai/code/session_...`) from CLI
/// output. This is the preferred link: it opens straight into the session.
/// Requires the `/code/` path segment so `claude.ai/chat/...` never matches.
pub fn extract_session_url(raw: &str) -> Option<String> {
    find_url(&SESSION_URL_RE, raw)
}

/// Extract the environment URL, printed before any session attaches. Fallback
/// only: the pre-created session (`--create-session-in-dir`, default on)
/// normally yields a session URL within a couple of seconds.
pub fn extract_environment_url(raw: &str) -> Option<String> {
    find_url(&ENVIRONMENT_URL_RE, raw)
}

#[async_trait]
impl Executable for ClaudeRemoteControlRequest {
    async fn spawn(
        &self,
        current_dir: &Path,
        _approvals: Arc<dyn ExecutorApprovalService>,
        env: &ExecutionEnv,
    ) -> Result<SpawnedChild, ExecutorError> {
        let effective_dir = match &self.working_dir {
            Some(rel_path) => current_dir.join(rel_path),
            None => current_dir.to_path_buf(),
        };

        let builder = apply_overrides(
            CommandBuilder::new(REMOTE_CONTROL_BASE_COMMAND.to_string())
                .params(self.build_params()),
            &self.cmd,
        )?;
        // into_resolved() surfaces a missing binary as ExecutorError::ExecutableNotFound,
        // which start_execution already turns into an actionable SetupRequired message.
        // Going through a shell would bury it as an opaque exit code 127 instead.
        let (program, args) = builder.build_initial()?.into_resolved().await?;

        let mut command = Command::new(program);
        command
            .kill_on_drop(true)
            // Piped so the one-time consent below can be answered; closed right
            // after, so any LATER interactive prompt (login, trust) still reads
            // EOF and fails fast instead of hanging.
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(&effective_dir)
            .env("NPM_CONFIG_LOGLEVEL", "error")
            .args(&args);

        env.clone()
            .with_profile(&self.cmd)
            .apply_to_command(&mut command);

        // Remote Control is subscription-auth only; API keys are unsupported.
        // A stale key in the environment can shadow the OAuth token, so strip it
        // unconditionally rather than behind an opt-in flag.
        command.env_remove("ANTHROPIC_API_KEY");

        let mut child = command.group_spawn_no_window()?;

        // The very first remote-control run on a machine shows a one-time
        // "Enable Remote Control? (y/n)" prompt (verified on 2.1.233), and the
        // answer persists. The user's explicit start action IS that consent, so
        // answer it; on an already-consented machine the byte sits unread in a
        // closed pipe and is harmless. Dropping the handle closes the pipe.
        if let Some(mut stdin) = child.inner().stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(b"y\n").await;
            let _ = stdin.flush().await;
        }

        Ok(child.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str) -> ClaudeRemoteControlRequest {
        ClaudeRemoteControlRequest {
            name: name.to_string(),
            permission_mode: ClaudeRemoteControlPermissionMode::BypassPermissions,
            spawn_mode: ClaudeRemoteControlSpawnMode::SameDir,
            capacity: None,
            session_id: None,
            continue_existing: false,
            working_dir: None,
            cmd: CmdOverrides::default(),
        }
    }

    // Verbatim fixtures captured from `claude remote-control` 2.1.233 on
    // 2026-08-17 (PRP task 0.1 probe). The session URL is wrapped in an OSC 8
    // hyperlink escape; the environment URL is plain text and printed FIRST.
    const REAL_OSC8_SESSION_LINE: &str = "    \x1b]8;;https://claude.ai/code/session_01GUZA9hpor3WPWrWDfWc6am?from=cli\x07Attached\x1b]8;;\x07";
    const REAL_ENVIRONMENT_LINE: &str = "Continue coding in the Claude mobile app or https://claude.ai/code?environment=env_01NU6JZLjs6QbGBeRdXmvUX9";

    // Synthetic line kept for the plain-text path.
    const SUCCESS_LINE: &str =
        "  Open this session:  https://claude.ai/code/abc123XYZ_-def  (or scan the QR)";

    /// THE regression this fixture exists for: the URL lives inside the OSC 8
    /// escape sequence, so stripping ANSI first deletes it. Raw-first matching
    /// must recover it, without the `?from=cli` query tail.
    #[test]
    fn extracts_session_url_from_real_osc8_line() {
        assert_eq!(
            extract_session_url(REAL_OSC8_SESSION_LINE),
            Some("https://claude.ai/code/session_01GUZA9hpor3WPWrWDfWc6am".to_string())
        );
    }

    /// Prove the failure mode is real: after stripping, the OSC 8 payload —
    /// URL included — is gone. If this ever starts finding a URL, the strip
    /// crate changed behaviour and find_url's ordering comment needs review.
    #[test]
    fn stripping_deletes_the_osc8_url() {
        let stripped = strip_ansi_escapes::strip_str(REAL_OSC8_SESSION_LINE);
        assert!(
            !stripped.contains("claude.ai/code/session_"),
            "strip_ansi_escapes now preserves OSC 8 payloads: {stripped:?}"
        );
    }

    #[test]
    fn environment_url_extracts_but_is_not_a_session_url() {
        assert_eq!(
            extract_environment_url(REAL_ENVIRONMENT_LINE),
            Some("https://claude.ai/code?environment=env_01NU6JZLjs6QbGBeRdXmvUX9".to_string())
        );
        // The env link must never satisfy the session matcher, or the tap
        // would stop waiting before the pre-created session attaches.
        assert_eq!(extract_session_url(REAL_ENVIRONMENT_LINE), None);
    }

    #[test]
    fn extracts_bare_url() {
        assert_eq!(
            extract_session_url("https://claude.ai/code/abc123"),
            Some("https://claude.ai/code/abc123".to_string())
        );
    }

    #[test]
    fn extracts_from_success_line() {
        assert_eq!(
            extract_session_url(SUCCESS_LINE),
            Some("https://claude.ai/code/abc123XYZ_-def".to_string())
        );
    }

    #[test]
    fn strips_ansi_codes() {
        let ansi = "\x1b[1;36mhttps://claude.ai/code/abc123\x1b[0m";
        assert_eq!(
            extract_session_url(ansi),
            Some("https://claude.ai/code/abc123".to_string())
        );
    }

    #[test]
    fn trims_trailing_punctuation() {
        for suffix in ['.', ',', ')', ']', '"', '\'', ';', ':'] {
            let input = format!("see https://claude.ai/code/abc123{suffix}");
            assert_eq!(
                extract_session_url(&input),
                Some("https://claude.ai/code/abc123".to_string()),
                "failed for trailing {suffix:?}"
            );
        }
    }

    #[test]
    fn ignores_non_code_claude_urls() {
        assert_eq!(extract_session_url("https://claude.ai/chat/abc123"), None);
    }

    #[test]
    fn returns_none_without_url() {
        assert_eq!(
            extract_session_url("starting remote control server..."),
            None
        );
    }

    /// The failure mode raw-stdout parsing actually has: the child's output
    /// arrives in arbitrary byte chunks, so a URL can straddle a boundary.
    /// Accumulating into a rolling buffer must recover it at every split point.
    #[test]
    fn url_survives_chunk_splitting_at_every_offset() {
        for split in 0..REAL_OSC8_SESSION_LINE.len() {
            if !REAL_OSC8_SESSION_LINE.is_char_boundary(split) {
                continue;
            }
            let (head, tail) = REAL_OSC8_SESSION_LINE.split_at(split);
            let mut buf = String::from(head);
            // Mid-stream: the head alone may not contain the URL yet.
            buf.push_str(tail);
            assert!(
                extract_session_url(&buf).is_some(),
                "URL not recovered when reassembled at offset {split}"
            );
        }
    }

    #[test]
    fn default_params_emit_spawn_and_permission_mode_explicitly() {
        let params = req("my-workspace").build_params();
        assert_eq!(
            params,
            vec![
                "--name",
                "my-workspace",
                "--spawn",
                "same-dir",
                "--permission-mode",
                "bypassPermissions",
            ]
        );
    }

    #[test]
    fn capacity_and_session_id_append() {
        let mut r = req("w");
        r.capacity = Some(4);
        r.session_id = Some("11111111-2222-3333-4444-555555555555".to_string());
        let params = r.build_params();
        assert!(params.windows(2).any(|w| w == ["--capacity", "4"]));
        assert!(
            params
                .windows(2)
                .any(|w| w == ["--session-id", "11111111-2222-3333-4444-555555555555"])
        );
    }

    #[test]
    fn session_id_suppresses_continue() {
        let mut r = req("w");
        r.session_id = Some("some-id".to_string());
        r.continue_existing = true;
        let params = r.build_params();
        assert!(
            !params.iter().any(|p| p == "--continue"),
            "--continue must not be emitted alongside --session-id"
        );
    }

    #[test]
    fn continue_alone_is_emitted() {
        let mut r = req("w");
        r.continue_existing = true;
        assert!(r.build_params().iter().any(|p| p == "--continue"));
    }

    /// Catches the classic `acceptEdits` vs `accept_edits` casing bug, which
    /// would otherwise only surface at runtime as an unknown-flag error.
    #[test]
    fn permission_mode_cli_str_matches_serde() {
        for mode in [
            ClaudeRemoteControlPermissionMode::Default,
            ClaudeRemoteControlPermissionMode::AcceptEdits,
            ClaudeRemoteControlPermissionMode::Auto,
            ClaudeRemoteControlPermissionMode::BypassPermissions,
            ClaudeRemoteControlPermissionMode::DontAsk,
            ClaudeRemoteControlPermissionMode::Plan,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let serde_str = json.trim_matches('"');
            assert_eq!(mode.as_cli_str(), serde_str, "mismatch for {mode:?}");
        }
    }

    #[test]
    fn spawn_mode_cli_str_matches_serde() {
        for mode in [
            ClaudeRemoteControlSpawnMode::SameDir,
            ClaudeRemoteControlSpawnMode::Worktree,
            ClaudeRemoteControlSpawnMode::Session,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(
                mode.as_cli_str(),
                json.trim_matches('"'),
                "mismatch for {mode:?}"
            );
        }
    }

    /// `executor_action` is unconstrained historical TEXT, so a blob written by
    /// an older build must still load.
    #[test]
    fn minimal_historical_blob_deserializes_with_defaults() {
        let json = r#"{"name":"legacy-workspace"}"#;
        let parsed: ClaudeRemoteControlRequest = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "legacy-workspace");
        assert_eq!(
            parsed.spawn_mode,
            ClaudeRemoteControlSpawnMode::SameDir,
            "spawn_mode must default to same-dir"
        );
        assert_eq!(
            parsed.permission_mode,
            ClaudeRemoteControlPermissionMode::AcceptEdits
        );
        assert!(!parsed.continue_existing);
        assert!(parsed.capacity.is_none());
    }
}
