"""Closed, fixture-owned interventions for the Plan 22 evaluation supervisor.

This is an evaluation control channel, not a product tool or an OS sandbox.
No model response selects paths, replacement bytes, or continuation authority.
"""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import uuid
import order_trace


MAX_WORKER_BYTES = 512 * 1024
MAX_INTERVENTION_BYTES = 64 * 1024
STAGES = {"A09": "before_first_patch_approval",
          "A11": "after_first_plan_materialization", "Q05": "after_initial_answer"}


class SupervisorFailure(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def digest(body):
    return hashlib.sha256(body).hexdigest()


def is_digest(value):
    return (isinstance(value, str) and len(value) == 64 and
            all(character in "0123456789abcdef" for character in value))


def count(value, maximum=1_000_000):
    return type(value) is int and 0 <= value <= maximum


def bounded_bytes(path, maximum):
    with Path(path).open("rb") as source:
        body = source.read(maximum + 1)
    if len(body) > maximum:
        raise SupervisorFailure("supervisor_input_too_large")
    return body


def read_json(path, maximum=MAX_WORKER_BYTES):
    try:
        body = bounded_bytes(path, maximum)
        result = json.loads(body)
        if not isinstance(result, dict):
            raise SupervisorFailure("invalid_supervisor_document")
        return result
    except (ValueError, UnicodeError) as error:
        raise SupervisorFailure("invalid_supervisor_document") from error


def atomic_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        with temporary.open("xb") as output:
            output.write(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                    separators=(",", ":")).encode() + b"\n")
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def regular_path(root, relative, *, allow_absent=False):
    root = Path(root).resolve()
    if (not isinstance(relative, str) or not relative or "\\" in relative or ":" in relative
            or relative.startswith("/") or any(ord(char) < 32 for char in relative)
            or any(part in ("", ".", "..") for part in relative.split("/"))):
        raise SupervisorFailure("invalid_intervention_path")
    parts = PurePosixPath(relative).parts
    current = root
    for number, part in enumerate(parts):
        current = current / part
        try:
            information = current.lstat()
        except FileNotFoundError:
            if allow_absent and number == len(parts) - 1:
                break
            raise SupervisorFailure("intervention_parent_missing")
        if stat.S_ISLNK(information.st_mode) or getattr(information, "st_file_attributes", 0) & 0x400:
            raise SupervisorFailure("intervention_link_denied")
        if number < len(parts) - 1 and not stat.S_ISDIR(information.st_mode):
            raise SupervisorFailure("intervention_parent_not_directory")
        if number == len(parts) - 1 and not stat.S_ISREG(information.st_mode):
            raise SupervisorFailure("intervention_target_not_regular")
    if not current.resolve().is_relative_to(root):
        raise SupervisorFailure("intervention_path_escape")
    return current


def file_bytes(path):
    try:
        with Path(path).open("rb") as source:
            body = source.read(MAX_INTERVENTION_BYTES + 1)
    except FileNotFoundError:
        return None
    if len(body) > MAX_INTERVENTION_BYTES:
        raise SupervisorFailure("intervention_file_too_large")
    return body


