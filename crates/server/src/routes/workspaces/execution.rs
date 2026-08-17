use axum::{
    Extension, Json, Router,
    extract::State,
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    session::{CreateSession, Session},
    workspace::Workspace,
    workspace_repo::WorkspaceRepo,
};
use deployment::Deployment;
use executors::actions::{
    ExecutorAction, ExecutorActionType,
    claude_remote_control::{
        ClaudeRemoteControlPermissionMode, ClaudeRemoteControlRequest, ClaudeRemoteControlSpawnMode,
    },
    script::{ScriptContext, ScriptRequest, ScriptRequestLanguage},
};
use serde::{Deserialize, Serialize};
use services::services::container::ContainerService;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use super::remote_control_support;
use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum RunScriptError {
    NoScriptConfigured,
    ProcessAlreadyRunning,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(tag = "type", rename_all = "snake_case")]
pub enum RemoteControlError {
    AlreadyRunning { execution_process_id: Uuid },
    NoWorktree,
}

#[derive(Debug, Default, Deserialize, TS)]
pub struct StartRemoteControlRequest {
    /// Session name shown in the list on claude.ai/code.
    /// Defaults to the workspace name, then the branch.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<ClaudeRemoteControlPermissionMode>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub capacity: Option<u32>,
    #[serde(default)]
    pub continue_existing: bool,
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/dev-server/start", post(start_dev_server))
        .route("/cleanup", post(run_cleanup_script))
        .route("/archive", post(run_archive_script))
        .route("/stop", post(stop_workspace_execution))
        .route("/remote-control/start", post(start_remote_control))
        .route("/remote-control/trust", post(trust_remote_control_worktree))
        .route("/remote-control", get(get_remote_control))
    // Stopping deliberately goes through POST /api/execution-processes/{id}/stop.
    // The workspace-level /stop above must NOT stop remote control: "stop the
    // agent" has to leave a session the user is driving from their phone alive.
}

/// Start a Claude Remote Control session in this workspace's worktree.
///
/// Deliberately has no `has_running_blocking_processes_for_workspace` gate:
/// remote control runs alongside normal agent runs by design.
#[axum::debug_handler]
pub async fn start_remote_control(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Json(body): Json<StartRemoteControlRequest>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess, RemoteControlError>>, ApiError> {
    let pool = &deployment.db().pool;

    // Idempotent: if one is already running, hand back its id so the caller can
    // read that row's remote_control_url rather than starting a second server.
    let existing =
        ExecutionProcess::find_running_remote_control_by_workspace(pool, workspace.id).await?;
    if let Some(running) = existing.into_iter().next() {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            RemoteControlError::AlreadyRunning {
                execution_process_id: running.id,
            },
        )));
    }

    // Guarantees container_ref / the worktree exists before we spawn into it.
    deployment
        .container()
        .ensure_container_exists(&workspace)
        .await?;

    // Auto-context: `claude remote-control` cannot accept an initial prompt,
    // so drop the task context into the worktree as CLAUDE.local.md — Claude
    // Code auto-reads it at session start, and info/exclude keeps it out of
    // diffs. Best-effort: a failure here must not block the handoff.
    {
        let fresh = db::models::workspace::Workspace::find_by_id(pool, workspace.id).await?;
        if let Some(container_ref) = fresh.as_ref().and_then(|w| w.container_ref.as_deref()) {
            let task = db::models::workspace::Workspace::get_first_user_message(pool, workspace.id)
                .await
                .ok()
                .flatten();
            let name = workspace
                .name
                .clone()
                .unwrap_or_else(|| workspace.branch.clone());
            let content =
                remote_control_support::handoff_markdown(&name, &workspace.branch, task.as_deref());
            if let Err(e) = remote_control_support::write_handoff_file(
                std::path::Path::new(container_ref),
                &content,
            ) {
                tracing::warn!("skipping remote-control handoff file: {e}");
            }
        }
    }

    // Attach to the workspace's latest session. The frontend's execution-process
    // stream is scoped by session, so using the latest keeps the common
    // single-session case visible.
    let session = match Session::find_latest_by_workspace_id(pool, workspace.id).await? {
        Some(s) => s,
        None => {
            Session::create(
                pool,
                &CreateSession {
                    executor: Some("claude-remote-control".to_string()),
                    name: None,
                },
                Uuid::new_v4(),
                workspace.id,
            )
            .await?
        }
    };

    // Mirror ClaudeCode::permission_mode(): with the stock profile this resolves
    // to bypassPermissions, i.e. parity with a normal VK Claude run in the same
    // worktree. Note dangerously_skip_permissions is inert and is not consulted.
    let permission_mode = body
        .permission_mode
        .unwrap_or(ClaudeRemoteControlPermissionMode::BypassPermissions);

    let name = body
        .name
        .or_else(|| workspace.name.clone())
        .unwrap_or_else(|| workspace.branch.clone());

    let executor_action = ExecutorAction::new(
        ExecutorActionType::ClaudeRemoteControlRequest(ClaudeRemoteControlRequest {
            name,
            permission_mode,
            spawn_mode: ClaudeRemoteControlSpawnMode::SameDir,
            capacity: body.capacity,
            session_id: None,
            continue_existing: body.continue_existing,
            working_dir: body.working_dir,
            cmd: Default::default(),
        }),
        // Never a next_action: this is a terminal, long-lived process.
        None,
    );

    let execution_process = deployment
        .container()
        .start_execution(
            &workspace,
            &session,
            &executor_action,
            &ExecutionProcessRunReason::RemoteControl,
        )
        .await?;

    deployment
        .track_if_analytics_allowed(
            "claude_remote_control_started",
            serde_json::json!({ "workspace_id": workspace.id.to_string() }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(execution_process)))
}

