ALTER TABLE goal_criterion_origins ADD COLUMN python_integer_translation BLOB
    CHECK (python_integer_translation IS NULL OR
        (requirement_kind = 'existing_test' AND length(python_integer_translation) BETWEEN 51 AND 132143));

ALTER TABLE goal_intake_review_findings ADD COLUMN python_integer_translation BLOB
    CHECK (python_integer_translation IS NULL OR
        (kind = 'outcome' AND evidence_kind IS 'existing_test'
            AND length(python_integer_translation) BETWEEN 51 AND 132143));

ALTER TABLE verification_specs_v1 ADD COLUMN python_integer_translation BLOB
    CHECK (python_integer_translation IS NULL OR
        (target_kind = 'test' AND length(python_integer_translation) BETWEEN 51 AND 132143));

ALTER TABLE verification_evidence ADD COLUMN python_integer_translation_witness BLOB
    CHECK (python_integer_translation_witness IS NULL OR
        (evidence_kind = 'test' AND length(python_integer_translation_witness) BETWEEN 131 AND 132223));
