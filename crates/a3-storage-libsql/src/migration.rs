use blake3::Hasher;
use libsql::{Connection, TransactionBehavior, params};
use std::error::Error;
use std::fmt;

const CATALOG_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "bootstrap_catalog",
        sql: "CREATE TABLE schema_migrations (\n\
          version INTEGER PRIMARY KEY CHECK (version > 0),\n\
          name TEXT NOT NULL UNIQUE,\n\
          checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
          ) STRICT;",
    },
    Migration {
        version: 2,
        name: "project_catalog",
        sql: "CREATE TABLE projects (\n\
          project_id BLOB PRIMARY KEY NOT NULL CHECK (length(project_id) = 32),\n\
          repository_id BLOB NOT NULL UNIQUE CHECK (length(repository_id) = 32),\n\
          repository_common_directory BLOB NOT NULL\n\
            CHECK (length(repository_common_directory) BETWEEN 1 AND 131072),\n\
          repository_path_encoding TEXT NOT NULL\n\
            CHECK (repository_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          main_remote_id BLOB CHECK (main_remote_id IS NULL OR length(main_remote_id) = 32),\n\
          created_open_sequence INTEGER NOT NULL UNIQUE CHECK (created_open_sequence > 0),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE\n\
            CHECK (last_open_sequence >= created_open_sequence),\n\
          UNIQUE (project_id, repository_id)\n\
          ) STRICT;\n\
          CREATE INDEX projects_main_remote_id_idx\n\
            ON projects (main_remote_id) WHERE main_remote_id IS NOT NULL;\n\
          CREATE TABLE recent_worktrees (\n\
          worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
          project_id BLOB NOT NULL CHECK (length(project_id) = 32),\n\
          repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
          worktree_root BLOB NOT NULL CHECK (length(worktree_root) BETWEEN 1 AND 131072),\n\
          worktree_path_encoding TEXT NOT NULL\n\
            CHECK (worktree_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          worktree_root_display TEXT NOT NULL\n\
            CHECK (length(worktree_root_display) BETWEEN 1 AND 32768),\n\
          head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
          head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
          head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE CHECK (last_open_sequence > 0),\n\
          CHECK (\n\
            (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
            (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
          ),\n\
          FOREIGN KEY (project_id, repository_id)\n\
            REFERENCES projects(project_id, repository_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          CREATE INDEX recent_worktrees_project_id_idx\n\
            ON recent_worktrees (project_id);",
    },
    Migration {
        version: 3,
        name: "worktree_reconciliation",
        sql: "CREATE TABLE projects_v3 (\n\
          project_id BLOB PRIMARY KEY NOT NULL CHECK (length(project_id) = 32),\n\
          created_open_sequence INTEGER NOT NULL UNIQUE CHECK (created_open_sequence > 0),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE\n\
            CHECK (last_open_sequence >= created_open_sequence)\n\
          ) STRICT;\n\
          CREATE TABLE repository_observations_v3 (\n\
          repository_id BLOB PRIMARY KEY NOT NULL CHECK (length(repository_id) = 32),\n\
          project_id BLOB NOT NULL CHECK (length(project_id) = 32),\n\
          repository_common_directory BLOB NOT NULL\n\
            CHECK (length(repository_common_directory) BETWEEN 1 AND 131072),\n\
          repository_path_encoding TEXT NOT NULL\n\
            CHECK (repository_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          main_remote_id BLOB CHECK (main_remote_id IS NULL OR length(main_remote_id) = 32),\n\
          first_open_sequence INTEGER NOT NULL CHECK (first_open_sequence > 0),\n\
          last_open_sequence INTEGER NOT NULL CHECK (last_open_sequence >= first_open_sequence),\n\
          UNIQUE (project_id, repository_id),\n\
          FOREIGN KEY (project_id) REFERENCES projects_v3(project_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          CREATE TABLE recent_worktrees_v3 (\n\
          worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
          project_id BLOB NOT NULL CHECK (length(project_id) = 32),\n\
          repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
          worktree_anchor_id BLOB CHECK (worktree_anchor_id IS NULL OR length(worktree_anchor_id) = 32),\n\
          worktree_root BLOB NOT NULL CHECK (length(worktree_root) BETWEEN 1 AND 131072),\n\
          worktree_path_encoding TEXT NOT NULL\n\
            CHECK (worktree_path_encoding IN ('unix-bytes-v1', 'windows-utf16le-v1', 'utf8-lossy-v1')),\n\
          worktree_root_display TEXT NOT NULL\n\
            CHECK (length(worktree_root_display) BETWEEN 1 AND 32768),\n\
          head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
          head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
          head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
          last_open_sequence INTEGER NOT NULL UNIQUE CHECK (last_open_sequence > 0),\n\
          CHECK (\n\
            (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
            (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
          ),\n\
          FOREIGN KEY (project_id) REFERENCES projects_v3(project_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
          FOREIGN KEY (project_id, repository_id)\n\
            REFERENCES repository_observations_v3(project_id, repository_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          INSERT INTO projects_v3 (project_id, created_open_sequence, last_open_sequence)\n\
            SELECT project_id, created_open_sequence, last_open_sequence FROM projects;\n\
          INSERT INTO repository_observations_v3 (\n\
            repository_id, project_id, repository_common_directory, repository_path_encoding,\n\
            main_remote_id, first_open_sequence, last_open_sequence\n\
          ) SELECT repository_id, project_id, repository_common_directory,\n\
            repository_path_encoding, main_remote_id, created_open_sequence, last_open_sequence\n\
            FROM projects;\n\
          INSERT INTO recent_worktrees_v3 (\n\
            worktree_id, project_id, repository_id, worktree_anchor_id, worktree_root,\n\
            worktree_path_encoding, worktree_root_display, head_kind, head_object_id,\n\
            head_reference, last_open_sequence\n\
          ) SELECT worktree_id, project_id, repository_id, NULL, worktree_root,\n\
            worktree_path_encoding, worktree_root_display, head_kind, head_object_id,\n\
            head_reference, last_open_sequence FROM recent_worktrees;\n\
          DROP TABLE recent_worktrees;\n\
          DROP TABLE projects;\n\
          ALTER TABLE projects_v3 RENAME TO projects;\n\
          ALTER TABLE repository_observations_v3 RENAME TO repository_observations;\n\
          ALTER TABLE recent_worktrees_v3 RENAME TO recent_worktrees;\n\
          CREATE INDEX repository_observations_project_id_idx\n\
            ON repository_observations (project_id);\n\
          CREATE INDEX repository_observations_main_remote_id_idx\n\
            ON repository_observations (main_remote_id) WHERE main_remote_id IS NOT NULL;\n\
          CREATE INDEX recent_worktrees_project_id_idx\n\
            ON recent_worktrees (project_id);\n\
          CREATE INDEX recent_worktrees_anchor_idx\n\
            ON recent_worktrees (worktree_anchor_id) WHERE worktree_anchor_id IS NOT NULL;\n\
          CREATE TABLE worktree_reconciliations (\n\
          target_worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(target_worktree_id) = 32),\n\
          source_worktree_id BLOB NOT NULL UNIQUE CHECK (length(source_worktree_id) = 32),\n\
          project_id BLOB NOT NULL CHECK (length(project_id) = 32),\n\
          source_repository_id BLOB NOT NULL CHECK (length(source_repository_id) = 32),\n\
          target_repository_id BLOB NOT NULL CHECK (length(target_repository_id) = 32),\n\
          worktree_anchor_id BLOB NOT NULL CHECK (length(worktree_anchor_id) = 32),\n\
          evidence_kind TEXT NOT NULL\n\
            CHECK (evidence_kind IN ('repository-anchor', 'remote-anchor')),\n\
          source_last_open_sequence INTEGER NOT NULL CHECK (source_last_open_sequence > 0),\n\
          status TEXT NOT NULL CHECK (status IN ('prepared', 'completed')),\n\
          completed_open_sequence INTEGER\n\
            CHECK (completed_open_sequence IS NULL OR completed_open_sequence > 0),\n\
          CHECK (source_worktree_id <> target_worktree_id),\n\
          CHECK (\n\
            (status = 'prepared' AND completed_open_sequence IS NULL) OR\n\
            (status = 'completed' AND completed_open_sequence IS NOT NULL)\n\
          ),\n\
          FOREIGN KEY (project_id) REFERENCES projects(project_id)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;",
    },
    Migration {
        version: 4,
        name: "desktop_settings_snapshots",
        sql: "CREATE TABLE desktop_settings_revisions (\n\
          revision INTEGER PRIMARY KEY NOT NULL CHECK (revision > 0),\n\
          endpoint_provider_id TEXT\n\
            CHECK (endpoint_provider_id IS NULL OR length(CAST(endpoint_provider_id AS BLOB)) BETWEEN 1 AND 128),\n\
          endpoint_origin TEXT\n\
            CHECK (endpoint_origin IS NULL OR length(CAST(endpoint_origin AS BLOB)) BETWEEN 1 AND 2048),\n\
          endpoint_scope TEXT CHECK (endpoint_scope IS NULL OR endpoint_scope IN ('local_loopback', 'remote')),\n\
          health_status TEXT CHECK (health_status IS NULL OR health_status IN\n\
            ('not_checked', 'healthy', 'capability_limited', 'unreachable', 'cancelled', 'remote_blocked')),\n\
          health_checked_at_unix_millis INTEGER\n\
            CHECK (health_checked_at_unix_millis IS NULL OR health_checked_at_unix_millis >= 0),\n\
          CHECK ((endpoint_provider_id IS NULL AND endpoint_origin IS NULL AND endpoint_scope IS NULL\n\
              AND health_status IS NULL AND health_checked_at_unix_millis IS NULL) OR\n\
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL\n\
              AND endpoint_scope = 'local_loopback' AND health_status = 'not_checked'\n\
              AND health_checked_at_unix_millis IS NULL) OR\n\
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL\n\
              AND endpoint_scope = 'local_loopback'\n\
              AND health_status IN ('healthy', 'capability_limited', 'unreachable', 'cancelled')\n\
              AND health_checked_at_unix_millis IS NOT NULL) OR\n\
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL\n\
              AND endpoint_scope = 'remote' AND health_status = 'remote_blocked'\n\
              AND health_checked_at_unix_millis IS NULL))\n\
          ) STRICT;\n\
          CREATE TABLE desktop_llm_profiles (\n\
          revision INTEGER NOT NULL CHECK (revision > 0),\n\
          role TEXT NOT NULL CHECK (role IN ('coding', 'mapping')),\n\
          provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),\n\
          model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),\n\
          context_tokens INTEGER NOT NULL CHECK (context_tokens BETWEEN 1024 AND 1048576),\n\
          output_tokens INTEGER NOT NULL CHECK (output_tokens BETWEEN 1 AND 262144\n\
            AND output_tokens <= context_tokens),\n\
          parallelism INTEGER NOT NULL CHECK (parallelism BETWEEN 1 AND 64),\n\
          temperature_milli INTEGER NOT NULL CHECK (temperature_milli BETWEEN 0 AND 2000),\n\
          top_p_milli INTEGER NOT NULL CHECK (top_p_milli BETWEEN 1 AND 1000),\n\
          schema_grounding TEXT NOT NULL CHECK (schema_grounding IN ('format_only', 'repeat_in_prompt')),\n\
          structured_output TEXT NOT NULL CHECK (structured_output IN ('verified', 'unavailable')),\n\
          tool_call_mode TEXT NOT NULL CHECK (tool_call_mode IN ('disabled', 'native_reported')),\n\
          probed_at_unix_millis INTEGER NOT NULL CHECK (probed_at_unix_millis >= 0),\n\
          PRIMARY KEY (revision, role),\n\
          FOREIGN KEY (revision) REFERENCES desktop_settings_revisions(revision)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          CREATE TABLE desktop_embedding_profiles (\n\
          revision INTEGER PRIMARY KEY NOT NULL CHECK (revision > 0),\n\
          provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),\n\
          model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),\n\
          dimension INTEGER NOT NULL CHECK (dimension BETWEEN 1 AND 8192),\n\
          max_batch_size INTEGER NOT NULL CHECK (max_batch_size BETWEEN 1 AND 64),\n\
          probed_at_unix_millis INTEGER NOT NULL CHECK (probed_at_unix_millis >= 0),\n\
          FOREIGN KEY (revision) REFERENCES desktop_settings_revisions(revision)\n\
            ON UPDATE RESTRICT ON DELETE RESTRICT\n\
          ) STRICT;\n\
          CREATE TRIGGER desktop_settings_revisions_update_guard\n\
          BEFORE UPDATE ON desktop_settings_revisions BEGIN\n\
            SELECT RAISE(ABORT, 'desktop settings revisions are immutable');\n\
          END;\n\
          CREATE TRIGGER desktop_settings_revisions_delete_guard\n\
          BEFORE DELETE ON desktop_settings_revisions BEGIN\n\
            SELECT RAISE(ABORT, 'desktop settings revisions are append-only');\n\
          END;\n\
          CREATE TRIGGER desktop_llm_profiles_update_guard\n\
          BEFORE UPDATE ON desktop_llm_profiles BEGIN\n\
            SELECT RAISE(ABORT, 'desktop llm profiles are immutable');\n\
          END;\n\
          CREATE TRIGGER desktop_llm_profiles_delete_guard\n\
          BEFORE DELETE ON desktop_llm_profiles BEGIN\n\
            SELECT RAISE(ABORT, 'desktop llm profiles are append-only');\n\
          END;\n\
          CREATE TRIGGER desktop_embedding_profiles_update_guard\n\
          BEFORE UPDATE ON desktop_embedding_profiles BEGIN\n\
            SELECT RAISE(ABORT, 'desktop embedding profiles are immutable');\n\
          END;\n\
          CREATE TRIGGER desktop_embedding_profiles_delete_guard\n\
          BEFORE DELETE ON desktop_embedding_profiles BEGIN\n\
            SELECT RAISE(ABORT, 'desktop embedding profiles are append-only');\n\
          END;",
    },
    Migration {
        version: 5,
        name: "provider_credential_metadata",
        sql: "DROP TRIGGER desktop_settings_revisions_update_guard;
          DROP TRIGGER desktop_settings_revisions_delete_guard;
          DROP TRIGGER desktop_llm_profiles_update_guard;
          DROP TRIGGER desktop_llm_profiles_delete_guard;
          DROP TRIGGER desktop_embedding_profiles_update_guard;
          DROP TRIGGER desktop_embedding_profiles_delete_guard;
          ALTER TABLE desktop_llm_profiles RENAME TO desktop_llm_profiles_v4;
          ALTER TABLE desktop_embedding_profiles RENAME TO desktop_embedding_profiles_v4;
          ALTER TABLE desktop_settings_revisions RENAME TO desktop_settings_revisions_v4;
          CREATE TABLE desktop_settings_revisions (
          revision INTEGER PRIMARY KEY NOT NULL CHECK (revision > 0),
          endpoint_provider_id TEXT
            CHECK (endpoint_provider_id IS NULL OR length(CAST(endpoint_provider_id AS BLOB)) BETWEEN 1 AND 128),
          endpoint_origin TEXT
            CHECK (endpoint_origin IS NULL OR length(CAST(endpoint_origin AS BLOB)) BETWEEN 1 AND 2048),
          endpoint_scope TEXT CHECK (endpoint_scope IS NULL OR endpoint_scope IN ('local_loopback', 'remote')),
          endpoint_access TEXT CHECK (endpoint_access IS NULL OR endpoint_access IN
            ('local', 'remote_blocked', 'explicit_user_initiated_remote')),
          credential_requirement TEXT NOT NULL CHECK (credential_requirement IN ('none', 'api_key')),
          credential_state TEXT NOT NULL CHECK (credential_state IN
            ('not_required', 'missing', 'storing', 'configured', 'deleting')),
          credential_generation INTEGER NOT NULL CHECK (credential_generation >= 0),
          health_status TEXT CHECK (health_status IS NULL OR health_status IN
            ('not_checked', 'healthy', 'capability_limited', 'unreachable', 'cancelled', 'remote_blocked')),
          health_checked_at_unix_millis INTEGER
            CHECK (health_checked_at_unix_millis IS NULL OR health_checked_at_unix_millis >= 0),
          CHECK (
            (endpoint_provider_id IS NULL AND endpoint_origin IS NULL AND endpoint_scope IS NULL
              AND endpoint_access IS NULL AND credential_requirement = 'none'
              AND credential_state = 'not_required' AND credential_generation = 0
              AND health_status IS NULL AND health_checked_at_unix_millis IS NULL) OR
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL
              AND endpoint_scope = 'local_loopback' AND endpoint_access = 'local'
              AND credential_requirement = 'none' AND credential_state = 'not_required'
              AND credential_generation = 0) OR
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL
              AND endpoint_scope = 'remote' AND endpoint_access = 'remote_blocked'
              AND credential_requirement = 'none' AND credential_state = 'not_required'
              AND credential_generation = 0) OR
            (endpoint_provider_id IS NOT NULL AND endpoint_origin IS NOT NULL
              AND endpoint_scope = 'remote' AND endpoint_access = 'explicit_user_initiated_remote'
              AND credential_requirement = 'api_key' AND credential_state IN
                ('missing', 'storing', 'configured', 'deleting')
              AND ((credential_state = 'missing' AND credential_generation >= 0)
                OR (credential_state <> 'missing' AND credential_generation > 0))))
          CHECK (
            (endpoint_provider_id IS NULL AND health_status IS NULL) OR
            (endpoint_access = 'remote_blocked' AND health_status = 'remote_blocked'
              AND health_checked_at_unix_millis IS NULL) OR
            (endpoint_access IN ('local', 'explicit_user_initiated_remote')
              AND health_status = 'not_checked' AND health_checked_at_unix_millis IS NULL) OR
            (endpoint_access IN ('local', 'explicit_user_initiated_remote')
              AND health_status IN ('healthy', 'capability_limited', 'unreachable', 'cancelled')
              AND health_checked_at_unix_millis IS NOT NULL))
          ) STRICT;
          CREATE TABLE desktop_llm_profiles (
          revision INTEGER NOT NULL CHECK (revision > 0),
          role TEXT NOT NULL CHECK (role IN ('coding', 'mapping')),
          provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),
          model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),
          context_tokens INTEGER NOT NULL CHECK (context_tokens BETWEEN 1024 AND 1048576),
          output_tokens INTEGER NOT NULL CHECK (output_tokens BETWEEN 1 AND 262144
            AND output_tokens <= context_tokens),
          parallelism INTEGER NOT NULL CHECK (parallelism BETWEEN 1 AND 64),
          temperature_milli INTEGER NOT NULL CHECK (temperature_milli BETWEEN 0 AND 2000),
          top_p_milli INTEGER NOT NULL CHECK (top_p_milli BETWEEN 1 AND 1000),
          schema_grounding TEXT NOT NULL CHECK (schema_grounding IN ('format_only', 'repeat_in_prompt')),
          structured_output TEXT NOT NULL CHECK (structured_output IN ('verified', 'unavailable')),
          tool_call_mode TEXT NOT NULL CHECK (tool_call_mode IN ('disabled', 'native_reported')),
          probed_at_unix_millis INTEGER NOT NULL CHECK (probed_at_unix_millis >= 0),
          PRIMARY KEY (revision, role),
          FOREIGN KEY (revision) REFERENCES desktop_settings_revisions(revision)
            ON UPDATE RESTRICT ON DELETE RESTRICT
          ) STRICT;
          CREATE TABLE desktop_embedding_profiles (
          revision INTEGER PRIMARY KEY NOT NULL CHECK (revision > 0),
          provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),
          model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),
          dimension INTEGER NOT NULL CHECK (dimension BETWEEN 1 AND 8192),
          max_batch_size INTEGER NOT NULL CHECK (max_batch_size BETWEEN 1 AND 64),
          probed_at_unix_millis INTEGER NOT NULL CHECK (probed_at_unix_millis >= 0),
          FOREIGN KEY (revision) REFERENCES desktop_settings_revisions(revision)
            ON UPDATE RESTRICT ON DELETE RESTRICT
          ) STRICT;
          INSERT INTO desktop_settings_revisions (
            revision, endpoint_provider_id, endpoint_origin, endpoint_scope, endpoint_access,
            credential_requirement, credential_state, credential_generation,
            health_status, health_checked_at_unix_millis
          ) SELECT revision, endpoint_provider_id, endpoint_origin, endpoint_scope,
            CASE
              WHEN endpoint_scope = 'local_loopback' THEN 'local'
              WHEN endpoint_provider_id = 'gemini' AND endpoint_scope = 'remote'
                AND endpoint_origin = 'https://generativelanguage.googleapis.com'
                THEN 'explicit_user_initiated_remote'
              WHEN endpoint_scope = 'remote' THEN 'remote_blocked'
              ELSE NULL
            END,
            CASE WHEN endpoint_provider_id = 'gemini' AND endpoint_scope = 'remote'
              AND endpoint_origin = 'https://generativelanguage.googleapis.com'
              THEN 'api_key' ELSE 'none' END,
            CASE WHEN endpoint_provider_id = 'gemini' AND endpoint_scope = 'remote'
              AND endpoint_origin = 'https://generativelanguage.googleapis.com'
              THEN 'missing' ELSE 'not_required' END,
            0,
            CASE WHEN endpoint_provider_id = 'gemini' AND endpoint_scope = 'remote'
              AND endpoint_origin = 'https://generativelanguage.googleapis.com'
              THEN 'not_checked' ELSE health_status END,
            CASE WHEN endpoint_provider_id = 'gemini' AND endpoint_scope = 'remote'
              AND endpoint_origin = 'https://generativelanguage.googleapis.com'
              THEN NULL ELSE health_checked_at_unix_millis END
            FROM desktop_settings_revisions_v4;
          INSERT INTO desktop_llm_profiles SELECT * FROM desktop_llm_profiles_v4;
          INSERT INTO desktop_embedding_profiles SELECT * FROM desktop_embedding_profiles_v4;
          DROP TABLE desktop_llm_profiles_v4;
          DROP TABLE desktop_embedding_profiles_v4;
          DROP TABLE desktop_settings_revisions_v4;
          CREATE TRIGGER desktop_settings_revisions_update_guard
          BEFORE UPDATE ON desktop_settings_revisions BEGIN
            SELECT RAISE(ABORT, 'desktop settings revisions are immutable');
          END;
          CREATE TRIGGER desktop_settings_revisions_delete_guard
          BEFORE DELETE ON desktop_settings_revisions BEGIN
            SELECT RAISE(ABORT, 'desktop settings revisions are append-only');
          END;
          CREATE TRIGGER desktop_llm_profiles_update_guard
          BEFORE UPDATE ON desktop_llm_profiles BEGIN
            SELECT RAISE(ABORT, 'desktop llm profiles are immutable');
          END;
          CREATE TRIGGER desktop_llm_profiles_delete_guard
          BEFORE DELETE ON desktop_llm_profiles BEGIN
            SELECT RAISE(ABORT, 'desktop llm profiles are append-only');
          END;
          CREATE TRIGGER desktop_embedding_profiles_update_guard
          BEFORE UPDATE ON desktop_embedding_profiles BEGIN
            SELECT RAISE(ABORT, 'desktop embedding profiles are immutable');
          END;
          CREATE TRIGGER desktop_embedding_profiles_delete_guard
          BEFORE DELETE ON desktop_embedding_profiles BEGIN
            SELECT RAISE(ABORT, 'desktop embedding profiles are append-only');
          END;",
    },
    Migration {
        version: 6,
        name: "project_catalog_search",
        sql: "CREATE INDEX recent_worktrees_activation_order_idx
          ON recent_worktrees (last_open_sequence DESC);
          CREATE VIRTUAL TABLE project_catalog_fts USING fts5(
            worktree_root_display,
            content='recent_worktrees',
            content_rowid='rowid',
            tokenize='trigram'
          );
          INSERT INTO project_catalog_fts(project_catalog_fts) VALUES('rebuild');
          CREATE TRIGGER recent_worktrees_catalog_ai AFTER INSERT ON recent_worktrees BEGIN
            INSERT INTO project_catalog_fts(rowid, worktree_root_display)
              VALUES (new.rowid, new.worktree_root_display);
          END;
          CREATE TRIGGER recent_worktrees_catalog_ad AFTER DELETE ON recent_worktrees BEGIN
            INSERT INTO project_catalog_fts(project_catalog_fts, rowid, worktree_root_display)
              VALUES ('delete', old.rowid, old.worktree_root_display);
          END;
          CREATE TRIGGER recent_worktrees_catalog_au AFTER UPDATE OF worktree_root_display
            ON recent_worktrees BEGIN
            INSERT INTO project_catalog_fts(project_catalog_fts, rowid, worktree_root_display)
              VALUES ('delete', old.rowid, old.worktree_root_display);
            INSERT INTO project_catalog_fts(rowid, worktree_root_display)
              VALUES (new.rowid, new.worktree_root_display);
          END;",
    },
    Migration {
        version: 7,
        name: "ui_preferences_snapshots",
        sql: "CREATE TABLE ui_preference_revisions (\n\
          revision INTEGER PRIMARY KEY NOT NULL CHECK (revision > 0),\n\
          agent_session_rail_width INTEGER NOT NULL\n\
            CHECK (agent_session_rail_width BETWEEN 220 AND 360),\n\
          agent_inspector_width INTEGER NOT NULL\n\
            CHECK (agent_inspector_width BETWEEN 320 AND 640),\n\
          agent_session_rail_collapsed INTEGER NOT NULL\n\
            CHECK (agent_session_rail_collapsed IN (0, 1)),\n\
          agent_inspector_collapsed INTEGER NOT NULL\n\
            CHECK (agent_inspector_collapsed IN (0, 1))\n\
          ) STRICT;\n\
          CREATE TRIGGER ui_preference_revisions_update_guard\n\
          BEFORE UPDATE ON ui_preference_revisions BEGIN\n\
            SELECT RAISE(ABORT, 'UI preference revisions are immutable');\n\
          END;\n\
          CREATE TRIGGER ui_preference_revisions_delete_guard\n\
          BEFORE DELETE ON ui_preference_revisions BEGIN\n\
            SELECT RAISE(ABORT, 'UI preference revisions are append-only');\n\
          END;",
    },
];

const KNOWLEDGE_BOOTSTRAP_MIGRATION: Migration = Migration {
    version: 1,
    name: "bootstrap_worktree_knowledge",
    sql: "CREATE TABLE schema_migrations (\n\
      version INTEGER PRIMARY KEY CHECK (version > 0),\n\
      name TEXT NOT NULL UNIQUE,\n\
      checksum BLOB NOT NULL CHECK (length(checksum) = 32)\n\
      ) STRICT;\n\
      CREATE TABLE worktree_storage_identity (\n\
      singleton INTEGER PRIMARY KEY CHECK (singleton = 1),\n\
      repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
      worktree_id BLOB NOT NULL UNIQUE CHECK (length(worktree_id) = 32)\n\
      ) STRICT;",
};