/// Grant Claude Code workspace trust for this workspace's worktree — the same
/// consent running `claude` there and accepting the trust prompt records.
///
/// Only ever reached from an explicit button press in the Remote Control
/// dialog's trust-failure state: the user's click IS the consent. Without this,
/// every fresh worktree requires a manual terminal round-trip before the first
/// Remote Control start.
#[axum::debug_handler]
pub async fn trust_remote_control_worktree(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    // Make sure the worktree exists before pointing trust at it.
    deployment
        .container()
        .ensure_container_exists(&workspace)
        .await?;
    let workspace =
        db::models::workspace::Workspace::find_by_id(&deployment.db().pool, workspace.id)
            .await?
            .ok_or(db::models::workspace::WorkspaceError::WorkspaceNotFound)?;

    let Some(container_ref) = workspace.container_ref.as_deref() else {
        return Ok(ResponseJson(ApiResponse::error(
            "Workspace has no worktree yet",
        )));
    };
    let Some(config_path) = remote_control_support::claude_config_path() else {
        return Ok(ResponseJson(ApiResponse::error(
            "Could not locate the Claude Code config (~/.claude.json)",
        )));
    };
    if !config_path.exists() {
        return Ok(ResponseJson(ApiResponse::error(
            "Claude Code has never run on this machine — install it and run `claude` once first",
        )));
    }

    match remote_control_support::grant_workspace_trust(
        &config_path,
        std::path::Path::new(container_ref),
    ) {
        Ok(keys) => {
            tracing::info!(
                "Granted Claude workspace trust for {} ({} key form(s))",
                container_ref,
                keys.len()
            );
            Ok(ResponseJson(ApiResponse::success(())))
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(&e))),
    }
}

