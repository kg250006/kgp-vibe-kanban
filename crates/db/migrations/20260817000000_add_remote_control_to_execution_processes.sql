-- Claude Remote Control support on execution_processes.
--
-- Two changes:
--   1. A nullable column holding the claude.ai session URL printed by
--      `claude remote-control`. NULL for every other run_reason.
--   2. A wider run_reason CHECK constraint admitting 'remotecontrol'.
--
-- Step 2 follows 20260203000000_add_archive_script_to_repos.sql verbatim:
-- SQLite cannot ALTER a CHECK constraint in place, so the column is added,
-- copied, dropped and renamed. The generated column executor_action_type is
-- derived from executor_action (not run_reason), so it does not block the drop.

-- 1. Session URL. Added first so it lands before the run_reason swap.
ALTER TABLE execution_processes ADD COLUMN remote_control_url TEXT;

-- 2a. Add the replacement column with the wider CHECK
ALTER TABLE execution_processes
  ADD COLUMN run_reason_new TEXT NOT NULL DEFAULT 'setupscript'
    CHECK (run_reason_new IN ('setupscript',
                               'cleanupscript',
                               'archivescript',
                               'codingagent',
                               'devserver',
                               'remotecontrol'));

-- 2b. Copy existing values across
UPDATE execution_processes
  SET run_reason_new = run_reason;

-- 2c. Drop any indexes that reference run_reason
DROP INDEX IF EXISTS idx_execution_processes_run_reason;
DROP INDEX IF EXISTS idx_execution_processes_session_status_run_reason;
DROP INDEX IF EXISTS idx_execution_processes_session_run_reason_created;

-- 2d. Remove the old column (requires SQLite 3.35+)
ALTER TABLE execution_processes DROP COLUMN run_reason;

-- 2e. Rename the new column back to the canonical name
ALTER TABLE execution_processes
  RENAME COLUMN run_reason_new TO run_reason;

-- 2f. Re-create all indexes
CREATE INDEX idx_execution_processes_run_reason
        ON execution_processes(run_reason);

CREATE INDEX idx_execution_processes_session_status_run_reason
        ON execution_processes (session_id, status, run_reason);

CREATE INDEX idx_execution_processes_session_run_reason_created
        ON execution_processes (session_id, run_reason, created_at DESC);