const KNOWLEDGE_PROJECT_INDEX_MIGRATION: Migration = Migration {
    version: 2,
    name: "project_snapshot_index_runs",
    sql: "CREATE TABLE repositories (\n\
      repository_id BLOB PRIMARY KEY NOT NULL CHECK (length(repository_id) = 32)\n\
      ) STRICT;\n\
      CREATE TABLE worktrees (\n\
      worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
      repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
      UNIQUE (worktree_id, repository_id),\n\
      FOREIGN KEY (repository_id) REFERENCES repositories(repository_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO repositories (repository_id)\n\
        SELECT repository_id FROM worktree_storage_identity WHERE singleton = 1;\n\
      INSERT INTO worktrees (worktree_id, repository_id)\n\
        SELECT worktree_id, repository_id FROM worktree_storage_identity WHERE singleton = 1;\n\
      CREATE TABLE snapshots (\n\
      snapshot_id BLOB PRIMARY KEY NOT NULL CHECK (length(snapshot_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      parent_snapshot_id BLOB CHECK (parent_snapshot_id IS NULL OR length(parent_snapshot_id) = 32),\n\
      generation INTEGER NOT NULL CHECK (generation > 0),\n\
      head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
      head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
      head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
      index_schema_version INTEGER NOT NULL\n\
        CHECK (index_schema_version BETWEEN 1 AND 4294967295),\n\
      CHECK (parent_snapshot_id IS NULL OR parent_snapshot_id <> snapshot_id),\n\
      CHECK (\n\
        (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
        (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
      ),\n\
      UNIQUE (worktree_id, generation),\n\
      UNIQUE (snapshot_id, worktree_id),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (parent_snapshot_id, worktree_id)\n\
        REFERENCES snapshots(snapshot_id, worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_adapter_revisions (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      language TEXT NOT NULL\n\
        CHECK (language IN ('generic', 'rust', 'typescript-javascript', 'python')),\n\
      adapter_version TEXT NOT NULL CHECK (length(adapter_version) BETWEEN 1 AND 128),\n\
      PRIMARY KEY (snapshot_id, language),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_changes (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      change_kind TEXT NOT NULL CHECK (change_kind IN ('upsert', 'delete')),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (snapshot_id, repository_path),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE index_runs (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      run_sequence INTEGER NOT NULL CHECK (run_sequence > 0),\n\
      ranking_policy_version INTEGER NOT NULL\n\
        CHECK (ranking_policy_version BETWEEN 1 AND 4294967295),\n\
      status TEXT NOT NULL CHECK (status IN ('building', 'published', 'failed', 'cancelled')),\n\
      UNIQUE (worktree_id, run_sequence),\n\
      FOREIGN KEY (snapshot_id, worktree_id)\n\
        REFERENCES snapshots(snapshot_id, worktree_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE UNIQUE INDEX index_runs_one_building_per_worktree_idx\n\
        ON index_runs (worktree_id) WHERE status = 'building';\n\
      CREATE UNIQUE INDEX index_runs_one_publish_per_snapshot_policy_idx\n\
        ON index_runs (snapshot_id, ranking_policy_version) WHERE status = 'published';\n\
      CREATE INDEX index_runs_worktree_sequence_idx\n\
        ON index_runs (worktree_id, run_sequence DESC);",
};

const KNOWLEDGE_RECONCILIABLE_IDENTITIES_MIGRATION: Migration = Migration {
    version: 3,
    name: "reconciliable_project_identities",
    sql: "CREATE TABLE repositories_v3 (\n\
      repository_id BLOB PRIMARY KEY NOT NULL CHECK (length(repository_id) = 32)\n\
      ) STRICT;\n\
      CREATE TABLE worktrees_v3 (\n\
      worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
      repository_id BLOB NOT NULL CHECK (length(repository_id) = 32),\n\
      UNIQUE (worktree_id, repository_id),\n\
      FOREIGN KEY (repository_id) REFERENCES repositories_v3(repository_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshots_v3 (\n\
      snapshot_id BLOB PRIMARY KEY NOT NULL CHECK (length(snapshot_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      parent_snapshot_id BLOB CHECK (parent_snapshot_id IS NULL OR length(parent_snapshot_id) = 32),\n\
      generation INTEGER NOT NULL CHECK (generation > 0),\n\
      head_kind TEXT NOT NULL CHECK (head_kind IN ('born', 'unborn')),\n\
      head_object_id TEXT CHECK (head_object_id IS NULL OR length(head_object_id) IN (40, 64)),\n\
      head_reference TEXT CHECK (head_reference IS NULL OR length(head_reference) BETWEEN 1 AND 1024),\n\
      index_schema_version INTEGER NOT NULL CHECK (index_schema_version BETWEEN 1 AND 4294967295),\n\
      CHECK (parent_snapshot_id IS NULL OR parent_snapshot_id <> snapshot_id),\n\
      CHECK (\n\
        (head_kind = 'born' AND head_object_id IS NOT NULL) OR\n\
        (head_kind = 'unborn' AND head_object_id IS NULL AND head_reference IS NOT NULL)\n\
      ),\n\
      UNIQUE (worktree_id, generation),\n\
      UNIQUE (snapshot_id, worktree_id),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees_v3(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (parent_snapshot_id, worktree_id)\n\
        REFERENCES snapshots_v3(snapshot_id, worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_adapter_revisions_v3 (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      language TEXT NOT NULL CHECK (language IN ('generic', 'rust', 'typescript-javascript', 'python')),\n\
      adapter_version TEXT NOT NULL CHECK (length(adapter_version) BETWEEN 1 AND 128),\n\
      PRIMARY KEY (snapshot_id, language),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots_v3(snapshot_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE snapshot_changes_v3 (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      change_kind TEXT NOT NULL CHECK (change_kind IN ('upsert', 'delete')),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (snapshot_id, repository_path),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots_v3(snapshot_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE index_runs_v3 (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      run_sequence INTEGER NOT NULL CHECK (run_sequence > 0),\n\
      ranking_policy_version INTEGER NOT NULL CHECK (ranking_policy_version BETWEEN 1 AND 4294967295),\n\
      status TEXT NOT NULL CHECK (status IN ('building', 'published', 'failed', 'cancelled')),\n\
      UNIQUE (worktree_id, run_sequence),\n\
      FOREIGN KEY (snapshot_id, worktree_id)\n\
        REFERENCES snapshots_v3(snapshot_id, worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO repositories_v3 SELECT * FROM repositories;\n\
      INSERT INTO worktrees_v3 SELECT * FROM worktrees;\n\
      INSERT INTO snapshots_v3 SELECT * FROM snapshots;\n\
      INSERT INTO snapshot_adapter_revisions_v3 SELECT * FROM snapshot_adapter_revisions;\n\
      INSERT INTO snapshot_changes_v3 SELECT * FROM snapshot_changes;\n\
      INSERT INTO index_runs_v3 SELECT * FROM index_runs;\n\
      DROP TABLE index_runs;\n\
      DROP TABLE snapshot_changes;\n\
      DROP TABLE snapshot_adapter_revisions;\n\
      DROP TABLE snapshots;\n\
      DROP TABLE worktrees;\n\
      DROP TABLE repositories;\n\
      ALTER TABLE repositories_v3 RENAME TO repositories;\n\
      ALTER TABLE worktrees_v3 RENAME TO worktrees;\n\
      ALTER TABLE snapshots_v3 RENAME TO snapshots;\n\
      ALTER TABLE snapshot_adapter_revisions_v3 RENAME TO snapshot_adapter_revisions;\n\
      ALTER TABLE snapshot_changes_v3 RENAME TO snapshot_changes;\n\
      ALTER TABLE index_runs_v3 RENAME TO index_runs;\n\
      CREATE UNIQUE INDEX index_runs_one_building_per_worktree_idx\n\
        ON index_runs (worktree_id) WHERE status = 'building';\n\
      CREATE UNIQUE INDEX index_runs_one_publish_per_snapshot_policy_idx\n\
        ON index_runs (snapshot_id, ranking_policy_version) WHERE status = 'published';\n\
      CREATE INDEX index_runs_worktree_sequence_idx\n\
        ON index_runs (worktree_id, run_sequence DESC);",
};

const KNOWLEDGE_ATOMIC_INDEX_PUBLICATION_MIGRATION: Migration = Migration {
    version: 4,
    name: "atomic_index_publication",
    sql: "CREATE TABLE file_revisions (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (index_run_id, repository_path),\n\
      UNIQUE (index_run_id, repository_path, content_hash),\n\
      FOREIGN KEY (index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE symbols (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      local_symbol_id INTEGER NOT NULL CHECK (local_symbol_id BETWEEN 1 AND 4294967295),\n\
      kind TEXT NOT NULL CHECK (kind IN (\n\
        'module', 'namespace', 'function', 'method', 'struct', 'enum', 'trait', 'interface',\n\
        'class', 'implementation', 'type-alias', 'constant', 'static', 'variable', 'field',\n\
        'variant', 'parameter'\n\
      )),\n\
      name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 1024),\n\
      signature TEXT CHECK (signature IS NULL OR length(CAST(signature AS BLOB)) BETWEEN 1 AND 16384),\n\
      declaration_start_byte INTEGER NOT NULL CHECK (declaration_start_byte BETWEEN 0 AND 4294967295),\n\
      declaration_end_byte INTEGER NOT NULL CHECK (declaration_end_byte BETWEEN declaration_start_byte AND 4294967295),\n\
      declaration_start_row INTEGER NOT NULL CHECK (declaration_start_row BETWEEN 0 AND 4294967295),\n\
      declaration_start_column INTEGER NOT NULL CHECK (declaration_start_column BETWEEN 0 AND 4294967295),\n\
      declaration_end_row INTEGER NOT NULL CHECK (declaration_end_row BETWEEN 0 AND 4294967295),\n\
      declaration_end_column INTEGER NOT NULL CHECK (declaration_end_column BETWEEN 0 AND 4294967295),\n\
      selection_start_byte INTEGER NOT NULL CHECK (selection_start_byte BETWEEN 0 AND 4294967295),\n\
      selection_end_byte INTEGER NOT NULL CHECK (selection_end_byte BETWEEN selection_start_byte AND 4294967295),\n\
      selection_start_row INTEGER NOT NULL CHECK (selection_start_row BETWEEN 0 AND 4294967295),\n\
      selection_start_column INTEGER NOT NULL CHECK (selection_start_column BETWEEN 0 AND 4294967295),\n\
      selection_end_row INTEGER NOT NULL CHECK (selection_end_row BETWEEN 0 AND 4294967295),\n\
      selection_end_column INTEGER NOT NULL CHECK (selection_end_column BETWEEN 0 AND 4294967295),\n\
      documentation_start_byte INTEGER CHECK (documentation_start_byte IS NULL OR documentation_start_byte BETWEEN 0 AND 4294967295),\n\
      documentation_end_byte INTEGER CHECK (documentation_end_byte IS NULL OR documentation_end_byte BETWEEN documentation_start_byte AND 4294967295),\n\
      documentation_start_row INTEGER CHECK (documentation_start_row IS NULL OR documentation_start_row BETWEEN 0 AND 4294967295),\n\
      documentation_start_column INTEGER CHECK (documentation_start_column IS NULL OR documentation_start_column BETWEEN 0 AND 4294967295),\n\
      documentation_end_row INTEGER CHECK (documentation_end_row IS NULL OR documentation_end_row BETWEEN 0 AND 4294967295),\n\
      documentation_end_column INTEGER CHECK (documentation_end_column IS NULL OR documentation_end_column BETWEEN 0 AND 4294967295),\n\
      visibility TEXT NOT NULL CHECK (visibility IN ('public', 'protected', 'private', 'internal', 'local', 'unknown')),\n\
      roles INTEGER NOT NULL CHECK (roles BETWEEN 0 AND 3),\n\
      CHECK ((documentation_start_byte IS NULL) = (documentation_end_byte IS NULL)),\n\
      CHECK ((documentation_start_byte IS NULL) = (documentation_start_row IS NULL)),\n\
      CHECK ((documentation_start_byte IS NULL) = (documentation_start_column IS NULL)),\n\
      CHECK ((documentation_start_byte IS NULL) = (documentation_end_row IS NULL)),\n\
      CHECK ((documentation_start_byte IS NULL) = (documentation_end_column IS NULL)),\n\
      PRIMARY KEY (index_run_id, symbol_id),\n\
      FOREIGN KEY (index_run_id, repository_path, content_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE symbol_edges (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      edge_sequence INTEGER NOT NULL CHECK (edge_sequence > 0),\n\
      source_kind TEXT NOT NULL CHECK (source_kind IN ('file', 'symbol')),\n\
      source_value BLOB NOT NULL,\n\
      target_kind TEXT NOT NULL CHECK (target_kind IN ('file', 'symbol')),\n\
      target_value BLOB NOT NULL,\n\
      relation_kind TEXT NOT NULL CHECK (relation_kind IN (\n\
        'contains', 'defines', 'imports', 'exports', 'calls', 'implements', 'extends', 'reads',\n\
        'writes', 'configures', 'tests', 'builds', 'documents'\n\
      )),\n\
      provider TEXT NOT NULL CHECK (provider IN ('tree-sitter', 'manifest', 'language-heuristic')),\n\
      confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 10000),\n\
      resolution TEXT NOT NULL CHECK (resolution IN (\n\
        'adapter-local-symbol', 'adapter-file', 'exact-module-reference',\n\
        'unique-file-local-name', 'unique-qualified-name'\n\
      )),\n\
      evidence_path BLOB NOT NULL CHECK (length(evidence_path) BETWEEN 1 AND 131072),\n\
      evidence_hash BLOB NOT NULL CHECK (length(evidence_hash) = 32),\n\
      evidence_start_byte INTEGER NOT NULL CHECK (evidence_start_byte BETWEEN 0 AND 4294967295),\n\
      evidence_end_byte INTEGER NOT NULL CHECK (evidence_end_byte BETWEEN evidence_start_byte AND 4294967295),\n\
      evidence_start_row INTEGER NOT NULL CHECK (evidence_start_row BETWEEN 0 AND 4294967295),\n\
      evidence_start_column INTEGER NOT NULL CHECK (evidence_start_column BETWEEN 0 AND 4294967295),\n\
      evidence_end_row INTEGER NOT NULL CHECK (evidence_end_row BETWEEN 0 AND 4294967295),\n\
      evidence_end_column INTEGER NOT NULL CHECK (evidence_end_column BETWEEN 0 AND 4294967295),\n\
      CHECK ((source_kind = 'symbol' AND length(source_value) = 32) OR\n\
             (source_kind = 'file' AND length(source_value) BETWEEN 1 AND 131072)),\n\
      CHECK ((target_kind = 'symbol' AND length(target_value) = 32) OR\n\
             (target_kind = 'file' AND length(target_value) BETWEEN 1 AND 131072)),\n\
      PRIMARY KEY (index_run_id, edge_sequence),\n\
      FOREIGN KEY (index_run_id, evidence_path, evidence_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE unresolved_edges (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      candidate_sequence INTEGER NOT NULL CHECK (candidate_sequence > 0),\n\
      source_kind TEXT NOT NULL CHECK (source_kind IN ('file', 'symbol')),\n\
      source_value BLOB NOT NULL,\n\
      target_kind TEXT NOT NULL CHECK (target_kind IN ('file', 'reference')),\n\
      target_value BLOB NOT NULL,\n\
      relation_kind TEXT NOT NULL CHECK (relation_kind IN (\n\
        'contains', 'defines', 'imports', 'exports', 'calls', 'implements', 'extends', 'reads',\n\
        'writes', 'configures', 'tests', 'builds', 'documents'\n\
      )),\n\
      provider TEXT NOT NULL CHECK (provider IN ('tree-sitter', 'manifest', 'language-heuristic')),\n\
      confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 10000),\n\
      reason TEXT NOT NULL CHECK (reason IN (\n\
        'no-deterministic-match', 'ambiguous-match', 'dynamic-reference', 'missing-file'\n\
      )),\n\
      evidence_path BLOB NOT NULL CHECK (length(evidence_path) BETWEEN 1 AND 131072),\n\
      evidence_hash BLOB NOT NULL CHECK (length(evidence_hash) = 32),\n\
      evidence_start_byte INTEGER NOT NULL CHECK (evidence_start_byte BETWEEN 0 AND 4294967295),\n\
      evidence_end_byte INTEGER NOT NULL CHECK (evidence_end_byte BETWEEN evidence_start_byte AND 4294967295),\n\
      evidence_start_row INTEGER NOT NULL CHECK (evidence_start_row BETWEEN 0 AND 4294967295),\n\
      evidence_start_column INTEGER NOT NULL CHECK (evidence_start_column BETWEEN 0 AND 4294967295),\n\
      evidence_end_row INTEGER NOT NULL CHECK (evidence_end_row BETWEEN 0 AND 4294967295),\n\
      evidence_end_column INTEGER NOT NULL CHECK (evidence_end_column BETWEEN 0 AND 4294967295),\n\
      CHECK ((source_kind = 'symbol' AND length(source_value) = 32) OR\n\
             (source_kind = 'file' AND length(source_value) BETWEEN 1 AND 131072)),\n\
      CHECK ((target_kind = 'reference' AND length(target_value) BETWEEN 1 AND 4096) OR\n\
             (target_kind = 'file' AND length(target_value) BETWEEN 1 AND 131072)),\n\
      PRIMARY KEY (index_run_id, candidate_sequence),\n\
      FOREIGN KEY (index_run_id, evidence_path, evidence_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE ranking_projections (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      rank_order INTEGER NOT NULL CHECK (rank_order > 0),\n\
      score INTEGER NOT NULL CHECK (score BETWEEN 0 AND 4294967295),\n\
      in_degree INTEGER NOT NULL CHECK (in_degree BETWEEN 0 AND 4294967295),\n\
      out_degree INTEGER NOT NULL CHECK (out_degree BETWEEN 0 AND 4294967295),\n\
      centrality INTEGER NOT NULL CHECK (centrality BETWEEN 0 AND 10000),\n\
      degree_contribution INTEGER NOT NULL CHECK (degree_contribution BETWEEN 0 AND 4294967295),\n\
      centrality_contribution INTEGER NOT NULL CHECK (centrality_contribution BETWEEN 0 AND 4294967295),\n\
      entrypoint_contribution INTEGER NOT NULL CHECK (entrypoint_contribution BETWEEN 0 AND 4294967295),\n\
      public_export_contribution INTEGER NOT NULL CHECK (public_export_contribution BETWEEN 0 AND 4294967295),\n\
      manifest_contribution INTEGER NOT NULL CHECK (manifest_contribution BETWEEN 0 AND 4294967295),\n\
      test_contribution INTEGER NOT NULL CHECK (test_contribution BETWEEN 0 AND 4294967295),\n\
      PRIMARY KEY (index_run_id, symbol_id),\n\
      UNIQUE (index_run_id, rank_order),\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX symbol_edges_source_idx ON symbol_edges (index_run_id, source_kind, source_value);\n\
      CREATE INDEX symbol_edges_target_idx ON symbol_edges (index_run_id, target_kind, target_value);",
};

const KNOWLEDGE_EXACT_SEARCH_MIGRATION: Migration = Migration {
    version: 5,
    name: "exact_search_projection",
    sql: "CREATE TABLE exact_search_projections (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      projection_version INTEGER NOT NULL CHECK (projection_version = 1),\n\
      symbol_count INTEGER NOT NULL CHECK (symbol_count BETWEEN 0 AND 1000000),\n\
      manifest_count INTEGER NOT NULL CHECK (manifest_count BETWEEN 0 AND 250000),\n\
      FOREIGN KEY (index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE exact_search_symbols (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      qualified_name TEXT NOT NULL\n\
        CHECK (length(CAST(qualified_name AS BLOB)) BETWEEN 1 AND 16384),\n\
      PRIMARY KEY (index_run_id, symbol_id),\n\
      FOREIGN KEY (index_run_id) REFERENCES exact_search_projections(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE exact_search_manifests (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (index_run_id, repository_path),\n\
      FOREIGN KEY (index_run_id) REFERENCES exact_search_projections(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE,\n\
      FOREIGN KEY (index_run_id, repository_path, content_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX exact_search_symbols_qualified_name_idx\n\
        ON exact_search_symbols (index_run_id, qualified_name, symbol_id);\n\
      CREATE INDEX symbols_exact_name_idx ON symbols (index_run_id, name, symbol_id);\n\
      CREATE INDEX symbols_exact_signature_idx\n\
        ON symbols (index_run_id, signature, symbol_id) WHERE signature IS NOT NULL;",
};

const KNOWLEDGE_LEXICAL_SEARCH_MIGRATION: Migration = Migration {
    version: 6,
    name: "lexical_search_projection",
    sql: "CREATE TABLE lexical_search_projections (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      projection_version INTEGER NOT NULL CHECK (projection_version = 1),\n\
      symbol_count INTEGER NOT NULL CHECK (symbol_count BETWEEN 0 AND 1000000),\n\
      path_count INTEGER NOT NULL CHECK (path_count BETWEEN 0 AND 250000),\n\
      card_count INTEGER NOT NULL CHECK (card_count BETWEEN 0 AND 1000000),\n\
      FOREIGN KEY (index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE VIRTUAL TABLE symbol_fts USING fts5(\n\
        index_run_id UNINDEXED, symbol_id UNINDEXED, repository_path,\n\
        qualified_name, name, signature, tokenize='trigram case_sensitive 0'\n\
      );\n\
      CREATE VIRTUAL TABLE path_fts USING fts5(\n\
        index_run_id UNINDEXED, repository_path UNINDEXED, path,\n\
        tokenize='trigram case_sensitive 0'\n\
      );\n\
      CREATE VIRTUAL TABLE card_fts USING fts5(\n\
        index_run_id UNINDEXED, card_id UNINDEXED, title, purpose, body,\n\
        tokenize='trigram case_sensitive 0'\n\
      );",
};

const KNOWLEDGE_SEMANTIC_EMBEDDING_MIGRATION: Migration = Migration {
    version: 7,
    name: "semantic_embedding_cache",
    sql: "CREATE TABLE semantic_cards (\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      body_hash BLOB NOT NULL CHECK (length(body_hash) = 32),\n\
      normalization_version INTEGER NOT NULL CHECK (normalization_version = 1),\n\
      normalized_body TEXT NOT NULL\n\
        CHECK (length(CAST(normalized_body AS BLOB)) BETWEEN 1 AND 16384),\n\
      PRIMARY KEY (card_id, body_hash)\n\
      ) STRICT;\n\
      CREATE TABLE semantic_card_snapshots (\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      body_hash BLOB NOT NULL CHECK (length(body_hash) = 32),\n\
      PRIMARY KEY (snapshot_id, card_id),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE,\n\
      FOREIGN KEY (card_id, body_hash) REFERENCES semantic_cards(card_id, body_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE embedding_profiles (\n\
      profile_id BLOB PRIMARY KEY NOT NULL CHECK (length(profile_id) = 32),\n\
      provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),\n\
      model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),\n\
      dimensions INTEGER NOT NULL CHECK (dimensions BETWEEN 1 AND 8192),\n\
      data_type TEXT NOT NULL CHECK (data_type = 'float32'),\n\
      quantization TEXT NOT NULL CHECK (quantization = 'none'),\n\
      normalization TEXT NOT NULL CHECK (normalization = 'l2_unit')\n\
      ) STRICT;\n\
      CREATE TABLE embeddings (\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      body_hash BLOB NOT NULL CHECK (length(body_hash) = 32),\n\
      profile_id BLOB NOT NULL CHECK (length(profile_id) = 32),\n\
      vector_bytes BLOB NOT NULL\n\
        CHECK (length(vector_bytes) BETWEEN 4 AND 32768 AND length(vector_bytes) % 4 = 0),\n\
      created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),\n\
      PRIMARY KEY (card_id, profile_id, body_hash),\n\
      FOREIGN KEY (card_id, body_hash) REFERENCES semantic_cards(card_id, body_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE,\n\
      FOREIGN KEY (profile_id) REFERENCES embedding_profiles(profile_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX semantic_card_snapshots_revision_idx\n\
        ON semantic_card_snapshots (card_id, body_hash, snapshot_id);\n\
      CREATE INDEX embeddings_profile_card_idx\n\
        ON embeddings (profile_id, card_id, body_hash);",
};

const KNOWLEDGE_MODULE_PROJECTION_MIGRATION: Migration = Migration {
    version: 8,
    name: "deterministic_module_projection",
    sql: "CREATE TABLE module_projections (\n\
      index_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(index_run_id) = 32),\n\
      policy_version INTEGER NOT NULL CHECK (policy_version > 0),\n\
      file_count INTEGER NOT NULL CHECK (file_count BETWEEN 0 AND 250000),\n\
      symbol_count INTEGER NOT NULL CHECK (symbol_count BETWEEN 0 AND 1000000),\n\
      module_count INTEGER NOT NULL CHECK (module_count BETWEEN 0 AND 250000),\n\
      membership_count INTEGER NOT NULL CHECK (membership_count BETWEEN 0 AND 2000000),\n\
      language_mask INTEGER NOT NULL CHECK (language_mask BETWEEN 0 AND 15),\n\
      repository_entrypoints_truncated INTEGER NOT NULL\n\
        CHECK (repository_entrypoints_truncated IN (0, 1)),\n\
      FOREIGN KEY (index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE modules (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      kind TEXT NOT NULL CHECK (kind IN ('manifest', 'path', 'graph-community')),\n\
      root_kind TEXT NOT NULL CHECK (root_kind IN ('repository', 'directory', 'none')),\n\
      root_path BLOB CHECK (root_path IS NULL OR length(root_path) BETWEEN 1 AND 131072),\n\
      central_symbols_truncated INTEGER NOT NULL CHECK (central_symbols_truncated IN (0, 1)),\n\
      entrypoints_truncated INTEGER NOT NULL CHECK (entrypoints_truncated IN (0, 1)),\n\
      tests_truncated INTEGER NOT NULL CHECK (tests_truncated IN (0, 1)),\n\
      CHECK ((root_kind = 'directory') = (root_path IS NOT NULL)),\n\
      CHECK ((kind = 'graph-community') = (root_kind = 'none')),\n\
      PRIMARY KEY (index_run_id, module_id),\n\
      FOREIGN KEY (index_run_id) REFERENCES module_projections(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_manifests (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      manifest_order INTEGER NOT NULL CHECK (manifest_order > 0),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      PRIMARY KEY (index_run_id, module_id, manifest_order),\n\
      UNIQUE (index_run_id, module_id, repository_path),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, repository_path, content_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_members (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      membership_kind TEXT NOT NULL CHECK (membership_kind IN ('manifest', 'path', 'graph-community')),\n\
      member_path BLOB NOT NULL CHECK (length(member_path) BETWEEN 1 AND 131072),\n\
      member_hash BLOB NOT NULL CHECK (length(member_hash) = 32),\n\
      manifest_path BLOB CHECK (manifest_path IS NULL OR length(manifest_path) BETWEEN 1 AND 131072),\n\
      manifest_hash BLOB CHECK (manifest_hash IS NULL OR length(manifest_hash) = 32),\n\
      CHECK ((manifest_path IS NULL) = (manifest_hash IS NULL)),\n\
      CHECK ((membership_kind = 'manifest') = (manifest_path IS NOT NULL)),\n\
      PRIMARY KEY (index_run_id, module_id, symbol_id),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, member_path, member_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, manifest_path, manifest_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE UNIQUE INDEX module_members_one_primary_idx\n\
        ON module_members (index_run_id, symbol_id)\n\
        WHERE membership_kind IN ('manifest', 'path');\n\
      CREATE TABLE module_membership_evidence (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      evidence_order INTEGER NOT NULL CHECK (evidence_order > 0),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 4294967295),\n\
      end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN start_byte AND 4294967295),\n\
      start_row INTEGER NOT NULL CHECK (start_row BETWEEN 0 AND 4294967295),\n\
      start_column INTEGER NOT NULL CHECK (start_column BETWEEN 0 AND 4294967295),\n\
      end_row INTEGER NOT NULL CHECK (end_row BETWEEN 0 AND 4294967295),\n\
      end_column INTEGER NOT NULL CHECK (end_column BETWEEN 0 AND 4294967295),\n\
      PRIMARY KEY (index_run_id, module_id, symbol_id, evidence_order),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, repository_path, content_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_central_symbols (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      rank_order INTEGER NOT NULL CHECK (rank_order BETWEEN 1 AND 16),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      PRIMARY KEY (index_run_id, module_id, rank_order),\n\
      UNIQUE (index_run_id, module_id, symbol_id),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_entrypoints (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      rank_order INTEGER NOT NULL CHECK (rank_order BETWEEN 1 AND 256),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      PRIMARY KEY (index_run_id, module_id, rank_order),\n\
      UNIQUE (index_run_id, module_id, symbol_id),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_tests (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      rank_order INTEGER NOT NULL CHECK (rank_order BETWEEN 1 AND 256),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      PRIMARY KEY (index_run_id, module_id, rank_order),\n\
      UNIQUE (index_run_id, module_id, symbol_id),\n\
      FOREIGN KEY (index_run_id, module_id) REFERENCES modules(index_run_id, module_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE repository_card_entrypoints (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      rank_order INTEGER NOT NULL CHECK (rank_order BETWEEN 1 AND 256),\n\
      symbol_id BLOB NOT NULL CHECK (length(symbol_id) = 32),\n\
      PRIMARY KEY (index_run_id, rank_order),\n\
      UNIQUE (index_run_id, symbol_id),\n\
      FOREIGN KEY (index_run_id) REFERENCES module_projections(index_run_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (index_run_id, symbol_id) REFERENCES symbols(index_run_id, symbol_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;",
};

const KNOWLEDGE_VERIFIED_MODULE_CARDS_MIGRATION: Migration = Migration {
    version: 9,
    name: "verified_module_cards",
    sql: "CREATE TABLE module_cards (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      card_schema_version INTEGER NOT NULL CHECK (card_schema_version = 1),\n\
      mapper_profile_version INTEGER NOT NULL CHECK (mapper_profile_version = 1),\n\
      confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 10000),\n\
      status TEXT NOT NULL CHECK (status = 'published'),\n\
      PRIMARY KEY (source_index_run_id, card_id),\n\
      UNIQUE (source_index_run_id, module_id),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_card_fields (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      field_kind TEXT NOT NULL CHECK (field_kind IN (\n\
        'title', 'paths', 'purpose', 'responsibilities', 'public-surface', 'entrypoints',\n\
        'dependencies', 'data-flows', 'invariants', 'tests', 'risks', 'open-questions'\n\
      )),\n\
      PRIMARY KEY (source_index_run_id, card_id, field_kind),\n\
      FOREIGN KEY (source_index_run_id, card_id)\n\
        REFERENCES module_cards(source_index_run_id, card_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_card_field_values (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      field_kind TEXT NOT NULL,\n\
      value_index INTEGER NOT NULL CHECK (value_index BETWEEN 0 AND 65535),\n\
      field_value TEXT NOT NULL\n\
        CHECK (length(CAST(field_value AS BLOB)) BETWEEN 1 AND 16384),\n\
      PRIMARY KEY (source_index_run_id, card_id, field_kind, value_index),\n\
      UNIQUE (source_index_run_id, card_id, field_kind, field_value),\n\
      FOREIGN KEY (source_index_run_id, card_id, field_kind)\n\
        REFERENCES module_card_fields(source_index_run_id, card_id, field_kind)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE evidence_refs (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('file', 'symbol', 'graph-edge')),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      symbol_id BLOB CHECK (symbol_id IS NULL OR length(symbol_id) = 32),\n\
      source_kind TEXT CHECK (source_kind IS NULL OR source_kind IN ('file', 'symbol')),\n\
      source_value BLOB,\n\
      target_kind TEXT CHECK (target_kind IS NULL OR target_kind IN ('file', 'symbol')),\n\
      target_value BLOB,\n\
      relation_kind TEXT CHECK (relation_kind IS NULL OR relation_kind IN (\n\
        'contains', 'defines', 'imports', 'exports', 'calls', 'implements', 'extends', 'reads',\n\
        'writes', 'configures', 'tests', 'builds', 'documents'\n\
      )),\n\
      provider TEXT CHECK (provider IS NULL OR provider IN ('tree-sitter', 'manifest', 'language-heuristic')),\n\
      edge_confidence INTEGER CHECK (edge_confidence IS NULL OR edge_confidence BETWEEN 0 AND 10000),\n\
      resolution TEXT CHECK (resolution IS NULL OR resolution IN (\n\
        'adapter-local-symbol', 'adapter-file', 'exact-module-reference',\n\
        'unique-file-local-name', 'unique-qualified-name'\n\
      )),\n\
      start_byte INTEGER CHECK (start_byte IS NULL OR start_byte BETWEEN 0 AND 4294967295),\n\
      end_byte INTEGER CHECK (end_byte IS NULL OR end_byte BETWEEN start_byte AND 4294967295),\n\
      start_row INTEGER CHECK (start_row IS NULL OR start_row BETWEEN 0 AND 4294967295),\n\
      start_column INTEGER CHECK (start_column IS NULL OR start_column BETWEEN 0 AND 4294967295),\n\
      end_row INTEGER CHECK (end_row IS NULL OR end_row BETWEEN 0 AND 4294967295),\n\
      end_column INTEGER CHECK (end_column IS NULL OR end_column BETWEEN 0 AND 4294967295),\n\
      CHECK ((source_kind IS NULL) = (source_value IS NULL)),\n\
      CHECK ((target_kind IS NULL) = (target_value IS NULL)),\n\
      CHECK (source_kind IS NULL OR\n\
        (source_kind = 'symbol' AND length(source_value) = 32) OR\n\
        (source_kind = 'file' AND length(source_value) BETWEEN 1 AND 131072)),\n\
      CHECK (target_kind IS NULL OR\n\
        (target_kind = 'symbol' AND length(target_value) = 32) OR\n\
        (target_kind = 'file' AND length(target_value) BETWEEN 1 AND 131072)),\n\
      CHECK (\n\
        (evidence_kind = 'file' AND symbol_id IS NULL AND source_kind IS NULL AND\n\
          relation_kind IS NULL AND provider IS NULL AND edge_confidence IS NULL AND\n\
          resolution IS NULL AND start_byte IS NULL AND end_byte IS NULL AND\n\
          start_row IS NULL AND start_column IS NULL AND end_row IS NULL AND end_column IS NULL) OR\n\
        (evidence_kind = 'symbol' AND symbol_id IS NOT NULL AND source_kind IS NULL AND\n\
          relation_kind IS NULL AND provider IS NULL AND edge_confidence IS NULL AND\n\
          resolution IS NULL AND start_byte IS NULL AND end_byte IS NULL AND\n\
          start_row IS NULL AND start_column IS NULL AND end_row IS NULL AND end_column IS NULL) OR\n\
        (evidence_kind = 'graph-edge' AND symbol_id IS NULL AND source_kind IS NOT NULL AND\n\
          target_kind IS NOT NULL AND relation_kind IS NOT NULL AND provider IS NOT NULL AND\n\
          edge_confidence IS NOT NULL AND resolution IS NOT NULL AND start_byte IS NOT NULL AND\n\
          end_byte IS NOT NULL AND start_row IS NOT NULL AND start_column IS NOT NULL AND\n\
          end_row IS NOT NULL AND end_column IS NOT NULL)\n\
      ),\n\
      PRIMARY KEY (source_index_run_id, evidence_id),\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_card_field_evidence (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      field_kind TEXT NOT NULL,\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      PRIMARY KEY (source_index_run_id, card_id, field_kind, evidence_id),\n\
      FOREIGN KEY (source_index_run_id, card_id, field_kind)\n\
        REFERENCES module_card_fields(source_index_run_id, card_id, field_kind)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (source_index_run_id, evidence_id)\n\
        REFERENCES evidence_refs(source_index_run_id, evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE claims (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      claim_id BLOB NOT NULL CHECK (length(claim_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      field_kind TEXT NOT NULL,\n\
      value_index INTEGER NOT NULL CHECK (value_index BETWEEN 0 AND 65535),\n\
      polarity TEXT NOT NULL CHECK (polarity IN ('affirms', 'denies')),\n\
      predicate_kind TEXT NOT NULL CHECK (predicate_kind IN (\n\
        'path', 'symbol', 'relation', 'observed', 'architectural-intent'\n\
      )),\n\
      statement TEXT CHECK (statement IS NULL OR\n\
        length(CAST(statement AS BLOB)) BETWEEN 1 AND 2048),\n\
      claim_kind TEXT NOT NULL CHECK (claim_kind IN ('fact', 'observation', 'hypothesis')),\n\
      status TEXT NOT NULL CHECK (status = 'active'),\n\
      confidence INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 10000),\n\
      CHECK ((predicate_kind IN ('observed', 'architectural-intent')) = (statement IS NOT NULL)),\n\
      CHECK ((predicate_kind = 'architectural-intent') = (claim_kind = 'hypothesis') OR\n\
        predicate_kind <> 'architectural-intent'),\n\
      CHECK ((predicate_kind = 'observed') = (claim_kind = 'observation') OR\n\
        predicate_kind <> 'observed'),\n\
      CHECK (predicate_kind IN ('observed', 'architectural-intent') OR\n\
        (polarity = 'affirms' AND claim_kind = 'fact') OR\n\
        (polarity = 'denies' AND claim_kind = 'hypothesis')),\n\
      PRIMARY KEY (source_index_run_id, claim_id),\n\
      UNIQUE (source_index_run_id, card_id, field_kind, value_index),\n\
      FOREIGN KEY (source_index_run_id, card_id, field_kind, value_index)\n\
        REFERENCES module_card_field_values(source_index_run_id, card_id, field_kind, value_index)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE claim_evidence (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      claim_id BLOB NOT NULL CHECK (length(claim_id) = 32),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      PRIMARY KEY (source_index_run_id, claim_id, evidence_id),\n\
      FOREIGN KEY (source_index_run_id, claim_id)\n\
        REFERENCES claims(source_index_run_id, claim_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (source_index_run_id, evidence_id)\n\
        REFERENCES evidence_refs(source_index_run_id, evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE claim_relations (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      claim_id BLOB NOT NULL CHECK (length(claim_id) = 32),\n\
      predicate_kind TEXT NOT NULL CHECK (predicate_kind IN ('path', 'symbol', 'relation')),\n\
      predicate_path BLOB CHECK (predicate_path IS NULL OR length(predicate_path) BETWEEN 1 AND 131072),\n\
      predicate_symbol_id BLOB CHECK (predicate_symbol_id IS NULL OR length(predicate_symbol_id) = 32),\n\
      source_kind TEXT CHECK (source_kind IS NULL OR source_kind IN ('file', 'symbol')),\n\
      source_value BLOB,\n\
      target_kind TEXT CHECK (target_kind IS NULL OR target_kind IN ('file', 'symbol')),\n\
      target_value BLOB,\n\
      relation_kind TEXT CHECK (relation_kind IS NULL OR relation_kind IN ('imports', 'exports', 'calls', 'tests')),\n\
      CHECK ((source_kind IS NULL) = (source_value IS NULL)),\n\
      CHECK ((target_kind IS NULL) = (target_value IS NULL)),\n\
      CHECK (source_kind IS NULL OR\n\
        (source_kind = 'symbol' AND length(source_value) = 32) OR\n\
        (source_kind = 'file' AND length(source_value) BETWEEN 1 AND 131072)),\n\
      CHECK (target_kind IS NULL OR\n\
        (target_kind = 'symbol' AND length(target_value) = 32) OR\n\
        (target_kind = 'file' AND length(target_value) BETWEEN 1 AND 131072)),\n\
      CHECK (\n\
        (predicate_kind = 'path' AND predicate_path IS NOT NULL AND predicate_symbol_id IS NULL AND\n\
          source_kind IS NULL AND target_kind IS NULL AND relation_kind IS NULL) OR\n\
        (predicate_kind = 'symbol' AND predicate_path IS NULL AND predicate_symbol_id IS NOT NULL AND\n\
          source_kind IS NULL AND target_kind IS NULL AND relation_kind IS NULL) OR\n\
        (predicate_kind = 'relation' AND predicate_path IS NULL AND predicate_symbol_id IS NULL AND\n\
          source_kind IS NOT NULL AND target_kind IS NOT NULL AND relation_kind IS NOT NULL)\n\
      ),\n\
      PRIMARY KEY (source_index_run_id, claim_id),\n\
      FOREIGN KEY (source_index_run_id, claim_id)\n\
        REFERENCES claims(source_index_run_id, claim_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE INDEX module_cards_snapshot_idx ON module_cards (snapshot_id, source_index_run_id);\n\
      CREATE INDEX claims_card_idx ON claims (source_index_run_id, card_id);\n\
      CREATE INDEX evidence_refs_snapshot_idx ON evidence_refs (snapshot_id, evidence_kind);",
};

const KNOWLEDGE_CARD_INVALIDATION_MIGRATION: Migration = Migration {
    version: 10,
    name: "card_invalidation_and_remap",
    sql: "CREATE TABLE module_card_lifecycle (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      status TEXT NOT NULL CHECK (status IN ('published', 'stale', 'needs-review')),\n\
      invalidated_by_index_run_id BLOB\n\
        CHECK (invalidated_by_index_run_id IS NULL OR length(invalidated_by_index_run_id) = 32),\n\
      reason TEXT CHECK (reason IS NULL OR reason IN (\n\
        'evidence-changed', 'module-removed', 'parser-version-changed',\n\
        'mapper-version-changed', 'direct-dependency-changed'\n\
      )),\n\
      CHECK (\n\
        (status = 'published' AND invalidated_by_index_run_id IS NULL AND reason IS NULL) OR\n\
        (status <> 'published' AND invalidated_by_index_run_id IS NOT NULL AND reason IS NOT NULL)\n\
      ),\n\
      PRIMARY KEY (source_index_run_id, card_id),\n\
      FOREIGN KEY (source_index_run_id, card_id)\n\
        REFERENCES module_cards(source_index_run_id, card_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO module_card_lifecycle (source_index_run_id, card_id, status)\n\
        SELECT source_index_run_id, card_id, 'published' FROM module_cards;\n\
      CREATE TABLE claim_lifecycle (\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      claim_id BLOB NOT NULL CHECK (length(claim_id) = 32),\n\
      status TEXT NOT NULL CHECK (status IN ('active', 'stale')),\n\
      invalidated_by_index_run_id BLOB\n\
        CHECK (invalidated_by_index_run_id IS NULL OR length(invalidated_by_index_run_id) = 32),\n\
      reason TEXT CHECK (reason IS NULL OR reason IN (\n\
        'evidence-changed', 'module-removed', 'parser-version-changed',\n\
        'mapper-version-changed', 'direct-dependency-changed'\n\
      )),\n\
      CHECK (\n\
        (status = 'active' AND invalidated_by_index_run_id IS NULL AND reason IS NULL) OR\n\
        (status = 'stale' AND invalidated_by_index_run_id IS NOT NULL AND reason IS NOT NULL)\n\
      ),\n\
      PRIMARY KEY (source_index_run_id, claim_id),\n\
      FOREIGN KEY (source_index_run_id, claim_id)\n\
        REFERENCES claims(source_index_run_id, claim_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO claim_lifecycle (source_index_run_id, claim_id, status)\n\
        SELECT source_index_run_id, claim_id, 'active' FROM claims;\n\
      CREATE TABLE evidence_invalidations (\n\
      target_index_run_id BLOB NOT NULL CHECK (length(target_index_run_id) = 32),\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      reason TEXT NOT NULL CHECK (reason = 'evidence-changed'),\n\
      PRIMARY KEY (target_index_run_id, source_index_run_id, evidence_id),\n\
      FOREIGN KEY (source_index_run_id, evidence_id)\n\
        REFERENCES evidence_refs(source_index_run_id, evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE module_remap_queue (\n\
      module_id BLOB PRIMARY KEY NOT NULL CHECK (length(module_id) = 32),\n\
      source_index_run_id BLOB NOT NULL CHECK (length(source_index_run_id) = 32),\n\
      card_id BLOB NOT NULL CHECK (length(card_id) = 32),\n\
      target_index_run_id BLOB NOT NULL CHECK (length(target_index_run_id) = 32),\n\
      target_snapshot_id BLOB NOT NULL CHECK (length(target_snapshot_id) = 32),\n\
      priority INTEGER NOT NULL CHECK (priority IN (0, 1)),\n\
      reason TEXT NOT NULL CHECK (reason IN (\n\
        'evidence-changed', 'parser-version-changed', 'mapper-version-changed',\n\
        'direct-dependency-changed'\n\
      )),\n\
      FOREIGN KEY (source_index_run_id, card_id)\n\
        REFERENCES module_cards(source_index_run_id, card_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (target_snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE INDEX module_card_lifecycle_status_idx\n\
        ON module_card_lifecycle (status, source_index_run_id, card_id);\n\
      CREATE INDEX claim_lifecycle_status_idx\n\
        ON claim_lifecycle (status, source_index_run_id, claim_id);\n\
      CREATE INDEX module_remap_queue_priority_idx\n\
        ON module_remap_queue (priority, module_id);",
};

const KNOWLEDGE_GOAL_CONTRACT_MIGRATION: Migration = Migration {
    version: 11,
    name: "revisioned_goal_contracts",
    sql: "CREATE TABLE tasks (\n\
      task_id BLOB PRIMARY KEY NOT NULL CHECK (length(task_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      current_goal_revision INTEGER NOT NULL\n\
        CHECK (current_goal_revision BETWEEN 1 AND 4294967295),\n\
      UNIQUE (task_id, worktree_id),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (task_id, current_goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED\n\
      ) STRICT;\n\
      CREATE TABLE goal_contract_revisions (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 4294967295),\n\
      previous_revision INTEGER\n\
        CHECK (previous_revision IS NULL OR previous_revision BETWEEN 1 AND 4294967295),\n\
      objective TEXT NOT NULL\n\
        CHECK (length(CAST(objective AS BLOB)) BETWEEN 1 AND 16384),\n\
      success_verification TEXT NOT NULL\n\
        CHECK (length(CAST(success_verification AS BLOB)) BETWEEN 1 AND 8192),\n\
      revision_reason TEXT\n\
        CHECK (revision_reason IS NULL OR\n\
          length(CAST(revision_reason AS BLOB)) BETWEEN 1 AND 4096),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      CHECK (\n\
        (revision = 1 AND previous_revision IS NULL AND revision_reason IS NULL) OR\n\
        (revision > 1 AND previous_revision = revision - 1 AND revision_reason IS NOT NULL)\n\
      ),\n\
      PRIMARY KEY (task_id, revision),\n\
      FOREIGN KEY (task_id) REFERENCES tasks(task_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (task_id, previous_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED\n\
      ) STRICT;\n\
      CREATE TABLE acceptance_criteria (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      criterion_id BLOB NOT NULL CHECK (length(criterion_id) = 32),\n\
      statement TEXT NOT NULL\n\
        CHECK (length(CAST(statement AS BLOB)) BETWEEN 1 AND 4096),\n\
      PRIMARY KEY (task_id, goal_revision, item_sequence),\n\
      UNIQUE (task_id, goal_revision, criterion_id),\n\
      UNIQUE (task_id, goal_revision, statement),\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE goal_contract_constraints (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      statement TEXT NOT NULL\n\
        CHECK (length(CAST(statement AS BLOB)) BETWEEN 1 AND 4096),\n\
      PRIMARY KEY (task_id, goal_revision, item_sequence),\n\
      UNIQUE (task_id, goal_revision, statement),\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE goal_contract_non_goals (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      statement TEXT NOT NULL\n\
        CHECK (length(CAST(statement AS BLOB)) BETWEEN 1 AND 4096),\n\
      PRIMARY KEY (task_id, goal_revision, item_sequence),\n\
      UNIQUE (task_id, goal_revision, statement),\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE goal_contract_user_decisions (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      statement TEXT NOT NULL\n\
        CHECK (length(CAST(statement AS BLOB)) BETWEEN 1 AND 4096),\n\
      PRIMARY KEY (task_id, goal_revision, item_sequence),\n\
      UNIQUE (task_id, goal_revision, statement),\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE INDEX tasks_worktree_idx ON tasks(worktree_id, task_id);\n\
      CREATE INDEX goal_contract_revisions_created_idx\n\
        ON goal_contract_revisions(task_id, created_at_unix_millis, revision);",
};

const KNOWLEDGE_TASK_LEDGER_MIGRATION: Migration = Migration {
    version: 12,
    name: "materialized_task_ledger",
    sql: "CREATE TABLE task_ledgers (\n\
      task_id BLOB PRIMARY KEY NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      plan_revision INTEGER NOT NULL CHECK (plan_revision BETWEEN 1 AND 4294967295),\n\
      store_version INTEGER NOT NULL CHECK (store_version >= 1),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      updated_at_unix_millis INTEGER NOT NULL\n\
        CHECK (updated_at_unix_millis >= created_at_unix_millis),\n\
      FOREIGN KEY (task_id) REFERENCES tasks(task_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE task_steps (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      parent_step_id BLOB CHECK (parent_step_id IS NULL OR length(parent_step_id) = 32),\n\
      intended_outcome TEXT NOT NULL\n\
        CHECK (length(CAST(intended_outcome AS BLOB)) BETWEEN 1 AND 8192),\n\
      rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 8192),\n\
      verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),\n\
      verification_method TEXT NOT NULL\n\
        CHECK (verification_method IN ('command', 'test', 'diff_invariant',\n\
          'diagnostic', 'user_confirm')),\n\
      verification_requirement TEXT NOT NULL\n\
        CHECK (length(CAST(verification_requirement AS BLOB)) BETWEEN 1 AND 8192),\n\
      introduced_plan_revision INTEGER NOT NULL\n\
        CHECK (introduced_plan_revision BETWEEN 1 AND 4294967295),\n\
      retired_plan_revision INTEGER\n\
        CHECK (retired_plan_revision IS NULL OR\n\
          retired_plan_revision BETWEEN 2 AND 4294967295),\n\
      status TEXT NOT NULL CHECK (status IN ('pending', 'ready', 'in_progress', 'blocked',\n\
        'awaiting_approval', 'verifying', 'completed', 'failed', 'cancelled', 'stale')),\n\
      blocking_reason TEXT\n\
        CHECK (blocking_reason IS NULL OR length(CAST(blocking_reason AS BLOB)) BETWEEN 1 AND 4096),\n\
      stale_kind TEXT CHECK (stale_kind IS NULL OR stale_kind IN ('verification_evidence',\n\
        'dependency')),\n\
      stale_dependency_step_id BLOB\n\
        CHECK (stale_dependency_step_id IS NULL OR length(stale_dependency_step_id) = 32),\n\
      CHECK (parent_step_id IS NULL OR parent_step_id <> step_id),\n\
      CHECK (retired_plan_revision IS NULL OR\n\
        retired_plan_revision > introduced_plan_revision),\n\
      CHECK ((status IN ('blocked', 'awaiting_approval')) = (blocking_reason IS NOT NULL)),\n\
      CHECK ((status = 'stale') = (stale_kind IS NOT NULL)),\n\
      CHECK ((stale_kind = 'dependency') = (stale_dependency_step_id IS NOT NULL)),\n\
      PRIMARY KEY (task_id, step_id),\n\
      UNIQUE (task_id, step_id, verification_spec_id),\n\
      UNIQUE (task_id, verification_spec_id),\n\
      FOREIGN KEY (task_id) REFERENCES task_ledgers(task_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, parent_step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED\n\
      ) STRICT;\n\
      CREATE TABLE task_step_dependencies (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      prerequisite_step_id BLOB NOT NULL CHECK (length(prerequisite_step_id) = 32),\n\
      CHECK (step_id <> prerequisite_step_id),\n\
      PRIMARY KEY (task_id, step_id, item_sequence),\n\
      UNIQUE (task_id, step_id, prerequisite_step_id),\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, prerequisite_step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED\n\
      ) STRICT;\n\
      CREATE TABLE task_step_expected_evidence (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 32),\n\
      description TEXT NOT NULL\n\
        CHECK (length(CAST(description AS BLOB)) BETWEEN 1 AND 4096),\n\
      PRIMARY KEY (task_id, step_id, item_sequence),\n\
      UNIQUE (task_id, step_id, description),\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_step_attempts (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      started_at_unix_millis INTEGER NOT NULL CHECK (started_at_unix_millis >= 0),\n\
      finished_at_unix_millis INTEGER\n\
        CHECK (finished_at_unix_millis IS NULL OR\n\
          finished_at_unix_millis >= started_at_unix_millis),\n\
      outcome TEXT NOT NULL CHECK (outcome IN ('active', 'blocked', 'verification_failed',\n\
        'completed', 'failed', 'cancelled')),\n\
      outcome_reason TEXT\n\
        CHECK (outcome_reason IS NULL OR length(CAST(outcome_reason AS BLOB)) BETWEEN 1 AND 4096),\n\
      result_summary TEXT\n\
        CHECK (result_summary IS NULL OR length(CAST(result_summary AS BLOB)) BETWEEN 1 AND 8192),\n\
      CHECK ((outcome = 'active') = (finished_at_unix_millis IS NULL)),\n\
      CHECK ((outcome IN ('blocked', 'failed', 'cancelled')) = (outcome_reason IS NOT NULL)),\n\
      PRIMARY KEY (task_id, step_id, attempt_number),\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_step_attempt_evidence (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      PRIMARY KEY (task_id, step_id, attempt_number, item_sequence),\n\
      UNIQUE (task_id, step_id, attempt_number, evidence_id),\n\
      FOREIGN KEY (task_id, step_id, attempt_number)\n\
        REFERENCES task_step_attempts(task_id, step_id, attempt_number)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_step_verifications (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),\n\
      verification_id BLOB NOT NULL CHECK (length(verification_id) = 32),\n\
      verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),\n\
      failure_summary TEXT\n\
        CHECK (failure_summary IS NULL OR length(CAST(failure_summary AS BLOB)) BETWEEN 1 AND 8192),\n\
      verified_at_unix_millis INTEGER NOT NULL CHECK (verified_at_unix_millis >= 0),\n\
      CHECK ((outcome = 'failed') = (failure_summary IS NOT NULL)),\n\
      PRIMARY KEY (task_id, step_id, attempt_number),\n\
      UNIQUE (task_id, verification_id),\n\
      FOREIGN KEY (task_id, step_id, attempt_number)\n\
        REFERENCES task_step_attempts(task_id, step_id, attempt_number)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, step_id, verification_spec_id)\n\
        REFERENCES task_steps(task_id, step_id, verification_spec_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE task_step_verification_evidence (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 4294967295),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      PRIMARY KEY (task_id, step_id, attempt_number, item_sequence),\n\
      UNIQUE (task_id, step_id, attempt_number, evidence_id),\n\
      FOREIGN KEY (task_id, step_id, attempt_number)\n\
        REFERENCES task_step_verifications(task_id, step_id, attempt_number)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_step_stale_evidence (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      PRIMARY KEY (task_id, step_id, item_sequence),\n\
      UNIQUE (task_id, step_id, evidence_id),\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_ledger_replans (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      plan_revision INTEGER NOT NULL CHECK (plan_revision BETWEEN 2 AND 4294967295),\n\
      previous_plan_revision INTEGER NOT NULL\n\
        CHECK (previous_plan_revision BETWEEN 1 AND 4294967294),\n\
      reason TEXT NOT NULL CHECK (length(CAST(reason AS BLOB)) BETWEEN 1 AND 4096),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      CHECK (previous_plan_revision = plan_revision - 1),\n\
      PRIMARY KEY (task_id, plan_revision),\n\
      FOREIGN KEY (task_id) REFERENCES task_ledgers(task_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_ledger_replan_retirements (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      plan_revision INTEGER NOT NULL CHECK (plan_revision BETWEEN 2 AND 4294967295),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      PRIMARY KEY (task_id, plan_revision, step_id),\n\
      FOREIGN KEY (task_id, plan_revision)\n\
        REFERENCES task_ledger_replans(task_id, plan_revision)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE task_ledger_replan_additions (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      plan_revision INTEGER NOT NULL CHECK (plan_revision BETWEEN 2 AND 4294967295),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      PRIMARY KEY (task_id, plan_revision, step_id),\n\
      FOREIGN KEY (task_id, plan_revision)\n\
        REFERENCES task_ledger_replans(task_id, plan_revision)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX task_ledgers_updated_idx ON task_ledgers(updated_at_unix_millis, task_id);\n\
      CREATE INDEX task_steps_status_idx ON task_steps(task_id, status, step_id);",
};

const KNOWLEDGE_RUN_JOURNAL_MIGRATION: Migration = Migration {
    version: 13,
    name: "append_only_run_journal",
    sql: "CREATE TABLE agent_runs (\n\
      run_id BLOB PRIMARY KEY NOT NULL CHECK (length(run_id) = 32),\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      task_ledger_revision INTEGER NOT NULL\n\
        CHECK (task_ledger_revision BETWEEN 1 AND 4294967295),\n\
      controller_state TEXT NOT NULL CHECK (controller_state IN ('intake', 'localize', 'plan',\n\
        'execute', 'verify', 'replan', 'await_approval', 'done', 'failed', 'cancelled')),\n\
      last_event_sequence INTEGER NOT NULL CHECK (last_event_sequence >= 1),\n\
      current_snapshot_id BLOB NOT NULL CHECK (length(current_snapshot_id) = 32),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      updated_at_unix_millis INTEGER NOT NULL\n\
        CHECK (updated_at_unix_millis >= created_at_unix_millis),\n\
      FOREIGN KEY (task_id) REFERENCES task_ledgers(task_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (task_id, goal_revision)\n\
        REFERENCES goal_contract_revisions(task_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (current_snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE run_events (\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      event_sequence INTEGER NOT NULL CHECK (event_sequence >= 1),\n\
      event_id BLOB NOT NULL CHECK (length(event_id) = 32),\n\
      occurred_at_unix_millis INTEGER NOT NULL CHECK (occurred_at_unix_millis >= 0),\n\
      event_kind TEXT NOT NULL CHECK (event_kind IN ('run_started', 'state_transition',\n\
        'context_compiled', 'model_interaction', 'tool_action', 'ledger_updated',\n\
        'verification_recorded', 'approval_recorded', 'diagnostic')),\n\
      state_from TEXT CHECK (state_from IS NULL OR state_from IN ('intake', 'localize', 'plan',\n\
        'execute', 'verify', 'replan', 'await_approval', 'done', 'failed', 'cancelled')),\n\
      state_to TEXT CHECK (state_to IS NULL OR state_to IN ('intake', 'localize', 'plan',\n\
        'execute', 'verify', 'replan', 'await_approval', 'done', 'failed', 'cancelled')),\n\
      ledger_revision_from INTEGER CHECK (ledger_revision_from IS NULL OR\n\
        ledger_revision_from BETWEEN 1 AND 4294967294),\n\
      ledger_revision_to INTEGER CHECK (ledger_revision_to IS NULL OR\n\
        ledger_revision_to BETWEEN 2 AND 4294967295),\n\
      payload_schema_version INTEGER NOT NULL CHECK (payload_schema_version = 1),\n\
      payload_code TEXT NOT NULL CHECK (payload_code IN ('none', 'user_request',\n\
        'controller_decision', 'policy_decision', 'timeout', 'cancellation',\n\
        'invalid_model_output', 'tool_failure', 'verification_failure', 'state_recovered')),\n\
      payload_outcome TEXT CHECK (payload_outcome IS NULL OR\n\
        payload_outcome IN ('succeeded', 'failed', 'cancelled', 'denied')),\n\
      redaction_source TEXT CHECK (redaction_source IS NULL OR\n\
        redaction_source IN ('untrusted_text', 'model_output', 'tool_output', 'external_error')),\n\
      redaction_observed_bytes INTEGER\n\
        CHECK (redaction_observed_bytes IS NULL OR redaction_observed_bytes >= 0),\n\
      redaction_source_truncated INTEGER\n\
        CHECK (redaction_source_truncated IS NULL OR redaction_source_truncated IN (0, 1)),\n\
      payload_digest BLOB NOT NULL CHECK (length(payload_digest) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      subject_kind TEXT CHECK (subject_kind IS NULL OR subject_kind IN ('tool', 'evidence')),\n\
      subject_id BLOB CHECK (subject_id IS NULL OR length(subject_id) = 32),\n\
      CHECK ((event_sequence = 1) = (event_kind = 'run_started')),\n\
      CHECK ((event_kind = 'state_transition') = (state_from IS NOT NULL)),\n\
      CHECK ((state_from IS NULL) = (state_to IS NULL)),\n\
      CHECK (state_from IS NULL OR state_from <> state_to),\n\
      CHECK ((event_kind = 'ledger_updated') = (ledger_revision_from IS NOT NULL)),\n\
      CHECK ((ledger_revision_from IS NULL) = (ledger_revision_to IS NULL)),\n\
      CHECK (ledger_revision_from IS NULL OR ledger_revision_to = ledger_revision_from + 1),\n\
      CHECK ((redaction_source IS NULL) = (redaction_observed_bytes IS NULL)),\n\
      CHECK ((redaction_source IS NULL) = (redaction_source_truncated IS NULL)),\n\
      CHECK ((subject_kind IS NULL) = (subject_id IS NULL)),\n\
      PRIMARY KEY (run_id, event_sequence),\n\
      UNIQUE (event_id),\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE INDEX agent_runs_task_state_idx ON agent_runs(task_id, controller_state, run_id);\n\
      CREATE INDEX run_events_subject_idx ON run_events(subject_kind, subject_id, run_id);",
};

const KNOWLEDGE_MODEL_PROFILE_RUN_REFERENCE_MIGRATION: Migration = Migration {
    version: 14,
    name: "model_profile_run_reference",
    sql: "ALTER TABLE agent_runs ADD COLUMN model_profile_id BLOB
        CHECK (model_profile_id IS NULL OR length(model_profile_id) = 32);\n\
      ALTER TABLE agent_runs ADD COLUMN model_profile_schema_version INTEGER
        CHECK (model_profile_schema_version IS NULL OR
          model_profile_schema_version BETWEEN 1 AND 65535);\n\
      CREATE TRIGGER agent_runs_profile_insert_guard
      BEFORE INSERT ON agent_runs
      WHEN (NEW.model_profile_id IS NULL AND NEW.model_profile_schema_version IS NOT NULL)
        OR (NEW.model_profile_id IS NOT NULL AND NEW.model_profile_schema_version IS NULL)
      BEGIN
        SELECT RAISE(ABORT, 'agent run model profile reference is incomplete');
      END;\n\
      CREATE TRIGGER agent_runs_profile_update_guard
      BEFORE UPDATE OF model_profile_id, model_profile_schema_version ON agent_runs
      WHEN (NEW.model_profile_id IS NULL AND NEW.model_profile_schema_version IS NOT NULL)
        OR (NEW.model_profile_id IS NOT NULL AND NEW.model_profile_schema_version IS NULL)
      BEGIN
        SELECT RAISE(ABORT, 'agent run model profile reference is incomplete');
      END;",
};

const KNOWLEDGE_AGENT_RUN_BUDGET_MIGRATION: Migration = Migration {
    version: 15,
    name: "durable_agent_run_budgets",
    sql: "ALTER TABLE agent_runs ADD COLUMN turn_limit INTEGER NOT NULL DEFAULT 128
        CHECK (turn_limit BETWEEN 1 AND 10000);\n\
      ALTER TABLE agent_runs ADD COLUMN prompt_token_limit INTEGER NOT NULL DEFAULT 2097152
        CHECK (prompt_token_limit >= 1);\n\
      ALTER TABLE agent_runs ADD COLUMN output_token_limit INTEGER NOT NULL DEFAULT 524288
        CHECK (output_token_limit >= 1);\n\
      ALTER TABLE agent_runs ADD COLUMN action_limit INTEGER NOT NULL DEFAULT 128
        CHECK (action_limit BETWEEN 1 AND 100000);\n\
      ALTER TABLE agent_runs ADD COLUMN duration_limit_millis INTEGER NOT NULL DEFAULT 7200000
        CHECK (duration_limit_millis BETWEEN 1 AND 604800000);\n\
      ALTER TABLE agent_runs ADD COLUMN repair_limit INTEGER NOT NULL DEFAULT 32
        CHECK (repair_limit BETWEEN 1 AND 10000);\n\
      ALTER TABLE agent_runs ADD COLUMN turn_count INTEGER NOT NULL DEFAULT 0
        CHECK (turn_count BETWEEN 0 AND 4294967295);\n\
      ALTER TABLE agent_runs ADD COLUMN prompt_tokens_used INTEGER NOT NULL DEFAULT 0
        CHECK (prompt_tokens_used >= 0);\n\
      ALTER TABLE agent_runs ADD COLUMN output_tokens_used INTEGER NOT NULL DEFAULT 0
        CHECK (output_tokens_used >= 0);\n\
      ALTER TABLE agent_runs ADD COLUMN action_count INTEGER NOT NULL DEFAULT 0
        CHECK (action_count BETWEEN 0 AND 4294967295 AND action_count <= turn_count);\n\
      ALTER TABLE agent_runs ADD COLUMN repair_count INTEGER NOT NULL DEFAULT 0
        CHECK (repair_count BETWEEN 0 AND 4294967295 AND repair_count <= turn_count);\n\
      ALTER TABLE run_events ADD COLUMN turn_prompt_tokens INTEGER
        CHECK (turn_prompt_tokens IS NULL OR turn_prompt_tokens BETWEEN 0 AND 4294967295);\n\
      ALTER TABLE run_events ADD COLUMN turn_output_tokens INTEGER
        CHECK (turn_output_tokens IS NULL OR turn_output_tokens BETWEEN 0 AND 4294967295);\n\
      ALTER TABLE run_events ADD COLUMN turn_action_kind TEXT
        CHECK (turn_action_kind IS NULL OR
          turn_action_kind IN ('search', 'inspect', 'update_ledger', 'finish'));\n\
      ALTER TABLE run_events ADD COLUMN turn_repair_used INTEGER
        CHECK (turn_repair_used IS NULL OR turn_repair_used IN (0, 1));\n\
      CREATE TRIGGER agent_runs_budget_immutable_guard
      BEFORE UPDATE OF turn_limit, prompt_token_limit, output_token_limit, action_limit,
        duration_limit_millis, repair_limit ON agent_runs
      WHEN NEW.turn_limit <> OLD.turn_limit
        OR NEW.prompt_token_limit <> OLD.prompt_token_limit
        OR NEW.output_token_limit <> OLD.output_token_limit
        OR NEW.action_limit <> OLD.action_limit
        OR NEW.duration_limit_millis <> OLD.duration_limit_millis
        OR NEW.repair_limit <> OLD.repair_limit
      BEGIN
        SELECT RAISE(ABORT, 'agent run budgets are immutable');
      END;\n\
      CREATE TRIGGER run_events_turn_charge_insert_guard
      BEFORE INSERT ON run_events
      WHEN (NEW.event_kind = 'model_interaction' AND
          (NEW.turn_prompt_tokens IS NULL OR NEW.turn_output_tokens IS NULL OR
           NEW.turn_repair_used IS NULL))
        OR (NEW.event_kind <> 'model_interaction' AND
          (NEW.turn_prompt_tokens IS NOT NULL OR NEW.turn_output_tokens IS NOT NULL OR
           NEW.turn_action_kind IS NOT NULL OR NEW.turn_repair_used IS NOT NULL))
      BEGIN
        SELECT RAISE(ABORT, 'run event turn charge is invalid');
      END;",
};

const KNOWLEDGE_AGENT_TOOL_EVIDENCE_MIGRATION: Migration = Migration {
    version: 16,
    name: "durable_agent_tool_evidence",
    sql: "CREATE TABLE tool_runs (\n\
      tool_run_id BLOB PRIMARY KEY NOT NULL CHECK (length(tool_run_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      event_sequence INTEGER NOT NULL CHECK (event_sequence >= 1),\n\
      status TEXT NOT NULL CHECK (status IN ('succeeded', 'failed', 'cancelled', 'denied')),\n\
      result_digest BLOB NOT NULL CHECK (length(result_digest) = 32),\n\
      result_truncated INTEGER NOT NULL CHECK (result_truncated IN (0, 1)),\n\
      snapshot_before_id BLOB NOT NULL CHECK (length(snapshot_before_id) = 32),\n\
      snapshot_after_id BLOB NOT NULL CHECK (length(snapshot_after_id) = 32),\n\
      observed_output_bytes INTEGER NOT NULL CHECK (observed_output_bytes >= 0),\n\
      UNIQUE (run_id, event_sequence),\n\
      FOREIGN KEY (run_id, event_sequence) REFERENCES run_events(run_id, event_sequence)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_before_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_after_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE tool_evidence (\n\
      tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 100),\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      location_kind TEXT NOT NULL CHECK (location_kind IN ('file', 'span')),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      start_byte INTEGER CHECK (start_byte IS NULL OR start_byte BETWEEN 0 AND 4294967295),\n\
      end_byte INTEGER CHECK (end_byte IS NULL OR end_byte BETWEEN 0 AND 4294967295),\n\
      start_row INTEGER CHECK (start_row IS NULL OR start_row BETWEEN 0 AND 4294967295),\n\
      start_column INTEGER CHECK (start_column IS NULL OR start_column BETWEEN 0 AND 4294967295),\n\
      end_row INTEGER CHECK (end_row IS NULL OR end_row BETWEEN 0 AND 4294967295),\n\
      end_column INTEGER CHECK (end_column IS NULL OR end_column BETWEEN 0 AND 4294967295),\n\
      CHECK ((location_kind = 'file' AND start_byte IS NULL AND end_byte IS NULL\n\
        AND start_row IS NULL AND start_column IS NULL AND end_row IS NULL\n\
        AND end_column IS NULL) OR (location_kind = 'span' AND start_byte IS NOT NULL\n\
        AND end_byte IS NOT NULL AND start_row IS NOT NULL AND start_column IS NOT NULL\n\
        AND end_row IS NOT NULL AND end_column IS NOT NULL)),\n\
      CHECK (start_byte IS NULL OR start_byte <= end_byte),\n\
      CHECK (start_row IS NULL OR start_row < end_row\n\
        OR (start_row = end_row AND start_column <= end_column)),\n\
      PRIMARY KEY (tool_run_id, item_sequence),\n\
      UNIQUE (tool_run_id, evidence_id),\n\
      FOREIGN KEY (tool_run_id) REFERENCES tool_runs(tool_run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TRIGGER tool_runs_event_guard\n\
      BEFORE INSERT ON tool_runs\n\
      WHEN NOT EXISTS (SELECT 1 FROM run_events\n\
        WHERE run_id = NEW.run_id AND event_sequence = NEW.event_sequence\n\
          AND event_kind = 'tool_action' AND subject_kind = 'tool'\n\
          AND subject_id = NEW.tool_run_id AND snapshot_id = NEW.snapshot_before_id)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'tool run journal anchor is invalid');\n\
      END;\n\
      CREATE INDEX tool_runs_run_idx ON tool_runs(run_id, event_sequence);\n\
      CREATE INDEX tool_evidence_id_idx ON tool_evidence(evidence_id, tool_run_id);",
};

const KNOWLEDGE_AGENT_RECOVERY_MIGRATION: Migration = Migration {
    version: 17,
    name: "durable_agent_recovery",
    sql: "CREATE TABLE tool_run_attempts (\n\
      tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),\n\
      attempt_sequence INTEGER NOT NULL CHECK (attempt_sequence BETWEEN 1 AND 4294967295),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      status TEXT NOT NULL CHECK (status IN ('in_flight', 'succeeded', 'failed', 'cancelled',\n\
        'denied', 'interrupted')),\n\
      started_at_unix_millis INTEGER NOT NULL CHECK (started_at_unix_millis >= 0),\n\
      updated_at_unix_millis INTEGER NOT NULL\n\
        CHECK (updated_at_unix_millis >= started_at_unix_millis),\n\
      PRIMARY KEY (tool_run_id, attempt_sequence),\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO tool_run_attempts (\n\
        tool_run_id, attempt_sequence, run_id, snapshot_id, status,\n\
        started_at_unix_millis, updated_at_unix_millis\n\
      ) SELECT tool_runs.tool_run_id, 1, tool_runs.run_id, tool_runs.snapshot_before_id,\n\
        tool_runs.status, run_events.occurred_at_unix_millis, run_events.occurred_at_unix_millis\n\
        FROM tool_runs JOIN run_events ON run_events.run_id = tool_runs.run_id\n\
          AND run_events.event_sequence = tool_runs.event_sequence;\n\
      CREATE UNIQUE INDEX tool_run_attempts_one_in_flight_idx\n\
        ON tool_run_attempts(tool_run_id) WHERE status = 'in_flight';\n\
      CREATE INDEX tool_run_attempts_run_status_idx\n\
        ON tool_run_attempts(run_id, status, tool_run_id, attempt_sequence);",
};

const KNOWLEDGE_POLICY_APPROVAL_MIGRATION: Migration = Migration {
    version: 18,
    name: "central_policy_and_approvals",
    sql: "CREATE TABLE approval_requests (\n\
      approval_request_id BLOB PRIMARY KEY NOT NULL CHECK (length(approval_request_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      requested_by TEXT NOT NULL CHECK (requested_by = 'controller'),\n\
      action_fingerprint BLOB NOT NULL CHECK (length(action_fingerprint) = 32),\n\
      scope_digest BLOB NOT NULL CHECK (length(scope_digest) = 32),\n\
      action_class TEXT NOT NULL CHECK (action_class IN ('read', 'derive', 'write',\n\
        'execute_safe', 'execute_open', 'network', 'destructive', 'publish', 'outside_root')),\n\
      risk_level TEXT NOT NULL CHECK (risk_level IN ('low', 'moderate', 'high', 'critical')),\n\
      requested_at_unix_millis INTEGER NOT NULL CHECK (requested_at_unix_millis >= 0),\n\
      expires_at_unix_millis INTEGER NOT NULL CHECK (expires_at_unix_millis > requested_at_unix_millis),\n\
      CHECK (expires_at_unix_millis - requested_at_unix_millis <= 86400000),\n\
      CHECK ((action_class IN ('read', 'derive') AND risk_level = 'low') OR\n\
        (action_class IN ('write', 'execute_safe') AND risk_level = 'moderate') OR\n\
        (action_class = 'execute_open' AND risk_level IN ('high', 'critical')) OR\n\
        (action_class = 'network' AND risk_level = 'high') OR\n\
        (action_class IN ('destructive', 'publish', 'outside_root')\n\
          AND risk_level = 'critical')),\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE approval_grants (\n\
      approval_id BLOB PRIMARY KEY NOT NULL CHECK (length(approval_id) = 32),\n\
      approval_request_id BLOB NOT NULL UNIQUE CHECK (length(approval_request_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      granted_by TEXT NOT NULL CHECK (granted_by = 'user'),\n\
      granted_event_id BLOB NOT NULL UNIQUE CHECK (length(granted_event_id) = 32),\n\
      granted_at_unix_millis INTEGER NOT NULL CHECK (granted_at_unix_millis >= 0),\n\
      expires_at_unix_millis INTEGER NOT NULL CHECK (expires_at_unix_millis > granted_at_unix_millis),\n\
      status TEXT NOT NULL CHECK (status IN ('active', 'consumed', 'revoked')),\n\
      consumed_decision_id BLOB\n\
        CHECK (consumed_decision_id IS NULL OR length(consumed_decision_id) = 32),\n\
      consumed_at_unix_millis INTEGER CHECK (consumed_at_unix_millis IS NULL OR\n\
        (consumed_at_unix_millis >= granted_at_unix_millis AND\n\
          consumed_at_unix_millis < expires_at_unix_millis)),\n\
      revoked_at_unix_millis INTEGER CHECK (revoked_at_unix_millis IS NULL OR\n\
        (revoked_at_unix_millis >= granted_at_unix_millis AND\n\
          revoked_at_unix_millis < expires_at_unix_millis)),\n\
      revoked_by TEXT CHECK (revoked_by IS NULL OR revoked_by = 'user'),\n\
      revoked_event_id BLOB UNIQUE\n\
        CHECK (revoked_event_id IS NULL OR length(revoked_event_id) = 32),\n\
      CHECK ((status = 'active' AND consumed_decision_id IS NULL\n\
        AND consumed_at_unix_millis IS NULL AND revoked_at_unix_millis IS NULL\n\
        AND revoked_by IS NULL AND revoked_event_id IS NULL) OR\n\
        (status = 'consumed' AND consumed_decision_id IS NOT NULL\n\
        AND consumed_at_unix_millis IS NOT NULL AND revoked_at_unix_millis IS NULL\n\
        AND revoked_by IS NULL AND revoked_event_id IS NULL) OR\n\
        (status = 'revoked' AND consumed_decision_id IS NULL\n\
        AND consumed_at_unix_millis IS NULL AND revoked_at_unix_millis IS NOT NULL\n\
        AND revoked_by = 'user' AND revoked_event_id IS NOT NULL)),\n\
      FOREIGN KEY (approval_request_id) REFERENCES approval_requests(approval_request_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (granted_event_id) REFERENCES run_events(event_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (revoked_event_id) REFERENCES run_events(event_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE policy_decisions (\n\
      policy_decision_id BLOB PRIMARY KEY NOT NULL CHECK (length(policy_decision_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 32),\n\
      actor TEXT NOT NULL CHECK (actor = 'controller'),\n\
      action_fingerprint BLOB NOT NULL CHECK (length(action_fingerprint) = 32),\n\
      scope_digest BLOB NOT NULL CHECK (length(scope_digest) = 32),\n\
      action_class TEXT NOT NULL CHECK (action_class IN ('read', 'derive', 'write',\n\
        'execute_safe', 'execute_open', 'network', 'destructive', 'publish', 'outside_root')),\n\
      risk_level TEXT NOT NULL CHECK (risk_level IN ('low', 'moderate', 'high', 'critical')),\n\
      outcome TEXT NOT NULL CHECK (outcome IN ('allowed', 'approval_required', 'denied')),\n\
      reason TEXT NOT NULL CHECK (reason IN ('system_automatic', 'system_approval_required',\n\
        'workspace_approval_required', 'workspace_denied', 'approval_granted',\n\
        'approval_run_mismatch', 'approval_scope_mismatch', 'approval_action_mismatch',\n\
        'approval_expired', 'approval_revoked', 'approval_already_consumed',\n\
        'approval_timestamp_regressed')),\n\
      approval_request_id BLOB\n\
        CHECK (approval_request_id IS NULL OR length(approval_request_id) = 32),\n\
      approval_id BLOB CHECK (approval_id IS NULL OR length(approval_id) = 32),\n\
      started_at_unix_millis INTEGER NOT NULL CHECK (started_at_unix_millis >= 0),\n\
      decided_at_unix_millis INTEGER NOT NULL CHECK (decided_at_unix_millis >= started_at_unix_millis),\n\
      duration_millis INTEGER NOT NULL CHECK (duration_millis >= 0),\n\
      CHECK (duration_millis = decided_at_unix_millis - started_at_unix_millis),\n\
      CHECK ((action_class IN ('read', 'derive') AND risk_level = 'low') OR\n\
        (action_class IN ('write', 'execute_safe') AND risk_level = 'moderate') OR\n\
        (action_class = 'execute_open' AND risk_level IN ('high', 'critical')) OR\n\
        (action_class = 'network' AND risk_level = 'high') OR\n\
        (action_class IN ('destructive', 'publish', 'outside_root')\n\
          AND risk_level = 'critical')),\n\
      CHECK ((outcome = 'allowed' AND reason = 'system_automatic'\n\
        AND approval_request_id IS NULL AND approval_id IS NULL) OR\n\
        (outcome = 'allowed' AND reason = 'approval_granted'\n\
        AND approval_request_id IS NULL AND approval_id IS NOT NULL) OR\n\
        (outcome = 'approval_required' AND reason IN ('system_approval_required',\n\
          'workspace_approval_required', 'approval_run_mismatch', 'approval_scope_mismatch',\n\
          'approval_action_mismatch', 'approval_expired', 'approval_revoked',\n\
          'approval_already_consumed', 'approval_timestamp_regressed')\n\
        AND approval_request_id IS NOT NULL AND approval_id IS NULL) OR\n\
        (outcome = 'denied' AND reason = 'workspace_denied'\n\
        AND approval_request_id IS NULL AND approval_id IS NULL)),\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (event_id) REFERENCES run_events(event_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (approval_request_id) REFERENCES approval_requests(approval_request_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (approval_id) REFERENCES approval_grants(approval_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TRIGGER approval_requests_immutable_guard\n\
      BEFORE UPDATE ON approval_requests BEGIN\n\
        SELECT RAISE(ABORT, 'approval requests are immutable');\n\
      END;\n\
      CREATE TRIGGER approval_requests_delete_guard\n\
      BEFORE DELETE ON approval_requests BEGIN\n\
        SELECT RAISE(ABORT, 'approval requests are append-only');\n\
      END;\n\
      CREATE TRIGGER policy_decisions_immutable_guard\n\
      BEFORE UPDATE ON policy_decisions BEGIN\n\
        SELECT RAISE(ABORT, 'policy decisions are immutable');\n\
      END;\n\
      CREATE TRIGGER policy_decisions_delete_guard\n\
      BEFORE DELETE ON policy_decisions BEGIN\n\
        SELECT RAISE(ABORT, 'policy decisions are append-only');\n\
      END;\n\
      CREATE TRIGGER approval_grants_insert_guard\n\
      BEFORE INSERT ON approval_grants\n\
      WHEN NEW.status <> 'active' OR NEW.consumed_decision_id IS NOT NULL\n\
        OR NEW.consumed_at_unix_millis IS NOT NULL OR NEW.revoked_at_unix_millis IS NOT NULL\n\
        OR NEW.revoked_by IS NOT NULL OR NEW.revoked_event_id IS NOT NULL\n\
        OR NOT EXISTS (SELECT 1 FROM approval_requests\n\
          WHERE approval_request_id = NEW.approval_request_id AND run_id = NEW.run_id\n\
            AND expires_at_unix_millis = NEW.expires_at_unix_millis\n\
            AND requested_at_unix_millis <= NEW.granted_at_unix_millis\n\
            AND NEW.granted_at_unix_millis < expires_at_unix_millis)\n\
        OR NOT EXISTS (SELECT 1 FROM run_events WHERE event_id = NEW.granted_event_id\n\
          AND run_id = NEW.run_id AND event_kind = 'approval_recorded'\n\
          AND payload_code = 'user_request' AND payload_outcome = 'succeeded'\n\
          AND occurred_at_unix_millis = NEW.granted_at_unix_millis)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'approval grant does not match its request');\n\
      END;\n\
      CREATE TRIGGER approval_grants_lifecycle_guard\n\
      BEFORE UPDATE ON approval_grants\n\
      WHEN OLD.status <> 'active' OR NEW.approval_id <> OLD.approval_id\n\
        OR NEW.approval_request_id <> OLD.approval_request_id OR NEW.run_id <> OLD.run_id\n\
        OR NEW.granted_by <> OLD.granted_by OR NEW.granted_event_id <> OLD.granted_event_id\n\
        OR NEW.granted_at_unix_millis <> OLD.granted_at_unix_millis\n\
        OR NEW.expires_at_unix_millis <> OLD.expires_at_unix_millis\n\
        OR NEW.status NOT IN ('consumed', 'revoked')\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'approval lifecycle transition is invalid');\n\
      END;\n\
      CREATE TRIGGER approval_grants_revocation_guard\n\
      BEFORE UPDATE ON approval_grants WHEN NEW.status = 'revoked'\n\
        AND NOT EXISTS (SELECT 1 FROM run_events WHERE event_id = NEW.revoked_event_id\n\
          AND run_id = NEW.run_id AND event_kind = 'approval_recorded'\n\
          AND payload_code = 'user_request' AND payload_outcome = 'cancelled'\n\
          AND occurred_at_unix_millis = NEW.revoked_at_unix_millis)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'approval revocation event is invalid');\n\
      END;\n\
      CREATE TRIGGER approval_grants_delete_guard\n\
      BEFORE DELETE ON approval_grants BEGIN\n\
        SELECT RAISE(ABORT, 'approval grants are durable');\n\
      END;\n\
      CREATE TRIGGER policy_decisions_event_guard\n\
      BEFORE INSERT ON policy_decisions\n\
      WHEN NOT EXISTS (SELECT 1 FROM run_events WHERE event_id = NEW.event_id\n\
        AND run_id = NEW.run_id AND event_kind = 'approval_recorded'\n\
        AND payload_code = 'policy_decision'\n\
        AND occurred_at_unix_millis = NEW.decided_at_unix_millis\n\
        AND ((NEW.outcome = 'allowed' AND payload_outcome = 'succeeded')\n\
          OR (NEW.outcome IN ('approval_required', 'denied') AND payload_outcome = 'denied')))\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'policy decision run event is invalid');\n\
      END;\n\
      CREATE TRIGGER policy_decisions_request_guard\n\
      BEFORE INSERT ON policy_decisions WHEN NEW.outcome = 'approval_required'\n\
        AND NOT EXISTS (SELECT 1 FROM approval_requests\n\
          WHERE approval_request_id = NEW.approval_request_id AND run_id = NEW.run_id\n\
            AND action_fingerprint = NEW.action_fingerprint\n\
            AND scope_digest = NEW.scope_digest AND action_class = NEW.action_class\n\
            AND risk_level = NEW.risk_level\n\
            AND requested_at_unix_millis = NEW.decided_at_unix_millis)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'policy decision approval request is invalid');\n\
      END;\n\
      CREATE TRIGGER policy_decisions_approval_guard\n\
      BEFORE INSERT ON policy_decisions WHEN NEW.reason = 'approval_granted'\n\
        AND NOT EXISTS (SELECT 1 FROM approval_grants JOIN approval_requests USING (approval_request_id)\n\
          WHERE approval_grants.approval_id = NEW.approval_id\n\
            AND approval_grants.run_id = NEW.run_id AND approval_grants.status = 'active'\n\
            AND approval_requests.action_fingerprint = NEW.action_fingerprint\n\
            AND approval_requests.scope_digest = NEW.scope_digest\n\
            AND approval_requests.action_class = NEW.action_class\n\
            AND approval_requests.risk_level = NEW.risk_level\n\
            AND approval_grants.granted_at_unix_millis <= NEW.decided_at_unix_millis\n\
            AND NEW.decided_at_unix_millis < approval_grants.expires_at_unix_millis)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'policy decision approval is invalid');\n\
      END;\n\
      CREATE TRIGGER approval_grants_consumption_guard\n\
      BEFORE UPDATE ON approval_grants WHEN NEW.status = 'consumed'\n\
        AND NOT EXISTS (SELECT 1 FROM policy_decisions JOIN approval_requests\n\
          ON approval_requests.approval_request_id = NEW.approval_request_id\n\
          WHERE policy_decisions.policy_decision_id = NEW.consumed_decision_id\n\
            AND policy_decisions.approval_id = NEW.approval_id\n\
            AND policy_decisions.run_id = NEW.run_id\n\
            AND policy_decisions.outcome = 'allowed'\n\
            AND policy_decisions.reason = 'approval_granted'\n\
            AND policy_decisions.decided_at_unix_millis = NEW.consumed_at_unix_millis\n\
            AND policy_decisions.action_fingerprint = approval_requests.action_fingerprint\n\
            AND policy_decisions.scope_digest = approval_requests.scope_digest\n\
            AND policy_decisions.action_class = approval_requests.action_class\n\
            AND policy_decisions.risk_level = approval_requests.risk_level)\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'approval consumption decision is invalid');\n\
      END;\n\
      CREATE INDEX approval_requests_run_idx\n\
        ON approval_requests(run_id, requested_at_unix_millis, approval_request_id);\n\
      CREATE INDEX approval_grants_run_status_idx\n\
        ON approval_grants(run_id, status, approval_id);\n\
      CREATE INDEX policy_decisions_run_idx\n\
        ON policy_decisions(run_id, decided_at_unix_millis, policy_decision_id);",
};

const KNOWLEDGE_COMMAND_ALLOWLIST_MIGRATION: Migration = Migration {
    version: 19,
    name: "project_command_allowlists",
    sql: "CREATE TABLE command_allowlist_revisions (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      revision INTEGER NOT NULL CHECK (revision > 0),\n\
      catalog_id BLOB NOT NULL CHECK (length(catalog_id) = 32),\n\
      confirmed_at_unix_millis INTEGER NOT NULL CHECK (confirmed_at_unix_millis >= 0),\n\
      PRIMARY KEY (worktree_id, revision),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE command_allowlist_entries (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      revision INTEGER NOT NULL CHECK (revision > 0),\n\
      ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 255),\n\
      command_id BLOB NOT NULL CHECK (length(command_id) = 32),\n\
      PRIMARY KEY (worktree_id, revision, ordinal),\n\
      UNIQUE (worktree_id, revision, command_id),\n\
      FOREIGN KEY (worktree_id, revision)\n\
        REFERENCES command_allowlist_revisions(worktree_id, revision)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE INDEX command_allowlist_latest_idx\n\
        ON command_allowlist_revisions(worktree_id, revision DESC);\n\
      CREATE TRIGGER command_allowlist_revisions_update_guard\n\
      BEFORE UPDATE ON command_allowlist_revisions\n\
      WHEN NEW.worktree_id = OLD.worktree_id OR NEW.revision <> OLD.revision\n\
        OR NEW.catalog_id <> OLD.catalog_id\n\
        OR NEW.confirmed_at_unix_millis <> OLD.confirmed_at_unix_millis\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'command allowlist revisions are immutable');\n\
      END;\n\
      CREATE TRIGGER command_allowlist_revisions_delete_guard\n\
      BEFORE DELETE ON command_allowlist_revisions BEGIN\n\
        SELECT RAISE(ABORT, 'command allowlist revisions are append-only');\n\
      END;\n\
      CREATE TRIGGER command_allowlist_entries_update_guard\n\
      BEFORE UPDATE ON command_allowlist_entries\n\
      WHEN NEW.worktree_id = OLD.worktree_id OR NEW.revision <> OLD.revision\n\
        OR NEW.ordinal <> OLD.ordinal OR NEW.command_id <> OLD.command_id\n\
      BEGIN\n\
        SELECT RAISE(ABORT, 'command allowlist entries are immutable');\n\
      END;\n\
      CREATE TRIGGER command_allowlist_entries_delete_guard\n\
      BEFORE DELETE ON command_allowlist_entries BEGIN\n\
        SELECT RAISE(ABORT, 'command allowlist entries are append-only');\n\
      END;",
};

const KNOWLEDGE_VERIFICATION_ENGINE_MIGRATION: Migration = Migration {
    version: 20,
    name: "typed_verification_engine",
    sql: "ALTER TABLE acceptance_criteria ADD COLUMN requirement TEXT NOT NULL DEFAULT 'must'
        CHECK (requirement IN ('must', 'should'));\n\
      CREATE UNIQUE INDEX task_ledgers_goal_revision_idx
        ON task_ledgers(task_id, goal_revision);\n\
      CREATE TABLE task_step_acceptance_criteria (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      step_id BLOB NOT NULL CHECK (length(step_id) = 32),\n\
      goal_revision INTEGER NOT NULL CHECK (goal_revision BETWEEN 1 AND 4294967295),\n\
      criterion_id BLOB NOT NULL CHECK (length(criterion_id) = 32),\n\
      PRIMARY KEY (task_id, step_id, criterion_id),\n\
      FOREIGN KEY (task_id, step_id) REFERENCES task_steps(task_id, step_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, goal_revision) REFERENCES task_ledgers(task_id, goal_revision)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE,\n\
      FOREIGN KEY (task_id, goal_revision, criterion_id)\n\
        REFERENCES acceptance_criteria(task_id, goal_revision, criterion_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_specs_v1 (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),\n\
      target_kind TEXT NOT NULL CHECK (target_kind IN\n\
        ('command', 'test', 'diff_invariant', 'diagnostic', 'user_confirm')),\n\
      command_id BLOB CHECK (command_id IS NULL OR length(command_id) = 32),\n\
      verification_scope TEXT CHECK (verification_scope IS NULL OR\n\
        verification_scope IN ('targeted', 'package', 'workspace')),\n\
      test_selector_kind TEXT CHECK (test_selector_kind IS NULL OR\n\
        test_selector_kind IN ('all', 'exact')),\n\
      test_selector TEXT CHECK (test_selector IS NULL OR\n\
        length(CAST(test_selector AS BLOB)) BETWEEN 1 AND 1024),\n\
      minimum_test_cases INTEGER CHECK (minimum_test_cases IS NULL OR\n\
        minimum_test_cases BETWEEN 1 AND 1000000),\n\
      diff_mode TEXT CHECK (diff_mode IS NULL OR\n\
        diff_mode IN ('no_changes', 'only_paths', 'exact_paths')),\n\
      diagnostic_policy TEXT CHECK (diagnostic_policy IS NULL OR\n\
        diagnostic_policy IN ('no_errors', 'no_warnings')),\n\
      confirmation_scope_id BLOB CHECK (confirmation_scope_id IS NULL OR\n\
        length(confirmation_scope_id) = 32),\n\
      CHECK ((target_kind = 'command' AND command_id IS NOT NULL\n\
          AND verification_scope IS NOT NULL AND test_selector_kind IS NULL\n\
          AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL\n\
          AND diagnostic_policy IS NULL AND confirmation_scope_id IS NULL) OR\n\
        (target_kind = 'test' AND command_id IS NOT NULL AND verification_scope IS NOT NULL\n\
          AND test_selector_kind IS NOT NULL\n\
          AND ((test_selector_kind = 'all' AND test_selector IS NULL) OR\n\
            (test_selector_kind = 'exact' AND test_selector IS NOT NULL))\n\
          AND minimum_test_cases IS NOT NULL AND diff_mode IS NULL\n\
          AND diagnostic_policy IS NULL AND confirmation_scope_id IS NULL) OR\n\
        (target_kind = 'diff_invariant' AND command_id IS NULL\n\
          AND verification_scope IS NULL AND test_selector_kind IS NULL\n\
          AND test_selector IS NULL AND minimum_test_cases IS NULL\n\
          AND diff_mode IS NOT NULL AND diagnostic_policy IS NULL\n\
          AND confirmation_scope_id IS NULL) OR\n\
        (target_kind = 'diagnostic' AND command_id IS NOT NULL\n\
          AND verification_scope IS NOT NULL AND test_selector_kind IS NULL\n\
          AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL\n\
          AND diagnostic_policy IS NOT NULL AND confirmation_scope_id IS NULL) OR\n\
        (target_kind = 'user_confirm' AND command_id IS NULL\n\
          AND verification_scope IS NULL AND test_selector_kind IS NULL\n\
          AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL\n\
          AND diagnostic_policy IS NULL AND confirmation_scope_id IS NOT NULL)),\n\
      PRIMARY KEY (task_id, verification_spec_id),\n\
      FOREIGN KEY (task_id, verification_spec_id)\n\
        REFERENCES task_steps(task_id, verification_spec_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE verification_spec_paths (\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      PRIMARY KEY (task_id, verification_spec_id, item_sequence),\n\
      UNIQUE (task_id, verification_spec_id, repository_path),\n\
      FOREIGN KEY (task_id, verification_spec_id)\n\
        REFERENCES verification_specs_v1(task_id, verification_spec_id)\n\
        ON UPDATE RESTRICT ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE verification_evidence (\n\
      evidence_id BLOB PRIMARY KEY NOT NULL CHECK (length(evidence_id) = 32),\n\
      task_id BLOB NOT NULL CHECK (length(task_id) = 32),\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      evidence_kind TEXT NOT NULL CHECK (evidence_kind IN\n\
        ('command', 'test', 'diff_invariant', 'diagnostic', 'user_confirm')),\n\
      schema_version INTEGER NOT NULL CHECK (schema_version = 1),\n\
      verification_run_id BLOB NOT NULL CHECK (length(verification_run_id) = 32),\n\
      verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      FOREIGN KEY (task_id) REFERENCES tasks(task_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT,\n\
      FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_process_evidence (\n\
      evidence_id BLOB PRIMARY KEY NOT NULL CHECK (length(evidence_id) = 32),\n\
      command_evidence_id BLOB NOT NULL CHECK (length(command_evidence_id) = 32),\n\
      tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),\n\
      command_id BLOB NOT NULL CHECK (length(command_id) = 32),\n\
      process_specification_id BLOB NOT NULL CHECK (length(process_specification_id) = 32),\n\
      policy_decision_id BLOB NOT NULL CHECK (length(policy_decision_id) = 32),\n\
      termination_kind TEXT NOT NULL CHECK (termination_kind IN ('exited', 'timed_out', 'cancelled')),\n\
      exit_code INTEGER,\n\
      duration_millis INTEGER NOT NULL CHECK (duration_millis >= 0),\n\
      stdout_digest BLOB NOT NULL CHECK (length(stdout_digest) = 32),\n\
      stdout_observed_bytes INTEGER NOT NULL CHECK (stdout_observed_bytes >= 0),\n\
      stdout_retained_limit INTEGER NOT NULL CHECK (stdout_retained_limit BETWEEN 1 AND 4294967295),\n\
      stdout_truncated INTEGER NOT NULL CHECK (stdout_truncated IN (0, 1)),\n\
      stdout_redaction TEXT CHECK (stdout_redaction IS NULL OR stdout_redaction IN\n\
        ('invalid_utf8', 'secret_candidate', 'unsafe_control')),\n\
      stderr_digest BLOB NOT NULL CHECK (length(stderr_digest) = 32),\n\
      stderr_observed_bytes INTEGER NOT NULL CHECK (stderr_observed_bytes >= 0),\n\
      stderr_retained_limit INTEGER NOT NULL CHECK (stderr_retained_limit BETWEEN 1 AND 4294967295),\n\
      stderr_truncated INTEGER NOT NULL CHECK (stderr_truncated IN (0, 1)),\n\
      stderr_redaction TEXT CHECK (stderr_redaction IS NULL OR stderr_redaction IN\n\
        ('invalid_utf8', 'secret_candidate', 'unsafe_control')),\n\
      CHECK (termination_kind = 'exited' OR exit_code IS NULL),\n\
      CHECK ((stdout_truncated = 1 AND stdout_observed_bytes > stdout_retained_limit) OR\n\
        (stdout_truncated = 0 AND (stdout_redaction IS NOT NULL OR\n\
          stdout_observed_bytes <= stdout_retained_limit))),\n\
      CHECK ((stderr_truncated = 1 AND stderr_observed_bytes > stderr_retained_limit) OR\n\
        (stderr_truncated = 0 AND (stderr_redaction IS NOT NULL OR\n\
          stderr_observed_bytes <= stderr_retained_limit))),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (tool_run_id) REFERENCES tool_runs(tool_run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (policy_decision_id) REFERENCES policy_decisions(policy_decision_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_test_cases (\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 1000000),\n\
      case_name TEXT NOT NULL CHECK (length(CAST(case_name AS BLOB)) BETWEEN 1 AND 1024),\n\
      outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed', 'ignored')),\n\
      PRIMARY KEY (evidence_id, item_sequence),\n\
      UNIQUE (evidence_id, case_name),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_process_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_diagnostic_reports (\n\
      evidence_id BLOB PRIMARY KEY NOT NULL CHECK (length(evidence_id) = 32),\n\
      error_count INTEGER NOT NULL CHECK (error_count BETWEEN 0 AND 4294967295),\n\
      warning_count INTEGER NOT NULL CHECK (warning_count BETWEEN 0 AND 4294967295),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_process_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_diff_evidence (\n\
      evidence_id BLOB PRIMARY KEY NOT NULL CHECK (length(evidence_id) = 32),\n\
      source_kind TEXT NOT NULL CHECK (source_kind IN ('patch', 'published_indexes')),\n\
      action_digest BLOB CHECK (action_digest IS NULL OR length(action_digest) = 32),\n\
      policy_decision_id BLOB CHECK (policy_decision_id IS NULL OR length(policy_decision_id) = 32),\n\
      base_index_run_id BLOB CHECK (base_index_run_id IS NULL OR length(base_index_run_id) = 32),\n\
      current_index_run_id BLOB CHECK (current_index_run_id IS NULL OR length(current_index_run_id) = 32),\n\
      base_snapshot_id BLOB NOT NULL CHECK (length(base_snapshot_id) = 32),\n\
      complete INTEGER NOT NULL CHECK (complete IN (0, 1)),\n\
      CHECK ((source_kind = 'patch' AND action_digest IS NOT NULL\n\
          AND policy_decision_id IS NOT NULL AND base_index_run_id IS NULL\n\
          AND current_index_run_id IS NULL) OR\n\
        (source_kind = 'published_indexes' AND action_digest IS NULL\n\
          AND policy_decision_id IS NULL AND base_index_run_id IS NOT NULL\n\
          AND current_index_run_id IS NOT NULL AND base_index_run_id <> current_index_run_id)),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (policy_decision_id) REFERENCES policy_decisions(policy_decision_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (base_index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (current_index_run_id) REFERENCES index_runs(index_run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (base_snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_diff_paths (\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 128),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      PRIMARY KEY (evidence_id, item_sequence),\n\
      UNIQUE (evidence_id, repository_path),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_diff_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_user_confirmations (\n\
      evidence_id BLOB PRIMARY KEY NOT NULL CHECK (length(evidence_id) = 32),\n\
      scope_id BLOB NOT NULL CHECK (length(scope_id) = 32),\n\
      confirmed_at_unix_millis INTEGER NOT NULL CHECK (confirmed_at_unix_millis >= 0),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE verification_evidence_dependencies (\n\
      evidence_id BLOB NOT NULL CHECK (length(evidence_id) = 32),\n\
      item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 512),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      dependency_state TEXT NOT NULL CHECK (dependency_state IN ('present', 'absent')),\n\
      content_hash BLOB CHECK (content_hash IS NULL OR length(content_hash) = 32),\n\
      CHECK ((dependency_state = 'present') = (content_hash IS NOT NULL)),\n\
      PRIMARY KEY (evidence_id, item_sequence),\n\
      UNIQUE (evidence_id, repository_path),\n\
      FOREIGN KEY (evidence_id) REFERENCES verification_evidence(evidence_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TRIGGER verification_evidence_update_guard\n\
      BEFORE UPDATE ON verification_evidence\n\
      WHEN NEW.evidence_id <> OLD.evidence_id OR NEW.task_id <> OLD.task_id\n\
        OR NEW.evidence_kind <> OLD.evidence_kind OR NEW.schema_version <> OLD.schema_version\n\
        OR NEW.verification_run_id <> OLD.verification_run_id\n\
        OR NEW.verification_spec_id <> OLD.verification_spec_id OR NEW.run_id <> OLD.run_id\n\
        OR NEW.snapshot_id <> OLD.snapshot_id\n\
      BEGIN SELECT RAISE(ABORT, 'verification evidence is immutable'); END;\n\
      CREATE TRIGGER verification_evidence_delete_guard\n\
      BEFORE DELETE ON verification_evidence\n\
      BEGIN SELECT RAISE(ABORT, 'verification evidence is append-only'); END;\n\
      CREATE INDEX verification_evidence_task_idx\n\
        ON verification_evidence(task_id, verification_spec_id, evidence_id);\n\
      CREATE INDEX verification_evidence_snapshot_idx\n\
        ON verification_evidence(worktree_id, snapshot_id, evidence_id);",
};

const KNOWLEDGE_AGENT_ACTION_V2_MIGRATION: Migration = Migration {
    version: 21,
    name: "agent_action_v2",
    sql: "ALTER TABLE run_events ADD COLUMN turn_action_kind_v2 TEXT
        CHECK (turn_action_kind_v2 IS NULL OR turn_action_kind_v2 IN
          ('search', 'inspect', 'update_ledger', 'finish', 'apply_patch', 'run'));\n\
      CREATE TRIGGER run_events_turn_action_v2_insert_guard
      BEFORE INSERT ON run_events
      WHEN (NEW.turn_action_kind IS NOT NULL AND NEW.turn_action_kind_v2 IS NOT NULL)
        OR (NEW.event_kind <> 'model_interaction' AND NEW.turn_action_kind_v2 IS NOT NULL)
      BEGIN
        SELECT RAISE(ABORT, 'run event v2 turn action is invalid');
      END;",
};

const KNOWLEDGE_MUTATION_RECOVERY_MIGRATION: Migration = Migration {
    version: 22,
    name: "durable_mutation_recovery",
    sql: "CREATE TABLE mutation_attempts (\n\
      tool_run_id BLOB NOT NULL CHECK (length(tool_run_id) = 32),\n\
      attempt_sequence INTEGER NOT NULL CHECK (attempt_sequence BETWEEN 1 AND 4294967295),\n\
      action_fingerprint BLOB NOT NULL CHECK (length(action_fingerprint) = 32),\n\
      action_kind TEXT NOT NULL CHECK (action_kind IN ('patch', 'process',\n\
        'unclassified_legacy')),\n\
      application_state TEXT NOT NULL CHECK (application_state IN\n\
        ('applied', 'not_applied', 'unknown')),\n\
      reconciliation_state TEXT NOT NULL CHECK (reconciliation_state IN\n\
        ('not_required', 'required', 'reconciled', 'replanned')),\n\
      reconciled_snapshot_id BLOB CHECK\n\
        (reconciled_snapshot_id IS NULL OR length(reconciled_snapshot_id) = 32),\n\
      reconciled_at_unix_millis INTEGER CHECK\n\
        (reconciled_at_unix_millis IS NULL OR reconciled_at_unix_millis >= 0),\n\
      CHECK ((application_state IN ('applied', 'not_applied')\n\
          AND reconciliation_state = 'not_required' AND reconciled_snapshot_id IS NULL\n\
          AND reconciled_at_unix_millis IS NULL) OR\n\
        (application_state = 'unknown' AND reconciliation_state = 'required'\n\
          AND reconciled_snapshot_id IS NULL AND reconciled_at_unix_millis IS NULL) OR\n\
        (application_state = 'unknown' AND reconciliation_state IN ('reconciled', 'replanned')\n\
          AND reconciled_snapshot_id IS NOT NULL AND reconciled_at_unix_millis IS NOT NULL)),\n\
      PRIMARY KEY (tool_run_id, attempt_sequence),\n\
      FOREIGN KEY (tool_run_id, attempt_sequence)\n\
        REFERENCES tool_run_attempts(tool_run_id, attempt_sequence)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT,\n\
      FOREIGN KEY (reconciled_snapshot_id) REFERENCES snapshots(snapshot_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO mutation_attempts (\n\
        tool_run_id, attempt_sequence, action_fingerprint, action_kind, application_state,\n\
        reconciliation_state, reconciled_snapshot_id, reconciled_at_unix_millis\n\
      ) SELECT tool_run_id, attempt_sequence, tool_run_id, 'unclassified_legacy', 'unknown',\n\
        'required', NULL, NULL FROM tool_run_attempts WHERE status = 'in_flight';\n\
      CREATE INDEX mutation_attempts_reconciliation_idx\n\
        ON mutation_attempts(application_state, reconciliation_state, tool_run_id,\n\
          attempt_sequence);\n\
      CREATE TRIGGER mutation_attempts_insert_guard\n\
      BEFORE INSERT ON mutation_attempts\n\
      WHEN NOT EXISTS (SELECT 1 FROM tool_run_attempts\n\
        WHERE tool_run_id = NEW.tool_run_id AND attempt_sequence = NEW.attempt_sequence\n\
          AND status = 'in_flight')\n\
      BEGIN SELECT RAISE(ABORT, 'mutation attempt requires an in-flight tool attempt'); END;\n\
      CREATE TRIGGER mutation_attempts_update_guard\n\
      BEFORE UPDATE ON mutation_attempts\n\
      WHEN NEW.tool_run_id <> OLD.tool_run_id\n\
        OR NEW.attempt_sequence <> OLD.attempt_sequence\n\
        OR NEW.action_fingerprint <> OLD.action_fingerprint\n\
        OR NEW.action_kind <> OLD.action_kind\n\
        OR NOT ((OLD.application_state = 'unknown' AND OLD.reconciliation_state = 'required')\n\
          OR (OLD.application_state = 'unknown' AND OLD.reconciliation_state = 'reconciled'\n\
            AND NEW.application_state = 'unknown' AND NEW.reconciliation_state = 'replanned'\n\
            AND NEW.reconciled_snapshot_id = OLD.reconciled_snapshot_id\n\
            AND NEW.reconciled_at_unix_millis = OLD.reconciled_at_unix_millis))\n\
      BEGIN SELECT RAISE(ABORT, 'mutation attempt transition is invalid'); END;\n\
      CREATE TRIGGER mutation_attempts_delete_guard\n\
      BEFORE DELETE ON mutation_attempts\n\
      BEGIN SELECT RAISE(ABORT, 'mutation attempts are append-only'); END;",
};

const KNOWLEDGE_INDEX_FILE_ANALYSIS_MIGRATION: Migration = Migration {
    version: 23,
    name: "published_index_file_analysis",
    sql: "CREATE TABLE index_file_analyses (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),\n\
      language TEXT NOT NULL CHECK (language IN\n\
        ('generic', 'rust', 'typescript-javascript', 'python')),\n\
      adapter_version TEXT CHECK\n\
        (adapter_version IS NULL OR length(CAST(adapter_version AS BLOB)) BETWEEN 1 AND 128),\n\
      total_bytes INTEGER CHECK (total_bytes IS NULL OR total_bytes BETWEEN 0 AND 4294967295),\n\
      covered_bytes INTEGER CHECK\n\
        (covered_bytes IS NULL OR covered_bytes BETWEEN 0 AND total_bytes),\n\
      incomplete_regions INTEGER CHECK\n\
        (incomplete_regions IS NULL OR incomplete_regions BETWEEN 0 AND 4294967295),\n\
      CHECK ((language = 'generic' AND adapter_version IS NULL AND total_bytes IS NULL\n\
          AND covered_bytes IS NULL AND incomplete_regions IS NULL) OR\n\
        (language <> 'generic' AND adapter_version IS NOT NULL AND total_bytes IS NOT NULL\n\
          AND covered_bytes IS NOT NULL AND incomplete_regions IS NOT NULL)),\n\
      PRIMARY KEY (index_run_id, repository_path),\n\
      FOREIGN KEY (index_run_id, repository_path, content_hash)\n\
        REFERENCES file_revisions(index_run_id, repository_path, content_hash)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE TABLE index_parse_diagnostics (\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),\n\
      diagnostic_sequence INTEGER NOT NULL CHECK (diagnostic_sequence > 0),\n\
      code TEXT NOT NULL CHECK (code IN ('syntax-error', 'missing-syntax',\n\
        'invalid-encoding', 'unsupported-syntax', 'output-truncated')),\n\
      severity TEXT NOT NULL CHECK (severity IN ('error', 'warning', 'information')),\n\
      start_byte INTEGER NOT NULL CHECK (start_byte BETWEEN 0 AND 4294967295),\n\
      end_byte INTEGER NOT NULL CHECK (end_byte BETWEEN start_byte AND 4294967295),\n\
      start_row INTEGER NOT NULL CHECK (start_row BETWEEN 0 AND 4294967295),\n\
      start_column INTEGER NOT NULL CHECK (start_column BETWEEN 0 AND 4294967295),\n\
      end_row INTEGER NOT NULL CHECK (end_row BETWEEN 0 AND 4294967295),\n\
      end_column INTEGER NOT NULL CHECK (end_column BETWEEN 0 AND 4294967295),\n\
      message TEXT NOT NULL CHECK (length(CAST(message AS BLOB)) BETWEEN 1 AND 1024),\n\
      PRIMARY KEY (index_run_id, repository_path, diagnostic_sequence),\n\
      FOREIGN KEY (index_run_id, repository_path)\n\
        REFERENCES index_file_analyses(index_run_id, repository_path)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;",
};

const KNOWLEDGE_AGENT_SESSION_MIGRATION: Migration = Migration {
    version: 24,
    name: "agent_conversation_sessions",
    sql: "CREATE TABLE agent_session_revisions (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      session_id BLOB NOT NULL CHECK (length(session_id) = 32),\n\
      revision INTEGER NOT NULL CHECK (revision > 0),\n\
      title TEXT NOT NULL CHECK (length(CAST(title AS BLOB)) BETWEEN 1 AND 120),\n\
      mode TEXT NOT NULL CHECK (mode IN ('ask', 'plan', 'agent')),\n\
      state TEXT NOT NULL CHECK (state IN\n\
        ('draft', 'running', 'awaiting_user', 'awaiting_plan_review', 'awaiting_approval',\n\
         'paused', 'completed', 'failed', 'cancelled', 'archived')),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      updated_at_unix_millis INTEGER NOT NULL\n\
        CHECK (updated_at_unix_millis >= created_at_unix_millis),\n\
      latest_sequence INTEGER CHECK (latest_sequence IS NULL OR latest_sequence > 0),\n\
      active_work_item_id BLOB\n\
        CHECK (active_work_item_id IS NULL OR length(active_work_item_id) = 32),\n\
      active_task_id BLOB CHECK (active_task_id IS NULL OR length(active_task_id) = 32),\n\
      active_work_item_mode TEXT\n\
        CHECK (active_work_item_mode IS NULL OR active_work_item_mode IN ('ask', 'plan', 'agent')),\n\
      current_plan_revision INTEGER\n\
        CHECK (current_plan_revision IS NULL OR current_plan_revision > 0),\n\
      presentation_deleted INTEGER NOT NULL CHECK (presentation_deleted IN (0, 1)),\n\
      CHECK ((active_work_item_id IS NULL AND active_task_id IS NULL\n\
          AND active_work_item_mode IS NULL) OR\n\
        (active_work_item_id IS NOT NULL AND active_task_id IS NOT NULL\n\
          AND active_work_item_mode IS NOT NULL)),\n\
      PRIMARY KEY (worktree_id, session_id, revision),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX agent_session_revisions_recent_idx\n\
        ON agent_session_revisions\n\
          (worktree_id, updated_at_unix_millis DESC, session_id, revision DESC);\n\
      CREATE TABLE agent_session_entries (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      session_id BLOB NOT NULL CHECK (length(session_id) = 32),\n\
      session_revision INTEGER NOT NULL CHECK (session_revision > 0),\n\
      sequence INTEGER NOT NULL CHECK (sequence > 0),\n\
      kind TEXT NOT NULL CHECK (kind IN\n\
        ('user_message', 'assistant_summary', 'plan', 'final_report', 'activity')),\n\
      content TEXT NOT NULL CHECK (length(CAST(content AS BLOB)) BETWEEN 1 AND 262144),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      work_item_id BLOB CHECK (work_item_id IS NULL OR length(work_item_id) = 32),\n\
      task_id BLOB CHECK (task_id IS NULL OR length(task_id) = 32),\n\
      plan_revision INTEGER CHECK (plan_revision IS NULL OR plan_revision > 0),\n\
      CHECK ((kind = 'plan' AND plan_revision IS NOT NULL) OR\n\
        (kind <> 'plan' AND plan_revision IS NULL)),\n\
      CHECK ((work_item_id IS NULL AND task_id IS NULL) OR\n\
        (work_item_id IS NOT NULL AND task_id IS NOT NULL)),\n\
      PRIMARY KEY (worktree_id, session_id, sequence),\n\
      FOREIGN KEY (worktree_id, session_id, session_revision)\n\
        REFERENCES agent_session_revisions(worktree_id, session_id, revision)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TRIGGER agent_session_revisions_update_guard\n\
      BEFORE UPDATE ON agent_session_revisions BEGIN\n\
        SELECT RAISE(ABORT, 'Agent session revisions are immutable');\n\
      END;\n\
      CREATE TRIGGER agent_session_revisions_delete_guard\n\
      BEFORE DELETE ON agent_session_revisions BEGIN\n\
        SELECT RAISE(ABORT, 'Agent session revisions are append-only');\n\
      END;\n\
      CREATE TRIGGER agent_session_entries_update_guard\n\
      BEFORE UPDATE ON agent_session_entries BEGIN\n\
        SELECT RAISE(ABORT, 'Agent session entries are immutable');\n\
      END;",
};

const KNOWLEDGE_DEEP_MAP_JOURNAL_MIGRATION: Migration = Migration {
    version: 25,
    name: "deep_map_run_journal",
    sql: "CREATE TABLE deep_map_runs (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      index_run_id BLOB NOT NULL CHECK (length(index_run_id) = 32),\n\
      snapshot_id BLOB NOT NULL CHECK (length(snapshot_id) = 32),\n\
      mode TEXT NOT NULL CHECK (mode IN ('fast', 'standard', 'thorough')),\n\
      token_budget INTEGER NOT NULL CHECK (token_budget IN (8000, 32000, 128000)),\n\
      time_budget_millis INTEGER NOT NULL CHECK (time_budget_millis IN (60000, 120000, 600000)),\n\
      tool_budget INTEGER NOT NULL CHECK (tool_budget IN (16, 64, 256)),\n\
      provider_id TEXT NOT NULL CHECK (length(CAST(provider_id AS BLOB)) BETWEEN 1 AND 128),\n\
      model_id TEXT NOT NULL CHECK (length(CAST(model_id AS BLOB)) BETWEEN 1 AND 512),\n\
      profile_id BLOB NOT NULL CHECK (length(profile_id) = 32),\n\
      profile_version INTEGER NOT NULL CHECK (profile_version > 0),\n\
      context_tokens INTEGER NOT NULL CHECK (context_tokens > 0),\n\
      output_tokens INTEGER NOT NULL CHECK (output_tokens > 0),\n\
      state TEXT NOT NULL CHECK (state IN\n\
        ('queued', 'running', 'pausing', 'paused', 'cancelling', 'succeeded', 'failed',\n\
         'cancelled', 'interrupted')),\n\
      created_at_unix_millis INTEGER NOT NULL CHECK (created_at_unix_millis >= 0),\n\
      updated_at_unix_millis INTEGER NOT NULL\n\
        CHECK (updated_at_unix_millis >= created_at_unix_millis),\n\
      confirmed_steps INTEGER NOT NULL CHECK (confirmed_steps >= 0),\n\
      total_steps INTEGER NOT NULL CHECK (total_steps >= confirmed_steps),\n\
      latest_event_sequence INTEGER NOT NULL CHECK (latest_event_sequence > 0),\n\
      diagnostic_code TEXT CHECK (diagnostic_code IS NULL OR diagnostic_code IN\n\
        ('no-published-index', 'stale-index', 'planning', 'model-unavailable',\n\
         'model-rejected', 'model-timeout', 'invalid-model-response', 'read', 'verification',\n\
         'publication-rejected', 'publication-storage', 'publication-timeout',\n\
         'publication-progress', 'invalid-checkpoint', 'progress-unavailable', 'interrupted')),\n\
      details_incomplete INTEGER NOT NULL CHECK (details_incomplete IN (0, 1)),\n\
      plan_stop_reason TEXT CHECK (plan_stop_reason IS NULL OR plan_stop_reason IN\n\
        ('coverage-planned', 'budget-exhausted', 'below-gain-threshold', 'no-eligible-seed')),\n\
      publication_result TEXT CHECK (publication_result IS NULL OR publication_result IN\n\
        ('published', 'already-current')),\n\
      CHECK ((state = 'failed' AND diagnostic_code IS NOT NULL) OR\n\
        (state <> 'failed' AND diagnostic_code IS NULL)),\n\
      PRIMARY KEY (worktree_id, run_id),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE CASCADE\n\
      ) STRICT;\n\
      CREATE INDEX deep_map_runs_recent_idx ON deep_map_runs\n\
        (worktree_id, updated_at_unix_millis DESC, run_id DESC);\n\
      CREATE INDEX deep_map_runs_anchor_idx ON deep_map_runs\n\
        (worktree_id, index_run_id, snapshot_id, updated_at_unix_millis DESC);\n\
      CREATE TABLE deep_map_steps (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      step_position INTEGER NOT NULL CHECK (step_position > 0),\n\
      module_id BLOB NOT NULL CHECK (length(module_id) = 32),\n\
      target_kind TEXT NOT NULL CHECK (target_kind IN ('module', 'manifest', 'symbol')),\n\
      seed_reason TEXT NOT NULL CHECK (seed_reason IN\n\
        ('manifest', 'entrypoint', 'central-symbol', 'test-root', 'graph-community',\n\
         'uncovered-module')),\n\
      reserved_tokens INTEGER NOT NULL CHECK (reserved_tokens >= 0),\n\
      reserved_time_millis INTEGER NOT NULL CHECK (reserved_time_millis >= 0),\n\
      reserved_tool_calls INTEGER NOT NULL CHECK (reserved_tool_calls >= 0),\n\
      information_gain_basis_points INTEGER NOT NULL\n\
        CHECK (information_gain_basis_points BETWEEN 0 AND 10000),\n\
      coverage_field_count INTEGER NOT NULL CHECK (coverage_field_count > 0),\n\
      evidence_requirement TEXT NOT NULL CHECK (evidence_requirement = 'field-evidence'),\n\
      verification_method TEXT NOT NULL CHECK (verification_method = 'published-index-evidence'),\n\
      confirmed INTEGER NOT NULL CHECK (confirmed IN (0, 1)),\n\
      PRIMARY KEY (worktree_id, run_id, step_position),\n\
      FOREIGN KEY (worktree_id, run_id) REFERENCES deep_map_runs(worktree_id, run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TABLE deep_map_events (\n\
      worktree_id BLOB NOT NULL CHECK (length(worktree_id) = 32),\n\
      run_id BLOB NOT NULL CHECK (length(run_id) = 32),\n\
      sequence INTEGER NOT NULL CHECK (sequence > 0),\n\
      occurred_at_unix_millis INTEGER NOT NULL CHECK (occurred_at_unix_millis >= 0),\n\
      state TEXT NOT NULL CHECK (state IN\n\
        ('queued', 'running', 'pausing', 'paused', 'cancelling', 'succeeded', 'failed',\n\
         'cancelled', 'interrupted')),\n\
      phase TEXT CHECK (phase IS NULL OR phase IN\n\
        ('planning', 'exploring', 'claiming', 'verifying', 'publishing')),\n\
      target_kind TEXT CHECK (target_kind IS NULL OR target_kind IN\n\
        ('project', 'module', 'manifest', 'symbol')),\n\
      safe_action TEXT CHECK (safe_action IS NULL OR safe_action IN\n\
        ('build-plan', 'inspect', 'search', 'propose', 'generate-claims',\n\
         'verify-evidence', 'publish-cards')),\n\
      module_id BLOB CHECK (module_id IS NULL OR length(module_id) = 32),\n\
      step_position INTEGER CHECK (step_position IS NULL OR step_position > 0),\n\
      total_steps INTEGER CHECK (total_steps IS NULL OR total_steps > 0),\n\
      confirmed INTEGER NOT NULL CHECK (confirmed IN (0, 1)),\n\
      result TEXT NOT NULL CHECK (result IN\n\
        ('pending', 'confirmed', 'already-current', 'published', 'paused', 'resumed',\n\
         'cancelled', 'failed', 'interrupted')),\n\
      diagnostic_code TEXT CHECK (diagnostic_code IS NULL OR diagnostic_code IN\n\
        ('no-published-index', 'stale-index', 'planning', 'model-unavailable',\n\
         'model-rejected', 'model-timeout', 'invalid-model-response', 'read', 'verification',\n\
         'publication-rejected', 'publication-storage', 'publication-timeout',\n\
         'publication-progress', 'invalid-checkpoint', 'progress-unavailable', 'interrupted')),\n\
      CHECK ((step_position IS NULL AND total_steps IS NULL) OR\n\
        (step_position IS NOT NULL AND total_steps IS NOT NULL\n\
         AND step_position <= total_steps)),\n\
      CHECK ((state = 'failed' AND result = 'failed' AND diagnostic_code IS NOT NULL) OR\n\
        (state <> 'failed' AND result <> 'failed' AND diagnostic_code IS NULL)),\n\
      PRIMARY KEY (worktree_id, run_id, sequence),\n\
      FOREIGN KEY (worktree_id, run_id) REFERENCES deep_map_runs(worktree_id, run_id)\n\
        ON UPDATE RESTRICT ON DELETE RESTRICT\n\
      ) STRICT;\n\
      CREATE TRIGGER deep_map_events_update_guard BEFORE UPDATE ON deep_map_events BEGIN\n\
        SELECT RAISE(ABORT, 'Deep-Map events are immutable');\n\
      END;\n\
      CREATE TRIGGER deep_map_events_delete_guard BEFORE DELETE ON deep_map_events BEGIN\n\
        SELECT RAISE(ABORT, 'Deep-Map events are append-only');\n\
      END;",
};

const KNOWLEDGE_CARD_SEARCH_REPAIR_MIGRATION: Migration = Migration {
    version: 26,
    name: "repair_legacy_card_search_projection",
    sql: "INSERT INTO card_fts (index_run_id, card_id, title, purpose, body)\n\
      SELECT cards.source_index_run_id, cards.card_id,\n\
        COALESCE((\n\
          SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
            SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
            WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
              AND values_row.card_id = cards.card_id AND values_row.field_kind = 'title'\n\
            ORDER BY values_row.value_index\n\
          ) AS ordered\n\
        ), ''),\n\
        COALESCE((\n\
          SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
            SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
            WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
              AND values_row.card_id = cards.card_id AND values_row.field_kind = 'purpose'\n\
            ORDER BY values_row.value_index\n\
          ) AS ordered\n\
        ), ''),\n\
        COALESCE((\n\
          SELECT group_concat(ordered.search_line, char(10)) FROM (\n\
            SELECT values_row.field_kind || ': ' || values_row.field_value AS search_line\n\
            FROM module_card_field_values AS values_row\n\
            WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
              AND values_row.card_id = cards.card_id\n\
            ORDER BY CASE values_row.field_kind\n\
              WHEN 'title' THEN 1 WHEN 'paths' THEN 2 WHEN 'purpose' THEN 3\n\
              WHEN 'responsibilities' THEN 4 WHEN 'public-surface' THEN 5\n\
              WHEN 'entrypoints' THEN 6 WHEN 'dependencies' THEN 7\n\
              WHEN 'data-flows' THEN 8 WHEN 'invariants' THEN 9 WHEN 'tests' THEN 10\n\
              WHEN 'risks' THEN 11 WHEN 'open-questions' THEN 12 ELSE 13 END,\n\
              values_row.value_index\n\
          ) AS ordered\n\
        ), '')\n\
      FROM module_cards AS cards\n\
      WHERE cards.status = 'published' AND NOT EXISTS (\n\
        SELECT 1 FROM card_fts AS search\n\
        WHERE search.index_run_id = cards.source_index_run_id\n\
          AND search.card_id = cards.card_id\n\
      );\n\
      UPDATE lexical_search_projections\n\
      SET card_count = (\n\
        SELECT COUNT(*) FROM module_cards\n\
        WHERE source_index_run_id = lexical_search_projections.index_run_id\n\
          AND status = 'published'\n\
      )\n\
      WHERE (\n\
        SELECT COUNT(*) FROM module_cards\n\
        WHERE source_index_run_id = lexical_search_projections.index_run_id\n\
          AND status = 'published'\n\
      ) = (\n\
        SELECT COUNT(*) FROM card_fts\n\
        WHERE index_run_id = lexical_search_projections.index_run_id\n\
      );",
};

const KNOWLEDGE_RECURRENT_CARD_SEARCH_REPAIR_MIGRATION: Migration = Migration {
    version: 27,
    name: "repair_recurrent_card_search_projection",
    sql: "INSERT INTO card_fts (index_run_id, card_id, title, purpose, body)\n\
      SELECT cards.source_index_run_id, cards.card_id,\n\
        COALESCE((\n\
          SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
            SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
            WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
              AND values_row.card_id = cards.card_id AND values_row.field_kind = 'title'\n\
            ORDER BY values_row.value_index\n\
          ) AS ordered\n\
        ), ''),\n\
        COALESCE((\n\
          SELECT group_concat(ordered.field_value, char(10)) FROM (\n\
            SELECT values_row.field_value FROM module_card_field_values AS values_row\n\
            WHERE values_row.source_index_run_id = cards.source_index_run_id\n\
              AND values_row.card_id = cards.card_id AND values_row.field_kind = 'purpose'\n\
            ORDER BY values_row.value_index\n\
          ) AS ordered\n\
        ), ''),\n\
        COALESCE((\n\
          SELECT group_concat(ordered.field_kind || ': ' || ordered.joined_values, char(10))\n\
          FROM (\n\
            SELECT fields.field_kind, COALESCE((\n\
              SELECT group_concat(ordered_values.field_value, char(10)) FROM (\n\
                SELECT values_row.field_value\n\
                FROM module_card_field_values AS values_row\n\
                WHERE values_row.source_index_run_id = fields.source_index_run_id\n\
                  AND values_row.card_id = fields.card_id\n\
                  AND values_row.field_kind = fields.field_kind\n\
                ORDER BY values_row.value_index\n\
              ) AS ordered_values\n\
            ), '') AS joined_values\n\
            FROM module_card_fields AS fields\n\
            WHERE fields.source_index_run_id = cards.source_index_run_id\n\
              AND fields.card_id = cards.card_id\n\
            ORDER BY CASE fields.field_kind\n\
              WHEN 'title' THEN 1 WHEN 'paths' THEN 2 WHEN 'purpose' THEN 3\n\
              WHEN 'responsibilities' THEN 4 WHEN 'public-surface' THEN 5\n\
              WHEN 'entrypoints' THEN 6 WHEN 'dependencies' THEN 7\n\
              WHEN 'data-flows' THEN 8 WHEN 'invariants' THEN 9 WHEN 'tests' THEN 10\n\
              WHEN 'risks' THEN 11 WHEN 'open-questions' THEN 12 ELSE 13 END\n\
          ) AS ordered\n\
        ), '')\n\
      FROM module_cards AS cards\n\
      WHERE cards.status = 'published'\n\
        AND EXISTS (\n\
          SELECT 1 FROM index_runs\n\
          WHERE index_runs.index_run_id = cards.source_index_run_id\n\
            AND index_runs.status = 'published'\n\
        )\n\
        AND EXISTS (\n\
          SELECT 1 FROM lexical_search_projections\n\
          WHERE lexical_search_projections.index_run_id = cards.source_index_run_id\n\
        )\n\
        AND EXISTS (\n\
          SELECT 1 FROM module_card_lifecycle AS lifecycle\n\
          WHERE lifecycle.source_index_run_id = cards.source_index_run_id\n\
            AND lifecycle.card_id = cards.card_id AND lifecycle.status = 'published'\n\
        )\n\
        AND EXISTS (\n\
          SELECT 1 FROM module_card_fields AS fields\n\
          WHERE fields.source_index_run_id = cards.source_index_run_id\n\
            AND fields.card_id = cards.card_id\n\
        )\n\
        AND NOT EXISTS (\n\
          SELECT 1 FROM module_card_fields AS fields\n\
          WHERE fields.source_index_run_id = cards.source_index_run_id\n\
            AND fields.card_id = cards.card_id\n\
            AND NOT EXISTS (\n\
              SELECT 1 FROM module_card_field_values AS values_row\n\
              WHERE values_row.source_index_run_id = fields.source_index_run_id\n\
                AND values_row.card_id = fields.card_id\n\
                AND values_row.field_kind = fields.field_kind\n\
            )\n\
        )\n\
        AND NOT EXISTS (\n\
          SELECT 1 FROM card_fts AS search\n\
          WHERE search.index_run_id = cards.source_index_run_id\n\
            AND search.card_id = cards.card_id\n\
        );\n\
      UPDATE lexical_search_projections\n\
      SET card_count = (\n\
        SELECT COUNT(*) FROM module_cards\n\
        WHERE source_index_run_id = lexical_search_projections.index_run_id\n\
          AND status = 'published'\n\
      )\n\
      WHERE (\n\
        SELECT COUNT(*) FROM module_cards\n\
        WHERE source_index_run_id = lexical_search_projections.index_run_id\n\
          AND status = 'published'\n\
      ) = (\n\
        SELECT COUNT(*) FROM card_fts\n\
        WHERE index_run_id = lexical_search_projections.index_run_id\n\
      );",
};

const KNOWLEDGE_MONOTONE_INDEX_RUN_SEQUENCE_MIGRATION: Migration = Migration {
    version: 28,
    name: "monotone_index_run_sequence_across_rebuilds",
    sql: "CREATE TABLE index_run_sequence_cursors (\n\
      worktree_id BLOB PRIMARY KEY NOT NULL CHECK (length(worktree_id) = 32),\n\
      last_sequence INTEGER NOT NULL CHECK (last_sequence > 0),\n\
      FOREIGN KEY (worktree_id) REFERENCES worktrees(worktree_id)\n\
        ON UPDATE CASCADE ON DELETE RESTRICT\n\
      ) STRICT;\n\
      INSERT INTO index_run_sequence_cursors (worktree_id, last_sequence)\n\
      SELECT worktree_id, MAX(run_sequence) FROM index_runs GROUP BY worktree_id;",
};

const KNOWLEDGE_MIGRATIONS: &[Migration] = &[
    KNOWLEDGE_BOOTSTRAP_MIGRATION,
    KNOWLEDGE_PROJECT_INDEX_MIGRATION,
    KNOWLEDGE_RECONCILIABLE_IDENTITIES_MIGRATION,
    KNOWLEDGE_ATOMIC_INDEX_PUBLICATION_MIGRATION,
    KNOWLEDGE_EXACT_SEARCH_MIGRATION,
    KNOWLEDGE_LEXICAL_SEARCH_MIGRATION,
    KNOWLEDGE_SEMANTIC_EMBEDDING_MIGRATION,
    KNOWLEDGE_MODULE_PROJECTION_MIGRATION,
    KNOWLEDGE_VERIFIED_MODULE_CARDS_MIGRATION,
    KNOWLEDGE_CARD_INVALIDATION_MIGRATION,
    KNOWLEDGE_GOAL_CONTRACT_MIGRATION,
    KNOWLEDGE_TASK_LEDGER_MIGRATION,
    KNOWLEDGE_RUN_JOURNAL_MIGRATION,
    KNOWLEDGE_MODEL_PROFILE_RUN_REFERENCE_MIGRATION,
    KNOWLEDGE_AGENT_RUN_BUDGET_MIGRATION,
    KNOWLEDGE_AGENT_TOOL_EVIDENCE_MIGRATION,
    KNOWLEDGE_AGENT_RECOVERY_MIGRATION,
    KNOWLEDGE_POLICY_APPROVAL_MIGRATION,
    KNOWLEDGE_COMMAND_ALLOWLIST_MIGRATION,
    KNOWLEDGE_VERIFICATION_ENGINE_MIGRATION,
    KNOWLEDGE_AGENT_ACTION_V2_MIGRATION,
    KNOWLEDGE_MUTATION_RECOVERY_MIGRATION,
    KNOWLEDGE_INDEX_FILE_ANALYSIS_MIGRATION,
    KNOWLEDGE_AGENT_SESSION_MIGRATION,
    KNOWLEDGE_DEEP_MAP_JOURNAL_MIGRATION,
    KNOWLEDGE_CARD_SEARCH_REPAIR_MIGRATION,
    KNOWLEDGE_RECURRENT_CARD_SEARCH_REPAIR_MIGRATION,
    KNOWLEDGE_MONOTONE_INDEX_RUN_SEQUENCE_MIGRATION,
];

const CATALOG_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.catalog-migration.v1";
const KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.knowledge-migration.v1";

/// Monotone version of the global catalog schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogSchemaVersion(u32);

impl CatalogSchemaVersion {
    /// Current schema version understood by this build.
    pub const CURRENT: Self = Self::new(7);

    /// Creates a schema version from a migration number.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Monotone version of one worktree knowledge database schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnowledgeSchemaVersion(u32);

impl KnowledgeSchemaVersion {
    /// Current worktree schema version understood by this build.
    pub const CURRENT: Self = Self::new(28);

    /// Creates a schema version from a migration number.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

pub(crate) async fn migrate_catalog(
    connection: &Connection,
) -> Result<CatalogSchemaVersion, MigrationError> {
    migrate(
        connection,
        CATALOG_MIGRATIONS,
        CatalogSchemaVersion::CURRENT.get(),
        CATALOG_MIGRATION_CHECKSUM_DOMAIN,
    )
    .await
    .map(CatalogSchemaVersion::new)
}

pub(crate) async fn migrate_knowledge(
    connection: &Connection,
    repository_id: &[u8; 32],
    worktree_id: &[u8; 32],
) -> Result<KnowledgeSchemaVersion, MigrationError> {
    let current = read_user_version(connection)
        .await
        .map_err(MigrationError::ReadVersion)?;
    if current == 0 {
        verify_history(
            connection,
            KNOWLEDGE_MIGRATIONS,
            current,
            KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
        )
        .await?;
        apply_knowledge_bootstrap(connection, repository_id, worktree_id).await?;
    }
    migrate(
        connection,
        KNOWLEDGE_MIGRATIONS,
        KnowledgeSchemaVersion::CURRENT.get(),
        KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
    )
    .await
    .map(KnowledgeSchemaVersion::new)
}

pub(crate) async fn verify_knowledge_migration_history(
    connection: &Connection,
    current: u32,
) -> Result<(), MigrationError> {
    verify_history(
        connection,
        KNOWLEDGE_MIGRATIONS,
        current,
        KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
    )
    .await
}

async fn apply_knowledge_bootstrap(
    connection: &Connection,
    repository_id: &[u8; 32],
    worktree_id: &[u8; 32],
) -> Result<(), MigrationError> {
    let migration = &KNOWLEDGE_BOOTSTRAP_MIGRATION;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|source| MigrationError::Begin {
            version: migration.version,
            source,
        })?;
    let result = async {
        apply_migration_body(&transaction, migration, KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN).await?;
        transaction
            .execute(
                "INSERT INTO worktree_storage_identity (singleton, repository_id, worktree_id)\n\
                 VALUES (1, ?1, ?2)",
                params![repository_id.to_vec(), worktree_id.to_vec()],
            )
            .await?;
        Ok::<(), libsql::Error>(())
    }
    .await;

    if let Err(source) = result {
        return match transaction.rollback().await {
            Ok(()) => Err(MigrationError::Apply {
                version: migration.version,
                source,
            }),
            Err(source) => Err(MigrationError::Rollback {
                version: migration.version,
                source,
            }),
        };
    }

    transaction
        .commit()
        .await
        .map_err(|source| MigrationError::Commit {
            version: migration.version,
            source,
        })
}

async fn migrate(
    connection: &Connection,
    migrations: &[Migration],
    supported: u32,
    checksum_domain: &[u8],
) -> Result<u32, MigrationError> {
    let current = read_user_version(connection)
        .await
        .map_err(MigrationError::ReadVersion)?;
    if current > supported {
        return Err(MigrationError::NewerSchema { current, supported });
    }
    verify_history(connection, migrations, current, checksum_domain).await?;

    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current)
    {
        apply_migration(connection, migration, checksum_domain).await?;
    }

    verify_history(connection, migrations, supported, checksum_domain).await?;
    Ok(supported)
}

async fn apply_migration(
    connection: &Connection,
    migration: &Migration,
    checksum_domain: &[u8],
) -> Result<(), MigrationError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(|source| MigrationError::Begin {
            version: migration.version,
            source,
        })?;

    if let Err(source) = apply_migration_body(&transaction, migration, checksum_domain).await {
        return match transaction.rollback().await {
            Ok(()) => Err(MigrationError::Apply {
                version: migration.version,
                source,
            }),
            Err(source) => Err(MigrationError::Rollback {
                version: migration.version,
                source,
            }),
        };
    }

    transaction
        .commit()
        .await
        .map_err(|source| MigrationError::Commit {
            version: migration.version,
            source,
        })
}

async fn apply_migration_body(
    transaction: &libsql::Transaction,
    migration: &Migration,
    checksum_domain: &[u8],
) -> libsql::Result<()> {
    transaction.execute_batch(migration.sql).await?;
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![
                i64::from(migration.version),
                migration.name,
                migration_checksum(migration, checksum_domain).to_vec()
            ],
        )
        .await?;
    transaction
        .execute_batch(&format!("PRAGMA user_version = {}", migration.version))
        .await?;
    Ok(())
}

async fn verify_history(
    connection: &Connection,
    migrations: &[Migration],
    current: u32,
    checksum_domain: &[u8],
) -> Result<(), MigrationError> {
    if current == 0 {
        return Ok(());
    }

    let maximum = query_i64(
        connection,
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
    )
    .await
    .map_err(MigrationError::ReadHistory)?;
    if maximum != i64::from(current) {
        return Err(MigrationError::HistoryMismatch { version: current });
    }

    for migration in migrations
        .iter()
        .filter(|migration| migration.version <= current)
    {
        let mut rows = connection
            .query(
                "SELECT name, checksum FROM schema_migrations WHERE version = ?1",
                [i64::from(migration.version)],
            )
            .await
            .map_err(MigrationError::ReadHistory)?;
        let row = rows
            .next()
            .await
            .map_err(MigrationError::ReadHistory)?
            .ok_or(MigrationError::HistoryMismatch {
                version: migration.version,
            })?;
        let name: String = row.get(0).map_err(MigrationError::ReadHistory)?;
        let checksum: Vec<u8> = row.get(1).map_err(MigrationError::ReadHistory)?;
        if name != migration.name
            || checksum.as_slice() != migration_checksum(migration, checksum_domain)
        {
            return Err(MigrationError::HistoryMismatch {
                version: migration.version,
            });
        }
    }
    Ok(())
}

pub(crate) async fn read_user_version(connection: &Connection) -> libsql::Result<u32> {
    let raw = query_i64(connection, "PRAGMA user_version").await?;
    let value = u32::try_from(raw).map_err(|_| libsql::Error::InvalidColumnType)?;
    Ok(value)
}

pub(crate) async fn query_i64(connection: &Connection, sql: &str) -> libsql::Result<i64> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    row.get(0)
}

pub(crate) async fn query_string(connection: &Connection, sql: &str) -> libsql::Result<String> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(libsql::Error::QueryReturnedNoRows)?;
    row.get(0)
}

fn migration_checksum(migration: &Migration, checksum_domain: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    update_checksum_field(&mut hasher, checksum_domain);
    update_checksum_field(&mut hasher, &migration.version.to_le_bytes());
    update_checksum_field(&mut hasher, migration.name.as_bytes());
    update_checksum_field(&mut hasher, migration.sql.as_bytes());
    *hasher.finalize().as_bytes()
}

fn update_checksum_field(hasher: &mut Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u128).to_le_bytes());
    hasher.update(value);
}

#[derive(Debug)]
pub(crate) enum MigrationError {
    ReadVersion(libsql::Error),
    NewerSchema { current: u32, supported: u32 },
    ReadHistory(libsql::Error),
    HistoryMismatch { version: u32 },
    Begin { version: u32, source: libsql::Error },
    Apply { version: u32, source: libsql::Error },
    Rollback { version: u32, source: libsql::Error },
    Commit { version: u32, source: libsql::Error },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadVersion(_) => formatter.write_str("could not read catalog schema version"),
            Self::NewerSchema { current, supported } => write!(
                formatter,
                "catalog schema {} is newer than supported schema {}",
                current, supported
            ),
            Self::ReadHistory(_) => formatter.write_str("could not read migration history"),
            Self::HistoryMismatch { version } => {
                write!(formatter, "migration history differs at schema {}", version)
            }
            Self::Begin { version, .. } => {
                write!(formatter, "could not begin migration {version}")
            }
            Self::Apply { version, .. } => {
                write!(formatter, "could not apply migration {version}")
            }
            Self::Rollback { version, .. } => {
                write!(formatter, "could not roll back migration {version}")
            }
            Self::Commit { version, .. } => {
                write!(formatter, "could not commit migration {version}")
            }
        }
    }
}

impl Error for MigrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadVersion(source)
            | Self::ReadHistory(source)
            | Self::Begin { source, .. }
            | Self::Apply { source, .. }
            | Self::Rollback { source, .. }
            | Self::Commit { source, .. } => Some(source),
            Self::NewerSchema { .. } | Self::HistoryMismatch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CATALOG_MIGRATION_CHECKSUM_DOMAIN, CATALOG_MIGRATIONS, CatalogSchemaVersion,
        KNOWLEDGE_MIGRATIONS, KnowledgeSchemaVersion, Migration, MigrationError, migrate,
        query_i64, query_string,
    };
    use libsql::params;
    use std::collections::HashSet;

    #[test]
    fn catalog_migration_definitions_are_contiguous_and_uniquely_named() {
        assert_migration_definitions(CATALOG_MIGRATIONS, CatalogSchemaVersion::CURRENT.get());
    }

    #[test]
    fn knowledge_migration_definitions_are_contiguous_and_uniquely_named() {
        assert_migration_definitions(KNOWLEDGE_MIGRATIONS, KnowledgeSchemaVersion::CURRENT.get());
    }

    fn assert_migration_definitions(migrations: &[Migration], current: u32) {
        assert_eq!(migrations.len(), current as usize);
        let mut names = HashSet::new();
        for (index, migration) in migrations.iter().enumerate() {
            assert_eq!(migration.version as usize, index + 1);
            assert!(names.insert(migration.name));
            assert!(!migration.sql.trim().is_empty());
        }
    }

    #[test]
    fn empty_knowledge_schema_migrates_to_current() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;

            let version = super::migrate_knowledge(&connection, &[1; 32], &[2; 32]).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (\n\
                     'schema_migrations', 'worktree_storage_identity', 'repositories', 'worktrees',\n\
                     'snapshots', 'snapshot_adapter_revisions', 'snapshot_changes', 'index_runs',\n\
                     'file_revisions', 'symbols', 'symbol_edges', 'unresolved_edges',\n\
                     'ranking_projections', 'exact_search_projections', 'exact_search_symbols',\n\
                     'exact_search_manifests', 'lexical_search_projections', 'symbol_fts',\n\
                     'path_fts', 'card_fts', 'module_card_lifecycle', 'claim_lifecycle',\n\
                     'evidence_invalidations', 'module_remap_queue', 'tasks',\n\
                     'goal_contract_revisions', 'acceptance_criteria',\n\
                     'goal_contract_constraints', 'goal_contract_non_goals',\n\
                     'goal_contract_user_decisions', 'task_ledgers', 'task_steps',\n\
                     'task_step_dependencies', 'task_step_expected_evidence',\n\
                     'task_step_attempts', 'task_step_attempt_evidence',\n\
                     'task_step_verifications', 'task_step_verification_evidence',\n\
                     'task_step_stale_evidence', 'task_ledger_replans',\n\
                     'task_ledger_replan_retirements', 'task_ledger_replan_additions',\n\
                     'agent_runs', 'run_events', 'tool_runs', 'tool_evidence',\n\
                     'tool_run_attempts', 'mutation_attempts', 'approval_requests',\n\
                     'approval_grants',\n\
                     'policy_decisions', 'command_allowlist_revisions',\n\
                     'command_allowlist_entries', 'task_step_acceptance_criteria',\n\
                     'verification_specs_v1', 'verification_spec_paths',\n\
                     'verification_evidence', 'verification_process_evidence',\n\
                     'verification_test_cases', 'verification_diagnostic_reports',\n\
                     'verification_diff_evidence', 'verification_diff_paths',\n\
                     'verification_user_confirmations', 'verification_evidence_dependencies',\n\
                     'agent_session_revisions', 'agent_session_entries',\n\
                     'deep_map_runs', 'deep_map_steps', 'deep_map_events',\n\
                     'index_run_sequence_cursors'\n\
                     )",
                )
                .await?,
                70
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_runs')
                     WHERE name IN ('model_profile_id', 'model_profile_schema_version')",
                )
                .await?,
                2
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_runs') WHERE name IN (
                     'turn_limit', 'prompt_token_limit', 'output_token_limit', 'action_limit',
                     'duration_limit_millis', 'repair_limit', 'turn_count',
                     'prompt_tokens_used', 'output_tokens_used', 'action_count', 'repair_count')",
                )
                .await?,
                11
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('run_events') WHERE name IN (
                     'turn_prompt_tokens', 'turn_output_tokens', 'turn_action_kind',
                     'turn_repair_used', 'turn_action_kind_v2')",
                )
                .await?,
                5
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name IN ('agent_runs_profile_insert_guard',
                       'agent_runs_profile_update_guard')",
                )
                .await?,
                2
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN (
                     'agent_runs_budget_immutable_guard',
                     'run_events_turn_charge_insert_guard',
                     'run_events_turn_action_v2_insert_guard')",
                )
                .await?,
                3
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn command_allowlist_history_allows_only_identity_reconciliation_cascade()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [1; 32];
            let worktree_id = [2; 32];
            super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;
            connection
                .execute(
                    "INSERT INTO command_allowlist_revisions
                     (worktree_id, revision, catalog_id, confirmed_at_unix_millis)
                     VALUES (?1, 1, ?2, 1)",
                    params![worktree_id.to_vec(), vec![3_u8; 32]],
                )
                .await?;
            connection
                .execute(
                    "INSERT INTO command_allowlist_entries
                     (worktree_id, revision, ordinal, command_id)
                     VALUES (?1, 1, 0, ?2)",
                    params![worktree_id.to_vec(), vec![4_u8; 32]],
                )
                .await?;
            assert!(
                connection
                    .execute(
                        "UPDATE command_allowlist_revisions SET catalog_id = ?1
                         WHERE worktree_id = ?2 AND revision = 1",
                        params![vec![5_u8; 32], worktree_id.to_vec()],
                    )
                    .await
                    .is_err()
            );

            let reconciled_worktree_id = [6; 32];
            connection
                .execute(
                    "UPDATE worktrees SET worktree_id = ?1 WHERE worktree_id = ?2",
                    params![reconciled_worktree_id.to_vec(), worktree_id.to_vec()],
                )
                .await?;

            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM command_allowlist_entries
                     WHERE worktree_id = x'0606060606060606060606060606060606060606060606060606060606060606'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM pragma_foreign_key_check").await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_upgrades_v1_identity_into_project_repositories()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [7; 32];
            let worktree_id = [8; 32];

            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 1);

            let version =
                super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM repositories").await?,
                1
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM worktrees").await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    macro_rules! knowledge_upgrade_tests {
        ($(($name:ident, $predecessor:literal)),+ $(,)?) => {
            const KNOWLEDGE_UPGRADE_PREDECESSORS: &[u32] = &[$($predecessor),+];

            $(
                #[test]
                fn $name() -> Result<(), Box<dyn std::error::Error>> {
                    crate::run_native_libsql_test(async {
                        assert_knowledge_upgrade_from_predecessor($predecessor).await
                    })
                }
            )+
        };
    }

    knowledge_upgrade_tests!(
        (knowledge_upgrades_from_v1, 1),
        (knowledge_upgrades_from_v2, 2),
        (knowledge_upgrades_from_v3, 3),
        (knowledge_upgrades_from_v4, 4),
        (knowledge_upgrades_from_v5, 5),
        (knowledge_upgrades_from_v6, 6),
        (knowledge_upgrades_from_v7, 7),
        (knowledge_upgrades_from_v8, 8),
        (knowledge_upgrades_from_v9, 9),
        (knowledge_upgrades_from_v10, 10),
        (knowledge_upgrades_from_v11, 11),
        (knowledge_upgrades_from_v12, 12),
        (knowledge_upgrades_from_v13, 13),
        (knowledge_upgrades_from_v14, 14),
        (knowledge_upgrades_from_v15, 15),
        (knowledge_upgrades_from_v16, 16),
        (knowledge_upgrades_from_v17, 17),
        (knowledge_upgrades_from_v18, 18),
        (knowledge_upgrades_from_v19, 19),
        (knowledge_upgrades_from_v20, 20),
        (knowledge_upgrades_from_v21, 21),
        (knowledge_upgrades_from_v22, 22),
        (knowledge_upgrades_from_v23, 23),
        (knowledge_upgrades_from_v24, 24),
        (knowledge_upgrades_from_v25, 25),
        (knowledge_upgrades_from_v26, 26),
        (knowledge_upgrades_from_v27, 27),
    );

    #[test]
    fn knowledge_upgrade_matrix_covers_every_supported_predecessor() {
        assert_eq!(
            KNOWLEDGE_UPGRADE_PREDECESSORS,
            (1..KnowledgeSchemaVersion::CURRENT.get()).collect::<Vec<_>>()
        );
    }

    async fn assert_knowledge_upgrade_from_predecessor(
        predecessor: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = libsql::Builder::new_local(":memory:").build().await?;
        let connection = database.connect()?;
        let identity_byte = u8::try_from(predecessor)?;
        let worktree_byte = u8::try_from(predecessor.checked_add(10).ok_or("version overflow")?)?;
        let repository_id = [identity_byte; 32];
        let worktree_id = [worktree_byte; 32];
        super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
        if predecessor > 1 {
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..usize::try_from(predecessor)?],
                predecessor,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
        }
        assert_eq!(
            query_i64(&connection, "PRAGMA user_version").await?,
            i64::from(predecessor)
        );

        let version = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;
        assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
        assert_eq!(
            query_i64(&connection, "SELECT COUNT(*) FROM pragma_foreign_key_check").await?,
            0
        );
        Ok(())
    }

    #[test]
    fn failed_knowledge_v2_upgrade_preserves_the_v1_database()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [9; 32];
            let worktree_id = [10; 32];

            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            connection
                .execute("CREATE TABLE repositories (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 2, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 1);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'worktrees'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('repositories') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v3_upgrade_preserves_v2_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [11; 32];
            let worktree_id = [12; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..2],
                2,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "INSERT INTO snapshots (
                       snapshot_id, worktree_id, parent_snapshot_id, generation, head_kind,
                       head_object_id, head_reference, index_schema_version
                     ) VALUES (?1, ?2, NULL, 1, 'unborn', NULL, 'refs/heads/main', 1)",
                    params![vec![13; 32], worktree_id.to_vec()],
                )
                .await?;
            connection
                .execute("CREATE TABLE snapshot_changes_v3 (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 3, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 2);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM snapshots").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'repositories_v3'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('snapshot_changes_v3') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v4_upgrade_preserves_v3_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [31; 32];
            let worktree_id = [32; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..3],
                3,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "INSERT INTO snapshots (\n\
                     snapshot_id, worktree_id, parent_snapshot_id, generation, head_kind,\n\
                     head_object_id, head_reference, index_schema_version\n\
                     ) VALUES (?1, ?2, NULL, 1, 'unborn', NULL, 'refs/heads/main', 1)",
                    params![vec![33; 32], worktree_id.to_vec()],
                )
                .await?;
            connection
                .execute("CREATE TABLE symbols (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 4, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 3);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM snapshots").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('symbols') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'file_revisions'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v5_upgrade_preserves_v4_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [34; 32];
            let worktree_id = [35; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..4],
                4,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE exact_search_symbols (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 5, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 4);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('exact_search_symbols')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'exact_search_projections'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v6_upgrade_preserves_v5_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [36; 32];
            let worktree_id = [37; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..5],
                5,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE symbol_fts (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 6, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 5);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'lexical_search_projections'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('symbol_fts')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v7_upgrade_preserves_v6_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [38; 32];
            let worktree_id = [39; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..6],
                6,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE semantic_cards (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 7, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 6);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('semantic_cards')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'embeddings'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v8_upgrade_preserves_v7_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [40; 32];
            let worktree_id = [41; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..7],
                7,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE module_projections (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 8, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 7);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('module_projections')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'modules'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v9_upgrade_preserves_v8_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [42; 32];
            let worktree_id = [43; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..8],
                8,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE module_cards (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 9, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 8);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('module_cards')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'evidence_refs'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v10_upgrade_preserves_v9_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [44; 32];
            let worktree_id = [45; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..9],
                9,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE module_card_lifecycle (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 10, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 9);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('module_card_lifecycle')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'claim_lifecycle'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v11_upgrade_preserves_v10_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [46; 32];
            let worktree_id = [47; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..10],
                10,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE tasks (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 11, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 10);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('tasks')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'goal_contract_revisions'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v12_upgrade_preserves_v11_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [48; 32];
            let worktree_id = [49; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..11],
                11,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE task_ledgers (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 12, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 11);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('task_ledgers')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'task_steps'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v13_upgrade_preserves_v12_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [50; 32];
            let worktree_id = [51; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..12],
                12,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE agent_runs (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 13, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 12);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_runs')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master\n\
                     WHERE type = 'table' AND name = 'run_events'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v14_upgrade_preserves_v13_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [52; 32];
            let worktree_id = [53; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..13],
                13,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TRIGGER agent_runs_profile_insert_guard
                     BEFORE INSERT ON agent_runs BEGIN SELECT 1; END",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 14, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 13);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_runs')
                     WHERE name IN ('model_profile_id', 'model_profile_schema_version')",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name = 'agent_runs_profile_insert_guard'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v15_upgrade_preserves_v14_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [54; 32];
            let worktree_id = [55; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..14],
                14,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TRIGGER agent_runs_budget_immutable_guard
                     BEFORE INSERT ON agent_runs BEGIN SELECT 1; END",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 15, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 14);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_runs')
                     WHERE name = 'turn_limit'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('run_events')
                     WHERE name = 'turn_prompt_tokens'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name = 'agent_runs_budget_immutable_guard'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v16_upgrade_preserves_v15_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [56; 32];
            let worktree_id = [57; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..15],
                15,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE tool_runs (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 16, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 15);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('tool_runs')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'tool_evidence'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v17_upgrade_preserves_v16_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [58; 32];
            let worktree_id = [59; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..16],
                16,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE tool_run_attempts (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 17, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 16);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('tool_run_attempts')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
                     AND name = 'tool_run_attempts_one_in_flight_idx'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v18_upgrade_preserves_v17_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [60; 32];
            let worktree_id = [61; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..17],
                17,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE approval_requests (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 18, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 17);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('approval_requests')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'policy_decisions'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v19_upgrade_preserves_v18_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [62; 32];
            let worktree_id = [63; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..18],
                18,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TABLE command_allowlist_revisions (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 19, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 18);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('command_allowlist_revisions')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'command_allowlist_entries'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_bootstrap_rolls_back_schema_history_and_version()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            connection
                .execute(
                    "CREATE TABLE worktree_storage_identity (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &[1; 32], &[2; 32]).await;

            assert!(matches!(result, Err(MigrationError::Apply { .. })));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 0);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('worktree_storage_identity') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn catalog_upgrades_from_every_supported_predecessor() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            for predecessor in 1..CatalogSchemaVersion::CURRENT.get() {
                let database = libsql::Builder::new_local(":memory:").build().await?;
                let connection = database.connect()?;
                migrate(
                    &connection,
                    &CATALOG_MIGRATIONS[..predecessor as usize],
                    predecessor,
                    CATALOG_MIGRATION_CHECKSUM_DOMAIN,
                )
                .await?;
                assert_eq!(
                    query_i64(&connection, "PRAGMA user_version").await?,
                    i64::from(predecessor)
                );

                let version = super::migrate_catalog(&connection).await?;
                assert_eq!(version, CatalogSchemaVersion::CURRENT);
                assert_eq!(
                    query_i64(
                        &connection,
                        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (\n\
                         'projects', 'repository_observations', 'recent_worktrees',\n\
                         'worktree_reconciliations'\n\
                         )",
                    )
                    .await?,
                    4
                );
                assert_eq!(
                    query_i64(&connection, "SELECT COUNT(*) FROM pragma_foreign_key_check").await?,
                    0
                );
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn catalog_v5_preserves_ollama_profiles_and_classifies_only_the_canonical_gemini_origin()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            migrate(
                &connection,
                &CATALOG_MIGRATIONS[..4],
                4,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute_batch(
                    "INSERT INTO desktop_settings_revisions VALUES
                       (1, 'ollama', 'http://127.0.0.1:11434', 'local_loopback', 'not_checked', NULL),
                       (2, 'gemini', 'https://generativelanguage.googleapis.com', 'remote', 'remote_blocked', NULL),
                       (3, 'gemini', 'https://gateway.example.test', 'remote', 'remote_blocked', NULL);
                     INSERT INTO desktop_llm_profiles VALUES
                       (1, 'coding', 'ollama', 'coder', 16384, 2048, 1, 0, 1000,
                        'repeat_in_prompt', 'verified', 'disabled', 100);
                     INSERT INTO desktop_embedding_profiles VALUES
                       (1, 'ollama', 'embedder', 768, 8, 101);",
                )
                .await?;

            let version = super::migrate_catalog(&connection).await?;
            assert_eq!(version, CatalogSchemaVersion::CURRENT);
            assert_eq!(
                query_string(
                    &connection,
                    "SELECT endpoint_access || '|' || credential_requirement || '|' ||
                       credential_state || '|' || health_status
                     FROM desktop_settings_revisions WHERE revision = 1",
                )
                .await?,
                "local|none|not_required|not_checked"
            );
            assert_eq!(
                query_string(
                    &connection,
                    "SELECT endpoint_access || '|' || credential_requirement || '|' ||
                       credential_state || '|' || health_status
                     FROM desktop_settings_revisions WHERE revision = 2",
                )
                .await?,
                "explicit_user_initiated_remote|api_key|missing|not_checked"
            );
            assert_eq!(
                query_string(
                    &connection,
                    "SELECT endpoint_access || '|' || credential_requirement || '|' ||
                       credential_state || '|' || health_status
                     FROM desktop_settings_revisions WHERE revision = 3",
                )
                .await?,
                "remote_blocked|none|not_required|remote_blocked"
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM desktop_llm_profiles").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM desktop_embedding_profiles"
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM pragma_foreign_key_check").await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn catalog_v6_indexes_existing_safe_project_displays() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            migrate(
                &connection,
                &CATALOG_MIGRATIONS[..5],
                5,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute_batch(
                    "INSERT INTO projects VALUES (x'0101010101010101010101010101010101010101010101010101010101010101', 1, 1);
                     INSERT INTO repository_observations VALUES (
                       x'0202020202020202020202020202020202020202020202020202020202020202',
                       x'0101010101010101010101010101010101010101010101010101010101010101',
                       x'2f7265706f', 'utf8-lossy-v1', NULL, 1, 1
                     );
                     INSERT INTO recent_worktrees VALUES (
                       x'0303030303030303030303030303030303030303030303030303030303030303',
                       x'0101010101010101010101010101010101010101010101010101010101010101',
                       x'0202020202020202020202020202020202020202020202020202020202020202',
                       NULL, x'2f6c6567616379', 'utf8-lossy-v1', '/legacy-dashboard', 'unborn',
                       NULL, 'refs/heads/main', 1
                     );",
                )
                .await?;

            assert_eq!(
                super::migrate_catalog(&connection).await?,
                CatalogSchemaVersion::CURRENT
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM project_catalog_fts
                     WHERE project_catalog_fts MATCH 'legacy'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_catalog_v3_upgrade_preserves_v2_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            migrate(
                &connection,
                &CATALOG_MIGRATIONS[..2],
                2,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "INSERT INTO projects (
                       project_id, repository_id, repository_common_directory,
                       repository_path_encoding, main_remote_id, created_open_sequence,
                       last_open_sequence
                     ) VALUES (?1, ?2, ?3, 'utf8-lossy-v1', NULL, 1, 1)",
                    params![vec![21; 32], vec![22; 32], b"repository".to_vec()],
                )
                .await?;
            connection
                .execute(
                    "INSERT INTO recent_worktrees (
                       worktree_id, project_id, repository_id, worktree_root,
                       worktree_path_encoding, worktree_root_display, head_kind,
                       head_object_id, head_reference, last_open_sequence
                     ) VALUES (
                       ?1, ?2, ?3, ?4, 'utf8-lossy-v1', 'repository', 'unborn',
                       NULL, 'refs/heads/main', 1
                     )",
                    params![
                        vec![23; 32],
                        vec![21; 32],
                        vec![22; 32],
                        b"repository".to_vec()
                    ],
                )
                .await?;
            connection
                .execute(
                    "CREATE TABLE worktree_reconciliations (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_catalog(&connection).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 3, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 2);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM projects").await?,
                1
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM recent_worktrees").await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'projects_v3'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('worktree_reconciliations') WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v20_upgrade_preserves_v19_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [21; 32];
            let worktree_id = [22; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..19],
                19,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TABLE task_step_acceptance_criteria (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 20, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 19);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                19
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('acceptance_criteria')
                     WHERE name = 'requirement'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('task_step_acceptance_criteria')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'verification_evidence'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM worktrees").await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v21_upgrade_preserves_v20_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [23; 32];
            let worktree_id = [24; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..20],
                20,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "ALTER TABLE run_events ADD COLUMN turn_action_kind_v2 INTEGER",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 21, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 20);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                20
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('run_events')
                     WHERE name = 'turn_action_kind_v2' AND type = 'INTEGER'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name = 'run_events_turn_action_v2_insert_guard'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v22_upgrade_preserves_v21_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [25; 32];
            let worktree_id = [26; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..21],
                21,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE mutation_attempts (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 22, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 21);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                21
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('mutation_attempts')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name = 'mutation_attempts_insert_guard'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v23_upgrade_preserves_v22_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [27; 32];
            let worktree_id = [28; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..22],
                22,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE index_file_analyses (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 23, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 22);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                22
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('index_file_analyses')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'index_parse_diagnostics'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_catalog_v7_upgrade_preserves_v6_schema() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            migrate(
                &connection,
                &CATALOG_MIGRATIONS[..6],
                6,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TABLE ui_preference_revisions (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_catalog(&connection).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 7, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 6);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('ui_preference_revisions')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'
                     AND name = 'ui_preference_revisions_update_guard'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v24_upgrade_preserves_v23_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [71; 32];
            let worktree_id = [72; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..23],
                23,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE agent_session_entries (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 24, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 23);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                23
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'agent_session_revisions'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('agent_session_entries')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v25_upgrade_preserves_v24_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [73; 32];
            let worktree_id = [74; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..24],
                24,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute("CREATE TABLE deep_map_steps (conflict INTEGER)", ())
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 25, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 24);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                24
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
                     AND name = 'deep_map_runs'",
                )
                .await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('deep_map_steps')
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_v26_repairs_legacy_card_search_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [75; 32];
            let worktree_id = [76; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..25],
                25,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            seed_legacy_card_projection(&connection, worktree_id).await?;

            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM card_fts").await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT card_count FROM lexical_search_projections",
                )
                .await?,
                0
            );

            let version =
                super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM card_fts").await?,
                1
            );
            assert_eq!(
                query_string(&connection, "SELECT title FROM card_fts").await?,
                "Legacy title"
            );
            assert_eq!(
                query_string(&connection, "SELECT purpose FROM card_fts").await?,
                "Legacy purpose"
            );
            assert_eq!(
                query_string(&connection, "SELECT body FROM card_fts").await?,
                "title: Legacy title\npurpose: Legacy purpose"
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT card_count FROM lexical_search_projections",
                )
                .await?,
                1
            );

            super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM card_fts").await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v26_repair_rolls_back_fts_and_marker()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [81; 32];
            let worktree_id = [82; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..25],
                25,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            seed_legacy_card_projection(&connection, worktree_id).await?;
            connection
                .execute(
                    "CREATE TRIGGER block_legacy_card_projection_repair\n\
                     BEFORE UPDATE ON lexical_search_projections BEGIN\n\
                       SELECT RAISE(ABORT, 'injected repair failure');\n\
                     END",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 26, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 25);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                25
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM card_fts").await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT card_count FROM lexical_search_projections",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_v27_repairs_projection_removed_after_v26() -> Result<(), Box<dyn std::error::Error>>
    {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [83; 32];
            let worktree_id = [84; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..26],
                26,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            seed_legacy_card_projection(&connection, worktree_id).await?;
            connection
                .execute(
                    "INSERT INTO module_card_field_values (\n\
                     source_index_run_id, card_id, field_kind, value_index, field_value\n\
                     ) VALUES (?1, ?2, 'purpose', 1, 'Second purpose line')",
                    params![vec![92; 32], vec![93; 32]],
                )
                .await?;

            let version =
                super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 28);
            assert_eq!(
                query_string(&connection, "SELECT purpose FROM card_fts").await?,
                "Legacy purpose\nSecond purpose line"
            );
            assert_eq!(
                query_string(&connection, "SELECT body FROM card_fts").await?,
                "title: Legacy title\npurpose: Legacy purpose\nSecond purpose line"
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT card_count FROM lexical_search_projections",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v27_repair_rolls_back_fts_and_marker()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [85; 32];
            let worktree_id = [86; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..26],
                26,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            seed_legacy_card_projection(&connection, worktree_id).await?;
            connection
                .execute(
                    "CREATE TRIGGER block_recurrent_card_projection_repair\n\
                     BEFORE UPDATE ON lexical_search_projections BEGIN\n\
                       SELECT RAISE(ABORT, 'injected recurrent repair failure');\n\
                     END",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 27, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 26);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                26
            );
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM card_fts").await?,
                0
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT card_count FROM lexical_search_projections",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_v28_backfills_the_durable_index_run_high_water_mark()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [95; 32];
            let worktree_id = [96; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..27],
                27,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "INSERT INTO snapshots (\n\
                     snapshot_id, worktree_id, parent_snapshot_id, generation, head_kind,\n\
                     head_object_id, head_reference, index_schema_version\n\
                     ) VALUES (?1, ?2, NULL, 1, 'unborn', NULL, 'refs/heads/main', 1)",
                    params![vec![97; 32], worktree_id.to_vec()],
                )
                .await?;
            connection
                .execute(
                    "INSERT INTO index_runs (\n\
                     index_run_id, worktree_id, snapshot_id, run_sequence,\n\
                     ranking_policy_version, status\n\
                     ) VALUES (?1, ?2, ?3, 7, 1, 'failed')",
                    params![vec![98; 32], worktree_id.to_vec(), vec![97; 32]],
                )
                .await?;

            let version =
                super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;

            assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 28);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT last_sequence FROM index_run_sequence_cursors",
                )
                .await?,
                7
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v28_upgrade_preserves_the_v27_database()
    -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let repository_id = [99; 32];
            let worktree_id = [100; 32];
            super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
            migrate(
                &connection,
                &KNOWLEDGE_MIGRATIONS[..27],
                27,
                super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await?;
            connection
                .execute(
                    "CREATE TABLE index_run_sequence_cursors (conflict INTEGER)",
                    (),
                )
                .await?;

            let result = super::migrate_knowledge(&connection, &repository_id, &worktree_id).await;

            assert!(matches!(
                result,
                Err(MigrationError::Apply { version: 28, .. })
            ));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 27);
            assert_eq!(
                query_i64(&connection, "SELECT COUNT(*) FROM schema_migrations").await?,
                27
            );
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM pragma_table_info('index_run_sequence_cursors')\n\
                     WHERE name = 'conflict'",
                )
                .await?,
                1
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    async fn seed_legacy_card_projection(
        connection: &libsql::Connection,
        worktree_id: [u8; 32],
    ) -> Result<(), libsql::Error> {
        let snapshot_id = [91; 32];
        let index_run_id = [92; 32];
        let card_id = [93; 32];
        connection
            .execute(
                "INSERT INTO snapshots (\n\
                 snapshot_id, worktree_id, parent_snapshot_id, generation, head_kind,\n\
                 head_object_id, head_reference, index_schema_version\n\
                 ) VALUES (?1, ?2, NULL, 1, 'unborn', NULL, 'refs/heads/main', 1)",
                params![snapshot_id.to_vec(), worktree_id.to_vec()],
            )
            .await?;
        connection
            .execute(
                "INSERT INTO index_runs (\n\
                 index_run_id, worktree_id, snapshot_id, run_sequence, ranking_policy_version, status\n\
                 ) VALUES (?1, ?2, ?3, 1, 1, 'published')",
                params![
                    index_run_id.to_vec(),
                    worktree_id.to_vec(),
                    snapshot_id.to_vec()
                ],
            )
            .await?;
        connection
            .execute(
                "INSERT INTO lexical_search_projections (\n\
                 index_run_id, projection_version, symbol_count, path_count, card_count\n\
                 ) VALUES (?1, 1, 0, 0, 0)",
                [index_run_id.to_vec()],
            )
            .await?;
        connection
            .execute(
                "INSERT INTO module_cards (\n\
                 source_index_run_id, snapshot_id, card_id, module_id, card_schema_version,\n\
                 mapper_profile_version, confidence, status\n\
                 ) VALUES (?1, ?2, ?3, ?4, 1, 1, 9000, 'published')",
                params![
                    index_run_id.to_vec(),
                    snapshot_id.to_vec(),
                    card_id.to_vec(),
                    vec![94; 32]
                ],
            )
            .await?;
        connection
            .execute(
                "INSERT INTO module_card_lifecycle (source_index_run_id, card_id, status)\n\
                 VALUES (?1, ?2, 'published')",
                params![index_run_id.to_vec(), card_id.to_vec()],
            )
            .await?;
        for (field_kind, field_value) in [("title", "Legacy title"), ("purpose", "Legacy purpose")]
        {
            connection
                .execute(
                    "INSERT INTO module_card_fields\n\
                     (source_index_run_id, card_id, field_kind) VALUES (?1, ?2, ?3)",
                    params![index_run_id.to_vec(), card_id.to_vec(), field_kind],
                )
                .await?;
            connection
                .execute(
                    "INSERT INTO module_card_field_values (\n\
                     source_index_run_id, card_id, field_kind, value_index, field_value\n\
                     ) VALUES (?1, ?2, ?3, 0, ?4)",
                    params![
                        index_run_id.to_vec(),
                        card_id.to_vec(),
                        field_kind,
                        field_value
                    ],
                )
                .await?;
        }
        Ok(())
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() -> Result<(), Box<dyn std::error::Error>> {
        crate::run_native_libsql_test(async {
            let database = libsql::Builder::new_local(":memory:").build().await?;
            let connection = database.connect()?;
            let migrations = [Migration {
                version: 1,
                name: "broken",
                sql: "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT, checksum BLOB);\n\
                      CREATE TABLE must_rollback (id INTEGER);\n\
                      THIS IS NOT SQL;",
            }];

            let result = migrate(
                &connection,
                &migrations,
                1,
                CATALOG_MIGRATION_CHECKSUM_DOMAIN,
            )
            .await;

            assert!(matches!(result, Err(MigrationError::Apply { .. })));
            assert_eq!(query_i64(&connection, "PRAGMA user_version").await?, 0);
            assert_eq!(
                query_i64(
                    &connection,
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = 'must_rollback'",
                )
                .await?,
                0
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }
}
