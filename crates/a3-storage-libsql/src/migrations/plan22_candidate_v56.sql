-- Candidate-only typed extensions. Existing tags remain compatibility projections.
ALTER TABLE goal_criterion_origins ADD COLUMN loopback_http_recipe BLOB
    CHECK (loopback_http_recipe IS NULL OR
        (requirement_kind = 'runtime_needs_recipe' AND length(loopback_http_recipe) BETWEEN 52 AND 133505));

ALTER TABLE goal_intake_review_findings ADD COLUMN loopback_http_recipe BLOB
    CHECK (loopback_http_recipe IS NULL OR
        (kind = 'outcome' AND evidence_kind IS 'runtime_needs_recipe'
            AND length(loopback_http_recipe) BETWEEN 52 AND 133505));

ALTER TABLE verification_specs_v1 ADD COLUMN loopback_http_recipe BLOB
    CHECK (loopback_http_recipe IS NULL OR
        (target_kind = 'deferred_command' AND verification_scope IS 'targeted'
            AND length(loopback_http_recipe) BETWEEN 52 AND 133505));

ALTER TABLE verification_evidence ADD COLUMN loopback_http_evidence BLOB
    CHECK (loopback_http_evidence IS NULL OR
        (evidence_kind = 'command' AND length(loopback_http_evidence) BETWEEN 321 AND 131887));
