-- Fresh candidate70 only. Historical v68/v69 schemas remain byte-identical.
CREATE TABLE edit_phase_jobs (
    run_id BLOB NOT NULL CHECK(length(run_id)=32),
    job_digest BLOB NOT NULL CHECK(length(job_digest)=32),
    call_kind TEXT NOT NULL CHECK(call_kind IN ('initial','structural_repair')),
    request_digest BLOB NOT NULL CHECK(length(request_digest)=32),
    generation_cap INTEGER NOT NULL CHECK(generation_cap BETWEEN 1 AND 262144),
    context_cap INTEGER NOT NULL CHECK(context_cap BETWEEN 1024 AND 1048576 AND context_cap>=generation_cap),
    halt_kind TEXT CHECK(halt_kind IN ('definition','caller','test','unknown_need','too_large')),
    PRIMARY KEY(run_id,job_digest,call_kind),
    FOREIGN KEY(run_id,job_digest,call_kind) REFERENCES edit_phase_reservations(run_id,job_digest,call_kind)
) STRICT;
CREATE TRIGGER edit_job_before_io BEFORE INSERT ON edit_phase_jobs
WHEN NEW.halt_kind IS NOT NULL OR NOT EXISTS(SELECT 1 FROM edit_phase_reservations r
 WHERE r.run_id=NEW.run_id AND r.job_digest=NEW.job_digest AND r.call_kind=NEW.call_kind AND r.outcome IS NULL)
BEGIN SELECT RAISE(ABORT,'Edit job must be bound before I/O'); END;
CREATE TRIGGER edit_job_observe_once BEFORE UPDATE ON edit_phase_jobs
WHEN OLD.halt_kind IS NOT NULL OR NEW.run_id!=OLD.run_id OR NEW.job_digest!=OLD.job_digest
 OR NEW.call_kind!=OLD.call_kind OR NEW.request_digest!=OLD.request_digest
 OR NEW.generation_cap!=OLD.generation_cap OR NEW.context_cap!=OLD.context_cap
 OR NOT EXISTS(SELECT 1 FROM edit_phase_reservations r WHERE r.run_id=OLD.run_id
 AND r.job_digest=OLD.job_digest AND r.call_kind=OLD.call_kind AND r.outcome IS NULL)
BEGIN SELECT RAISE(ABORT,'Edit job is immutable'); END;
CREATE TRIGGER edit_job_no_refund BEFORE DELETE ON edit_phase_jobs
BEGIN SELECT RAISE(ABORT,'Edit job cannot be refunded'); END;