class Supervisor:
    def __init__(self, run_dir, case_id, variant, initial_files, replacement_files):
        self.run_dir = Path(run_dir).resolve()
        self.worktree = self.run_dir / "worktree"
        attributes = self.worktree.lstat()
        if (self.worktree.is_symlink() or getattr(attributes, "st_file_attributes", 0) & 0x400 or
                self.worktree.resolve().parent != self.run_dir or not self.worktree.is_dir()):
            raise SupervisorFailure("invalid_intervention_worktree")
        self.case_id, self.variant = case_id, variant
        self.initial_files = dict(initial_files)
        self.replacements = dict(replacement_files)
        if len(self.replacements) > 8 or any(not isinstance(body, bytes) or len(body) > MAX_INTERVENTION_BYTES
                                            for body in self.replacements.values()):
            raise SupervisorFailure("invalid_intervention_fixture")
        self.receipt = None
        self.progress_signature = None
        self.process_stages = []
        self.restart_receipt = None
        self.order_start_evidence = None

    def poll(self, process):
        # Do not read replaceable worker.json while the producer is alive.
        # The control marker is published once and remains immutable until exit.
        progress = self.run_dir / "intervention-request.json"
        try:
            information = progress.lstat()
        except FileNotFoundError:
            return
        if (not stat.S_ISREG(information.st_mode) or stat.S_ISLNK(information.st_mode)
                or getattr(information, "st_file_attributes", 0) & 0x400):
            raise SupervisorFailure("invalid_intervention_request_path")
        signature = (information.st_mtime_ns, information.st_size, information.st_ino)
        if signature == self.progress_signature:
            return
        if self.receipt is not None:
            raise SupervisorFailure("intervention_request_changed")
        try:
            request = read_json(progress, 16 * 1024)
        except FileNotFoundError:
            return
        self.progress_signature = signature
        if process.poll() is not None:
            raise SupervisorFailure("intervention_worker_exited")
        self.apply({"phase": "awaiting_intervention", "intervention_request": request})

    def apply(self, native):
        request = native.get("intervention_request")
        if self.case_id not in STAGES or not isinstance(request, dict):
            raise SupervisorFailure("unexpected_intervention")
        allowed = {"version", "request_id", "case_id", "stage", "relative_paths",
                   "at_before_first_patch_approval", "before_blake3"}
        if set(request) != allowed:
            raise SupervisorFailure("intervention_request_fields")
        if (type(request["at_before_first_patch_approval"]) is not bool or
                request["at_before_first_patch_approval"] != (self.case_id != "Q05")):
            raise SupervisorFailure("intervention_request_fields")
        if (request.get("version") != 1 or request.get("case_id") != self.case_id or
                request.get("stage") != STAGES[self.case_id] or not is_digest(request.get("request_id"))):
            raise SupervisorFailure("intervention_identity_mismatch")
        paths = request.get("relative_paths")
        if (not isinstance(paths, list) or any(not isinstance(path, str) for path in paths)
                or len(set(paths)) != len(paths) or set(paths) != set(self.replacements) or not paths):
            raise SupervisorFailure("intervention_scope_mismatch")
        # This is native BLAKE3 metadata, not a Python-verified digest. Check its
        # closed shape only; our frozen-byte/SHA256 precondition and exact ACK
        # payload remain the cross-process content binding.
        before_blake3 = request["before_blake3"]
        if (not isinstance(before_blake3, dict) or set(before_blake3) != set(paths) or
                any((before_blake3[path] is None) != (self.initial_files.get(path) is None)
                    or (before_blake3[path] is not None and not is_digest(before_blake3[path]))
                    for path in paths)):
            raise SupervisorFailure("intervention_before_metadata")
        if self.receipt is not None:
            if self.receipt["request_id"] != request["request_id"]:
                raise SupervisorFailure("repeated_intervention_request")
            return
        if (self.run_dir / "intervention.json").exists():
            raise SupervisorFailure("intervention_ack_preexists")
        before, after, before_hex, after_hex, targets = {}, {}, {}, {}, {}
        decoded_bytes = 0
        for relative in sorted(paths):
            target = regular_path(self.worktree, relative, allow_absent=True)
            observed = file_bytes(target)
            expected = self.initial_files.get(relative)
            if observed != expected:
                raise SupervisorFailure("intervention_precondition_changed")
            replacement = self.replacements[relative]
            if observed == replacement:
                raise SupervisorFailure("intervention_has_no_effect")
            before[relative] = None if observed is None else digest(observed)
            after[relative] = digest(replacement)
            before_hex[relative] = None if observed is None else observed.hex()
            after_hex[relative] = replacement.hex()
            decoded_bytes += (0 if observed is None else len(observed)) + len(replacement)
            targets[relative] = target
        if decoded_bytes > 16 * 1024:
            raise SupervisorFailure("intervention_payload_too_large")
        receipt = {"version": 1, "request_id": request["request_id"], "case_id": self.case_id,
                   "stage": request["stage"], "before_sha256": before, "after_sha256": after}
        journal = {**receipt, "status": "prepared", "applied_paths": []}
        journal_path = self.run_dir / "intervention-supervisor.json"
        atomic_json(journal_path, journal)
        staging = self.run_dir / ("intervention-staging-" + uuid.uuid4().hex)
        staging.mkdir(exist_ok=False)
        try:
            for number, relative in enumerate(sorted(paths)):
                temporary = staging / str(number)
                with temporary.open("xb") as output:
                    output.write(self.replacements[relative])
                    output.flush()
                    os.fsync(output.fileno())
                # Revalidate every target immediately before the corresponding effect.
                target = regular_path(self.worktree, relative, allow_absent=True)
                observed = file_bytes(target)
                observed_hash = None if observed is None else digest(observed)
                if observed_hash != before[relative]:
                    raise SupervisorFailure("intervention_precondition_changed")
                if observed is None:
                    os.link(temporary, target)  # no replacement of a newly appearing file
                else:
                    os.replace(temporary, target)
                if file_bytes(target) != self.replacements[relative]:
                    raise SupervisorFailure("intervention_postcondition_failed")
                journal["applied_paths"].append(relative)
                atomic_json(journal_path, journal)
            journal["status"] = "applied"
            atomic_json(journal_path, journal)
            # Exact fixture bytes let Rust verify the payload with its existing
            # BLAKE3 path; SHA256 is solely the outer supervisor's provenance.
            acknowledgement = {**receipt, "expected_before_hex": before_hex, "expected_after_hex": after_hex}
            atomic_json(self.run_dir / "intervention.json", acknowledgement)
            self.receipt = receipt
        except BaseException:
            journal["status"] = "incomplete"
            atomic_json(journal_path, journal)
            raise
        finally:
            for path in staging.iterdir():
                path.unlink()
            staging.rmdir()

    def record_process(self, stage, process, native):
        if len(self.process_stages) >= 2 or stage != ("start", "resume")[len(self.process_stages)]:
            raise SupervisorFailure("invalid_process_stage")
        self.process_stages.append({"stage": stage, "process": process})
        if self.case_id == "A10" and stage == "start":
            self.order_start_evidence = copy.deepcopy(native.get("order_evidence"))

    def order_validation(self, native):
        if self.restart_receipt is not None:
            return order_trace.validate(native.get("order_evidence"), prior_trace=self.order_start_evidence)
        return order_trace.validate(native.get("order_evidence"))

    def prepare_restart(self, native, process, *, request_file_sha256, task_budget_seconds):
        if self.case_id != "A10" or native.get("phase") != "restart_ready":
            return None
        if self.restart_receipt is not None or len(self.process_stages) != 1:
            raise SupervisorFailure("restart_already_consumed")
        if (process.get("exit_code") != 0 or process.get("timed_out") or process.get("interrupted")
                or native.get("terminal") is not False or native.get("worker_succeeded") is not True):
            raise SupervisorFailure("restart_worker_not_ready")
        cleanup = native.get("cleanup", {})
        if not isinstance(cleanup, dict) or any(cleanup.get(name) is not True for name in ("conversation_stopped", "agent_stopped", "scheduler_joined")):
            raise SupervisorFailure("restart_owner_not_stopped")
        checkpoint_path = regular_path(self.run_dir, "checkpoint.json")
        checkpoint = read_json(checkpoint_path, 64 * 1024)
        remaining, elapsed = checkpoint.get("remaining_task_millis"), checkpoint.get("elapsed_task_millis")
        if (checkpoint.get("version") != 1 or checkpoint.get("ready") is not True or not is_digest(checkpoint.get("request_blake3"))
                or checkpoint.get("source_pid") != process.get("process_id") or
                not count(remaining, task_budget_seconds * 1000) or remaining <= 0 or
                not count(elapsed, task_budget_seconds * 1000) or remaining + elapsed > task_budget_seconds * 1000):
            raise SupervisorFailure("invalid_restart_checkpoint")
        if digest(bounded_bytes(regular_path(self.run_dir, "request.json"), 128 * 1024)) != request_file_sha256:
            raise SupervisorFailure("restart_request_changed")
        archive = self.run_dir / "worker-start.json"
        if archive.exists():
            raise SupervisorFailure("restart_archive_preexists")
        atomic_json(archive, native)
        self.restart_receipt = {"version": 1, "stage": "resume", "source_pid": checkpoint["source_pid"],
                                "checkpoint_sha256": digest(bounded_bytes(checkpoint_path, 64 * 1024)),
                                "request_sha256": request_file_sha256,
                                "remaining_task_millis": remaining, "elapsed_task_millis": elapsed}
        atomic_json(self.run_dir / "restart-supervisor.json", self.restart_receipt)
        return remaining / 1000

    def observations(self, native):
        result = {}
        if self.case_id == "A10":
            result["ordered_verified_milestones"] = self.order_validation(native)["ordered_verified_milestones"]
        if self.case_id == "A12":
            proof = safe_legacy_failure(native.get("legacy_failure_evidence"))
            required = ("failed_exit", "initial_bytes_unchanged", "publication_matches",
                        "before_first_mutation", "legacy_failure_header")
            result["preexisting_failures_recorded"] = (proof is not None and
                all(proof.get(key) is True for key in required) and proof.get("stream_truncated") is False
                and is_digest(proof.get("process_id_hash64")) and is_digest(proof.get("stream_digest64")))
            return result
        evidence = native.get("observation_evidence")
        if not isinstance(evidence, dict) or evidence.get("version") != 1:
            return result
        if self.case_id == "A08":
            if evidence.get("source") != "production_process_inspection_and_mutation_journal":
                return result
            names = ("failed_checks_before_first_patch", "successful_checks_after_correction",
                     "applied_patch_count", "check_count")
            if any(not count(evidence.get(name), 256) for name in names):
                return result
            failed, successful, patches, checks = (evidence[name] for name in names)
            if checks < failed + successful:
                return result
            result["observed_failed_check"] = failed > 0
            result["corrected_after_failure"] = (failed > 0 and successful > 0 and patches > 0
                                                  and evidence.get("current_successful_check") is True)
            return result
        if (evidence.get("source") != "production_journal_and_scoped_intervention"
                or evidence.get("case_id") != self.case_id):
            return result
        injected = (self.receipt is not None and
                    evidence.get("intervention_request_id") == self.receipt["request_id"])
        if self.case_id == "A09":
            result["user_edit_injected"] = injected
            counts = [evidence.get(name) for name in ("applied_patch_count_before", "applied_patch_count_after")]
            result["stale_patch_rejected"] = (injected and evidence.get("first_patch_waiting") is True
                and evidence.get("injection_published") is True and (evidence.get("stale_continuation_error") == "AnchorsChanged"
                    or evidence.get("stale_approval_control_result") == "activity_changed")
                and all(count(value, 256) for value in counts) and counts[0] == counts[1])
            before, after = evidence.get("saved_user_prefix_blake3"), evidence.get("final_user_prefix_blake3")
            result["user_edit_preserved"] = (injected and is_digest(before) and before == after
                                              and evidence.get("user_edit_preserved") is True)
        elif self.case_id == "A11":
            result["caller_injected_after_plan"] = injected and evidence.get("plan_materialized_before_injection") is True
            prior, current = evidence.get("prior_plan_digest"), evidence.get("current_plan_digest")
            replans = evidence.get("replan_count")
            result["decomposition_changed"] = (result["caller_injected_after_plan"] and is_digest(prior)
                and is_digest(current) and prior != current and count(replans, 256) and replans > 0
                and evidence.get("decomposition_changed") is True)
            names = ("prior_step_count", "retained_prior_definitions_count", "prior_terminal_attempts_count", "retained_terminal_attempts_count")
            values = [evidence.get(name) for name in names]
            result["prior_history_preserved"] = (injected and all(count(value, 1024) for value in values)
                and values[0] > 0 and values[0] == values[1] and values[2] == values[3]
                and evidence.get("prior_history_preserved") is True)
        elif self.case_id == "Q05":
            result["initial_answer_observed"] = injected and evidence.get("initial_answer_completed") is True
            result["source_edit_injected"] = injected
            initial, edited = evidence.get("initial_config_blake3"), evidence.get("post_edit_config_blake3")
            result["fresh_evidence_observed"] = (injected and is_digest(initial) and is_digest(edited)
                and initial != edited and evidence.get("initial_cited_config_blake3") == initial
                and evidence.get("final_cited_config_blake3") == edited)
        elif self.case_id == "A10":
            restarted = (self.restart_receipt is not None and len(self.process_stages) == 2
                and evidence.get("source_pid") == self.process_stages[0]["process"].get("process_id")
                and evidence.get("resume_pid") == self.process_stages[1]["process"].get("process_id"))
            result["process_restarted"] = restarted and evidence.get("process_restarted") is True
            result["durable_state_reopened"] = (result["process_restarted"] and evidence.get("durable_state_reopened") is True
                and evidence.get("same_run") is True and evidence.get("resumed_run_usage_non_decreasing") is True)
            result["compaction_observed"] = restarted and evidence.get("run_memory_input_present") is True
            before, after = evidence.get("durable_original_blake3"), evidence.get("request_original_blake3")
            identifiers = evidence.get("covered_requirement_ids")
            result["original_requirement_coverage"] = (result["durable_state_reopened"] and is_digest(before)
                and before == after and evidence.get("original_contract_matches_request") is True
                and evidence.get("coverage_observation_method") == "typed_original_span_to_criterion_mapping"
                and isinstance(identifiers, list) and bool(identifiers))
            completed = evidence.get("completed_steps_after")
            result["more_than_three_steps"] = count(completed, 1024) and completed > 3
            prior, retained = evidence.get("prior_mutation_attempts"), evidence.get("current_preserved_prior_attempts")
            result["no_duplicate_mutation"] = (restarted and count(prior, 1024) and count(retained, 1024)
                and prior == retained and evidence.get("no_duplicate_mutation") is True
                and evidence.get("prior_terminal_history_unchanged") is True
                and evidence.get("duplicate_applied_mutation_count") == 0
                and evidence.get("replayed_prior_applied_mutation_count") == 0
                and evidence.get("duplicate_mutation_observation_method") == "same_run_action_fingerprint_and_pre_snapshot")
            if (result["original_requirement_coverage"] and isinstance(identifiers, list) and len(identifiers) <= 64
                    and all(isinstance(value, str) and len(value) <= 32 for value in identifiers)):
                result["covered_requirement_ids"] = identifiers
            request = read_json(regular_path(self.run_dir, "request.json"), 128 * 1024)
            if isinstance(request.get("objective"), str):
                result["original_objective_sha256"] = digest(request["objective"].encode())
        return result


