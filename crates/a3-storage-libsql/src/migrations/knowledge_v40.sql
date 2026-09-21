ALTER TABLE verification_spec_paths RENAME TO verification_spec_paths_v39;
ALTER TABLE verification_specs_v1 RENAME TO verification_specs_v39;

CREATE TABLE verification_specs_v1 (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),
  target_kind TEXT NOT NULL CHECK (target_kind IN
    ('command', 'test', 'diff_invariant', 'diagnostic', 'user_confirm', 'deferred_command')),
  command_id BLOB CHECK (command_id IS NULL OR length(command_id) = 32),
  verification_scope TEXT CHECK (verification_scope IS NULL OR
    verification_scope IN ('targeted', 'package', 'workspace')),
  test_selector_kind TEXT CHECK (test_selector_kind IS NULL OR
    test_selector_kind IN ('all', 'exact')),
  test_selector TEXT CHECK (test_selector IS NULL OR
    length(CAST(test_selector AS BLOB)) BETWEEN 1 AND 1024),
  minimum_test_cases INTEGER CHECK (minimum_test_cases IS NULL OR
    minimum_test_cases BETWEEN 1 AND 1000000),
  diff_mode TEXT CHECK (diff_mode IS NULL OR
    diff_mode IN ('no_changes', 'only_paths', 'exact_paths', 'non_empty_changes')),
  diagnostic_policy TEXT CHECK (diagnostic_policy IS NULL OR
    diagnostic_policy IN ('no_errors', 'no_warnings')),
  confirmation_scope_id BLOB CHECK (confirmation_scope_id IS NULL OR
    length(confirmation_scope_id) = 32),
  CHECK ((target_kind = 'command' AND command_id IS NOT NULL
      AND verification_scope IS NOT NULL AND test_selector_kind IS NULL
      AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL
      AND diagnostic_policy IS NULL AND confirmation_scope_id IS NULL) OR
    (target_kind = 'test' AND command_id IS NOT NULL AND verification_scope IS NOT NULL
      AND test_selector_kind IS NOT NULL
      AND ((test_selector_kind = 'all' AND test_selector IS NULL) OR
        (test_selector_kind = 'exact' AND test_selector IS NOT NULL))
      AND minimum_test_cases IS NOT NULL AND diff_mode IS NULL
      AND diagnostic_policy IS NULL AND confirmation_scope_id IS NULL) OR
    (target_kind = 'diff_invariant' AND command_id IS NULL
      AND verification_scope IS NULL AND test_selector_kind IS NULL
      AND test_selector IS NULL AND minimum_test_cases IS NULL
      AND diff_mode IS NOT NULL AND diagnostic_policy IS NULL
      AND confirmation_scope_id IS NULL) OR
    (target_kind = 'diagnostic' AND command_id IS NOT NULL
      AND verification_scope IS NOT NULL AND test_selector_kind IS NULL
      AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL
      AND diagnostic_policy IS NOT NULL AND confirmation_scope_id IS NULL) OR
    (target_kind = 'user_confirm' AND command_id IS NULL
      AND verification_scope IS NULL AND test_selector_kind IS NULL
      AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL
      AND diagnostic_policy IS NULL AND confirmation_scope_id IS NOT NULL) OR
    (target_kind = 'deferred_command' AND command_id IS NULL
      AND verification_scope IS NOT NULL AND test_selector_kind IS NULL
      AND test_selector IS NULL AND minimum_test_cases IS NULL AND diff_mode IS NULL
      AND diagnostic_policy IS NULL AND confirmation_scope_id IS NULL)),
  PRIMARY KEY (task_id, verification_spec_id),
  FOREIGN KEY (task_id, verification_spec_id)
    REFERENCES task_steps(task_id, verification_spec_id)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

CREATE TABLE verification_spec_paths (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),
  item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 64),
  repository_path BLOB NOT NULL CHECK (length(repository_path) BETWEEN 1 AND 131072),
  PRIMARY KEY (task_id, verification_spec_id, item_sequence),
  UNIQUE (task_id, verification_spec_id, repository_path),
  FOREIGN KEY (task_id, verification_spec_id)
    REFERENCES verification_specs_v1(task_id, verification_spec_id)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

CREATE TABLE verification_spec_deferred_kinds (
  task_id BLOB NOT NULL CHECK (length(task_id) = 32),
  verification_spec_id BLOB NOT NULL CHECK (length(verification_spec_id) = 32),
  item_sequence INTEGER NOT NULL CHECK (item_sequence BETWEEN 1 AND 4),
  command_kind TEXT NOT NULL CHECK (command_kind IN ('test', 'build', 'lint', 'format')),
  PRIMARY KEY (task_id, verification_spec_id, item_sequence),
  UNIQUE (task_id, verification_spec_id, command_kind),
  FOREIGN KEY (task_id, verification_spec_id)
    REFERENCES verification_specs_v1(task_id, verification_spec_id)
    ON UPDATE RESTRICT ON DELETE CASCADE
) STRICT;

INSERT INTO verification_specs_v1 (
  task_id, verification_spec_id, target_kind, command_id, verification_scope,
  test_selector_kind, test_selector, minimum_test_cases, diff_mode, diagnostic_policy,
  confirmation_scope_id
)
SELECT task_id, verification_spec_id, target_kind, command_id, verification_scope,
  test_selector_kind, test_selector, minimum_test_cases, diff_mode, diagnostic_policy,
  confirmation_scope_id
FROM verification_specs_v39;

INSERT INTO verification_spec_paths (
  task_id, verification_spec_id, item_sequence, repository_path
)
SELECT task_id, verification_spec_id, item_sequence, repository_path
FROM verification_spec_paths_v39;

DROP TABLE verification_spec_paths_v39;
DROP TABLE verification_specs_v39;