/// Current Claude Remote Control process for this workspace, if any.
///
/// Returns the newest running row. Note this trusts the row's status: after an
/// ungraceful VK shutdown a `running` row can survive with no child behind it,
/// because `kill_all_running_processes` only runs on a clean exit. Reconciling
/// that here would need a child-store lookup, which lives on
/// `LocalContainerService` rather than the `ContainerService` trait; the
/// existing `cleanup_orphan_executions` startup pass is the right place to
/// handle it. Tracked as follow-up.
#[axum::debug_handler]
pub async fn get_remote_control(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Option<ExecutionProcess>>>, ApiError> {
    let pool = &deployment.db().pool;

    // ANY state, not just running: the dialog must be able to see a fast
    // failure (trust, auth) even when it landed on a session the UI has not
    // selected — which is always the case on a fresh workspace.
    let latest =
        ExecutionProcess::find_latest_remote_control_by_workspace(pool, workspace.id).await?;

    Ok(ResponseJson(ApiResponse::success(latest)))
}

#[axum::debug_handler]
pub async fn start_dev_server(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<ExecutionProcess>>>, ApiError> {
    let pool = &deployment.db().pool;

    let existing_dev_servers =
        match ExecutionProcess::find_running_dev_servers_by_workspace(pool, workspace.id).await {
            Ok(servers) => servers,
            Err(e) => {
                tracing::error!(
                    "Failed to find running dev servers for workspace {}: {}",
                    workspace.id,
                    e
                );
                return Err(ApiError::Workspace(
                    db::models::workspace::WorkspaceError::ValidationError(e.to_string()),
                ));
            }
        };

    for dev_server in existing_dev_servers {
        tracing::info!(
            "Stopping existing dev server {} for workspace {}",
            dev_server.id,
            workspace.id
        );

        if let Err(e) = deployment
            .container()
            .stop_execution(&dev_server, ExecutionProcessStatus::Killed)
            .await
        {
            tracing::error!("Failed to stop dev server {}: {}", dev_server.id, e);
        }
    }

    let repos = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;
    let repos_with_dev_script: Vec<_> = repos
        .iter()
        .filter(|r| r.dev_server_script.as_ref().is_some_and(|s| !s.is_empty()))
        .collect();

    if repos_with_dev_script.is_empty() {
        return Ok(ResponseJson(ApiResponse::error(
            "No dev server script configured for any repository in this workspace",
        )));
    }

    let session = match Session::find_latest_by_workspace_id(pool, workspace.id).await? {
        Some(s) => s,
        None => {
            Session::create(
                pool,
                &CreateSession {
                    executor: Some("dev-server".to_string()),
                    name: None,
                },
                Uuid::new_v4(),
                workspace.id,
            )
            .await?
        }
    };

    let mut execution_processes = Vec::new();
    for repo in repos_with_dev_script {
        let executor_action = ExecutorAction::new(
            ExecutorActionType::ScriptRequest(ScriptRequest {
                script: repo.dev_server_script.clone().unwrap(),
                language: ScriptRequestLanguage::Bash,
                context: ScriptContext::DevServer,
                working_dir: Some(repo.name.clone()),
            }),
            None,
        );

        let execution_process = deployment
            .container()
            .start_execution(
                &workspace,
                &session,
                &executor_action,
                &ExecutionProcessRunReason::DevServer,
            )
            .await?;
        execution_processes.push(execution_process);
    }

    deployment
        .track_if_analytics_allowed(
            "dev_server_started",
            serde_json::json!({
                "workspace_id": workspace.id.to_string(),
            }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(execution_processes)))
}

pub async fn stop_workspace_execution(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    deployment.container().try_stop(&workspace, false).await;

    deployment
        .track_if_analytics_allowed(
            "task_attempt_stopped",
            serde_json::json!({
                "workspace_id": workspace.id.to_string(),
            }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(())))
}

#[axum::debug_handler]
pub async fn run_cleanup_script(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess, RunScriptError>>, ApiError> {
    let pool = &deployment.db().pool;

    if ExecutionProcess::has_running_blocking_processes_for_workspace(pool, workspace.id).await? {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            RunScriptError::ProcessAlreadyRunning,
        )));
    }

    deployment
        .container()
        .ensure_container_exists(&workspace)
        .await?;

    let repos = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;
    let executor_action = match deployment.container().cleanup_actions_for_repos(&repos) {
        Some(action) => action,
        None => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                RunScriptError::NoScriptConfigured,
            )));
        }
    };

    let session = match Session::find_latest_by_workspace_id(pool, workspace.id).await? {
        Some(s) => s,
        None => {
            Session::create(
                pool,
                &CreateSession {
                    executor: None,
                    name: None,
                },
                Uuid::new_v4(),
                workspace.id,
            )
            .await?
        }
    };

    let execution_process = deployment
        .container()
        .start_execution(
            &workspace,
            &session,
            &executor_action,
            &ExecutionProcessRunReason::CleanupScript,
        )
        .await?;

    deployment
        .track_if_analytics_allowed(
            "cleanup_script_executed",
            serde_json::json!({
                "workspace_id": workspace.id.to_string(),
            }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(execution_process)))
}

pub async fn run_archive_script(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess, RunScriptError>>, ApiError> {
    let pool = &deployment.db().pool;
    if ExecutionProcess::has_running_blocking_processes_for_workspace(pool, workspace.id).await? {
        return Ok(ResponseJson(ApiResponse::error_with_data(
            RunScriptError::ProcessAlreadyRunning,
        )));
    }

    deployment
        .container()
        .ensure_container_exists(&workspace)
        .await?;

    let repos = WorkspaceRepo::find_repos_for_workspace(pool, workspace.id).await?;
    let executor_action = match deployment.container().archive_actions_for_repos(&repos) {
        Some(action) => action,
        None => {
            return Ok(ResponseJson(ApiResponse::error_with_data(
                RunScriptError::NoScriptConfigured,
            )));
        }
    };
    let session = match Session::find_latest_by_workspace_id(pool, workspace.id).await? {
        Some(s) => s,
        None => {
            Session::create(
                pool,
                &CreateSession {
                    executor: None,
                    name: None,
                },
                Uuid::new_v4(),
                workspace.id,
            )
            .await?
        }
    };

    let execution_process = deployment
        .container()
        .start_execution(
            &workspace,
            &session,
            &executor_action,
            &ExecutionProcessRunReason::ArchiveScript,
        )
        .await?;

    deployment
        .track_if_analytics_allowed(
            "archive_script_executed",
            serde_json::json!({
                "workspace_id": workspace.id.to_string(),
            }),
        )
        .await;

    Ok(ResponseJson(ApiResponse::success(execution_process)))
}