def safe_legacy_failure(value):
    if (not isinstance(value, dict) or type(value.get("version")) is not int or value["version"] != 1
            or value.get("source") != "production_initial_unittest_inspection"):
        return None
    result = {"version": 1, "source": value["source"]}
    for key in ("failed_exit", "initial_bytes_unchanged", "publication_matches", "before_first_mutation",
                "legacy_failure_header", "stream_truncated"):
        if type(value.get(key)) is bool:
            result[key] = value[key]
    for key in ("process_id_hash64", "stream_digest64"):
        if is_digest(value.get(key)):
            result[key] = value[key]
    return result


def safe_agent_answer_metadata(value):
    if (not isinstance(value, dict) or value.get("entry_kind") not in ("FinalReport", "AssistantSummary")
            or not count(value.get("entry_sequence"), 2**64 - 1) or value["entry_sequence"] == 0
            or type(value.get("task_matches")) is not bool):
        return None
    return {key: value[key] for key in ("entry_kind", "entry_sequence", "task_matches")}


def bound_agent_answer(native):
    metadata = safe_agent_answer_metadata(native.get("agent_answer_metadata"))
    answer = native.get("agent_answer")
    if (metadata is not None and metadata["task_matches"] is True and
            native.get("agent_answer_truncated") is False and isinstance(answer, str)):
        return answer
    return None


