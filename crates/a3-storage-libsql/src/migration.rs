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

const KNOWLEDGE_MIGRATIONS: &[Migration] = &[
    KNOWLEDGE_BOOTSTRAP_MIGRATION,
    KNOWLEDGE_PROJECT_INDEX_MIGRATION,
    KNOWLEDGE_RECONCILIABLE_IDENTITIES_MIGRATION,
    KNOWLEDGE_ATOMIC_INDEX_PUBLICATION_MIGRATION,
    KNOWLEDGE_EXACT_SEARCH_MIGRATION,
    KNOWLEDGE_LEXICAL_SEARCH_MIGRATION,
    KNOWLEDGE_SEMANTIC_EMBEDDING_MIGRATION,
    KNOWLEDGE_MODULE_PROJECTION_MIGRATION,
];

const CATALOG_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.catalog-migration.v1";
const KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN: &[u8] = b"a3.knowledge-migration.v1";

/// Monotone version of the global catalog schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CatalogSchemaVersion(u32);

impl CatalogSchemaVersion {
    /// Current schema version understood by this build.
    pub const CURRENT: Self = Self::new(3);

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
    pub const CURRENT: Self = Self::new(8);

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
        query_i64,
    };
    use futures::executor::block_on;
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
        block_on(async {
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
                     'path_fts', 'card_fts'\n\
                     )",
                )
                .await?,
                20
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn knowledge_upgrades_v1_identity_into_project_repositories()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
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

    #[test]
    fn knowledge_upgrades_from_every_supported_predecessor()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
            for predecessor in 1..KnowledgeSchemaVersion::CURRENT.get() {
                let database = libsql::Builder::new_local(":memory:").build().await?;
                let connection = database.connect()?;
                let repository_id = [predecessor as u8; 32];
                let worktree_id = [(predecessor + 10) as u8; 32];
                super::apply_knowledge_bootstrap(&connection, &repository_id, &worktree_id).await?;
                if predecessor > 1 {
                    migrate(
                        &connection,
                        &KNOWLEDGE_MIGRATIONS[..predecessor as usize],
                        predecessor,
                        super::KNOWLEDGE_MIGRATION_CHECKSUM_DOMAIN,
                    )
                    .await?;
                }
                assert_eq!(
                    query_i64(&connection, "PRAGMA user_version").await?,
                    i64::from(predecessor)
                );

                let version =
                    super::migrate_knowledge(&connection, &repository_id, &worktree_id).await?;
                assert_eq!(version, KnowledgeSchemaVersion::CURRENT);
                assert_eq!(
                    query_i64(&connection, "SELECT COUNT(*) FROM pragma_foreign_key_check").await?,
                    0
                );
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })
    }

    #[test]
    fn failed_knowledge_v2_upgrade_preserves_the_v1_database()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
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
        block_on(async {
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
        block_on(async {
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
        block_on(async {
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
        block_on(async {
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
        block_on(async {
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
        block_on(async {
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
    fn failed_knowledge_bootstrap_rolls_back_schema_history_and_version()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
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
        block_on(async {
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
    fn failed_catalog_v3_upgrade_preserves_v2_data_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
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
    fn failed_migration_rolls_back_schema_and_version() -> Result<(), Box<dyn std::error::Error>> {
        block_on(async {
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