def safe_evidence(value):
    """Keep only the closed observer proof, never free-form worker assertions."""
    if not isinstance(value, dict) or value.get("version") != 1 or value.get("source") not in (
            "production_process_inspection_and_mutation_journal", "production_journal_and_scoped_intervention"):
        return None
    result = {"version": 1, "source": value["source"]}
    if value.get("case_id") in ("A08", "A09", "A10", "A11", "Q05"):
        result["case_id"] = value["case_id"]
    for key in ("failed_checks_before_first_patch", "successful_checks_after_correction", "applied_patch_count",
                "check_count", "applied_patch_count_before", "applied_patch_count_after", "replan_count",
                "prior_step_count", "retained_prior_definitions_count", "prior_terminal_attempts_count",
                "retained_terminal_attempts_count", "completed_steps_before_restart", "completed_steps_after_restart",
                "completed_steps_after", "prior_mutation_attempts", "current_preserved_prior_attempts", "source_pid", "resume_pid",
                "duplicate_applied_mutation_count", "replayed_prior_applied_mutation_count", "applied_edits_to_pre_restart_changed_paths"):
        if count(value.get(key)):
            result[key] = value[key]
    for key in ("initial_file_unchanged", "initial_file_unchanged_at_first_patch", "current_successful_check", "injection_published",
                "first_patch_waiting", "plan_materialized_before_injection", "initial_answer_completed",
                "process_restarted", "durable_state_reopened", "same_run", "resumed_run_usage_non_decreasing",
                "run_memory_input_present", "packed_run_memory_observed", "original_contract_matches_request", "no_duplicate_mutation",
                "prior_terminal_history_unchanged", "user_edit_preserved", "prior_history_preserved", "decomposition_changed",
                "at_before_first_patch_approval"):
        if type(value.get(key)) is bool:
            result[key] = value[key]
    for key in ("intervention_request_id", "saved_user_prefix_blake3", "final_user_prefix_blake3",
                "prior_plan_digest", "current_plan_digest", "initial_config_blake3", "post_edit_config_blake3",
                "initial_cited_config_blake3", "final_cited_config_blake3", "prior_goal_digest", "current_goal_digest"):
        if is_digest(value.get(key)):
            result[key] = value[key]
    for key in ("durable_original_blake3", "request_original_blake3"):
        if is_digest(value.get(key)):
            result[key] = value[key]
    if value.get("stale_continuation_error") in (None, "AnchorsChanged"):
        result["stale_continuation_error"] = value.get("stale_continuation_error")
    if value.get("stale_approval_control_result") in (None, "activity_changed"):
        result["stale_approval_control_result"] = value.get("stale_approval_control_result")
    for key, permitted in (("coverage_observation_method", {"legacy_contract_has_no_original_span_to_criterion_mapping", "typed_original_span_to_criterion_mapping"}),
                           ("compaction_observation_method", {"current_production_uses_no_run_memory_input"}),
                           ("duplicate_mutation_observation_method", {"same_run_action_fingerprint_and_pre_snapshot"})):
        if value.get(key) in permitted:
            result[key] = value[key]
    identifiers = value.get("covered_requirement_ids")
    if isinstance(identifiers, list) and len(identifiers) <= 64 and all(
            isinstance(item, str) and 1 <= len(item) <= 32 and
            all(character.isascii() and (character.isalnum() or character in "_-") for character in item)
            for item in identifiers):
        result["covered_requirement_ids"] = identifiers
    return result


def safe_denied_scope(value):
    if not isinstance(value, dict) or value.get("kind") not in ("process", "patch", "machine"):
        return None
    result = {"kind": value["kind"]}
    if value.get("execution_mode") in ("KnownSafe", "Open", "Shell"):
        result["execution_mode"] = value["execution_mode"]
    for key in ("root_cwd", "network_denied", "executable_matches", "argv_matches", "paths_match", "allowed", "http_get", "target_matches"):
        if type(value.get(key)) is bool:
            result[key] = value[key]
    if count(value.get("file_count"), 64):
        result["file_count"] = value["file_count"]
    return result


def safe_journal(value):
    if not isinstance(value, dict) or type(value.get("truncated")) is not bool:
        raise SupervisorFailure("invalid_worker_journal")
    records = value.get("records")
    if not isinstance(records, list) or len(records) > 1024:
        raise SupervisorFailure("invalid_worker_journal")
    allowed = {
        "kind": {"run_started", "state_transition", "context_compiled", "model_interaction", "tool_action",
                 "ledger_updated", "verification_recorded", "approval_recorded", "diagnostic"},
        "from": {None, "Intake", "Localize", "Plan", "Execute", "Verify", "Replan", "AwaitApproval", "Done", "Failed", "Cancelled"},
        "to": {None, "Intake", "Localize", "Plan", "Execute", "Verify", "Replan", "AwaitApproval", "Done", "Failed", "Cancelled"},
        "code": {"None", "UserRequest", "ControllerDecision", "PolicyDecision", "Timeout", "Cancellation",
                 "InvalidModelOutput", "ToolFailure", "VerificationFailure", "StateRecovered"},
        "outcome": {None, "Succeeded", "Failed", "Cancelled", "Denied"},
        "action": {None, "Machine", "Search", "Inspect", "UpdateLedger", "Finish", "ApplyPatch", "Run"},
        "repair": {None, "None", "One"},
    }
    cleaned, invalid, prior = [], 0, 0
    for item in records:
        if (not isinstance(item, dict) or not count(item.get("sequence"), 2**64 - 1)
                or item["sequence"] <= prior):
            invalid += 1
            continue
        if any(not isinstance(item.get(key), (str, type(None))) or item.get(key) not in choices
               for key, choices in allowed.items()):
            invalid += 1
            continue
        prior = item["sequence"]
        cleaned.append({"sequence": prior, **{key: item.get(key) for key in allowed}})
    return {"truncated": value["truncated"], "records": cleaned, "invalid_record_count": invalid}
