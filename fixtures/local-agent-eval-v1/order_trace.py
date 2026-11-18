"""Closed, content-free A10 delivery-order proof validation; no model judgments."""
from __future__ import annotations

import re

MODULES = ("step_one.py", "step_two.py", "step_three.py", "step_four.py")
SOURCE = "production_approval_mutation_and_process_inspection"
MAX_EVENTS = 64
_NO_PRIOR = object()
TEST_ARGV = ["python", "-B", "-m", "unittest", "discover", "-s", "tests"]
TOP_FIELDS = {"version", "source", "case_id", "task_id", "run_id", "module_paths",
              "initial_blake3", "final_blake3", "events", "truncated", "invalid_reason"}
COMMON_FIELDS = {"seq", "kind", "run_id", "step_id", "tool_run_id", "attempt", "snapshot_id",
                 "approval_request_id", "action_fingerprint", "source_blake3", "actual_bound"}
MUTATION_FIELDS = {"affected_paths", "before_blake3", "after_blake3", "application_state",
                   "pre_snapshot_id", "post_snapshot_id"}
CHECK_FIELDS = {"checked_paths", "argv", "cwd", "exit_code", "process_id_hash64",
                "streams_complete", "protected_bytes_unchanged"}


def _digest(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def _hashes(value):
    return (isinstance(value, dict) and set(value) == set(MODULES)
            and all(_digest(item) for item in value.values()))


def _paths(value):
    return (isinstance(value, list) and bool(value) and len(value) <= len(MODULES)
            and all(isinstance(item, str) and item in MODULES for item in value)
            and len(set(value)) == len(value))


def validate(value, *, prior_trace=_NO_PRIOR):
    """Validate native bindings/chronology and an optional stopped-stage prefix.

    Cryptographic fields are native observer evidence, checked here for identity
    and consistency; Python does not claim to recompute repository BLAKE3 hashes.
    """
    result = {"version": 1, "ordered_verified_milestones": False, "reason": "invalid_trace_shape",
              "event_count": 0, "verified_modules": []}

    def fail(reason):
        result["reason"] = reason
        return result

    if (not isinstance(value, dict) or set(value) != TOP_FIELDS or type(value["version"]) is not int
            or value["version"] != 1 or value["source"] != SOURCE or value["case_id"] != "A10"
            or not _digest(value["task_id"]) or not _digest(value["run_id"])
            or value["module_paths"] != list(MODULES) or not _hashes(value["initial_blake3"])
            or not _hashes(value["final_blake3"]) or type(value["truncated"]) is not bool
            or not isinstance(value["events"], list) or len(value["events"]) > MAX_EVENTS):
        return result
    result["event_count"] = len(value["events"])
    if value["truncated"]:
        return fail("trace_truncated")
    if value["invalid_reason"] is not None:
        return fail("native_observation_invalid")
    if prior_trace is not _NO_PRIOR:
        # The first process has stopped. Its immutable archived trace must survive
        # as an exact chronological prefix in the resumed process, including IDs.
        prior_result = validate(prior_trace)
        if (prior_result["reason"] in ("invalid_trace_shape", "trace_truncated", "native_observation_invalid", "unobserved_final_source_change")
                or any(value[key] != prior_trace[key] for key in
                       ("version", "source", "case_id", "task_id", "run_id", "module_paths", "initial_blake3"))
                or len(prior_trace["events"]) > len(value["events"])
                or value["events"][:len(prior_trace["events"])] != prior_trace["events"]):
            return fail("restart_trace_prefix_changed")
    current = dict(value["initial_blake3"])
    current_snapshot = None
    mutated, passes, seen = set(), {}, set()
    for number, event in enumerate(value["events"], 1):
        if not isinstance(event, dict) or event.get("kind") not in ("mutation", "check"):
            return fail("invalid_event_shape")
        kind = event["kind"]
        fields = COMMON_FIELDS | (MUTATION_FIELDS if kind == "mutation" else CHECK_FIELDS)
        if (set(event) != fields or type(event["seq"]) is not int or event["seq"] != number
                or event["actual_bound"] is not True or event["run_id"] != value["run_id"]
                or any(not _digest(event[key]) for key in
                       ("run_id", "step_id", "tool_run_id", "snapshot_id", "approval_request_id", "action_fingerprint"))
                or type(event["attempt"]) is not int or not 1 <= event["attempt"] <= 2**32 - 1
                or not _hashes(event["source_blake3"])):
            return fail("invalid_event_binding")
        identity = (kind, event["run_id"], event["step_id"], event["tool_run_id"], event["attempt"])
        if identity in seen:
            return fail("duplicate_event_binding")
        seen.add(identity)
        if kind == "mutation":
            if (event["application_state"] not in ("applied", "not_applied", "unknown") or not _paths(event["affected_paths"])
                    or not _hashes(event["before_blake3"]) or not _hashes(event["after_blake3"])
                    or event["before_blake3"] != current or event["source_blake3"] != event["after_blake3"]
                    or not _digest(event["pre_snapshot_id"]) or not _digest(event["post_snapshot_id"])
                    or event["snapshot_id"] != event["pre_snapshot_id"]):
                return fail("invalid_mutation_binding")
            if current_snapshot is not None and event["pre_snapshot_id"] != current_snapshot:
                return fail("snapshot_chain_changed")
            changed = {path for path in MODULES if event["before_blake3"][path] != event["after_blake3"][path]}
            if not changed.issubset(event["affected_paths"]):
                return fail("mutation_has_unbound_source_change")
            if event["application_state"] == "unknown":
                return fail("mutation_result_unknown")
            if event["application_state"] == "not_applied":
                if changed or event["pre_snapshot_id"] != event["post_snapshot_id"]:
                    return fail("non_applied_mutation_changed_source")
                current_snapshot = event["post_snapshot_id"]
                continue
            if not changed:
                current_snapshot = event["post_snapshot_id"]
                continue
            if event["pre_snapshot_id"] == event["post_snapshot_id"]:
                return fail("invalid_mutation_binding")
            for path in changed:
                index = MODULES.index(path)
                if path not in mutated and index > 0:
                    previous = MODULES[index - 1]
                    if previous not in mutated or passes.get(previous) != current[previous]:
                        return fail("next_module_started_before_fresh_pass")
            for path in changed:
                mutated.add(path)
                passes.pop(path, None)
            current = dict(event["after_blake3"])
            current_snapshot = event["post_snapshot_id"]
        else:
            checked = event["checked_paths"]
            if (not _paths(checked) or event["cwd"] != "." or not _digest(event["process_id_hash64"])
                    or type(event["streams_complete"]) is not bool
                    or type(event["protected_bytes_unchanged"]) is not bool
                    or (event["exit_code"] is not None and
                        (type(event["exit_code"]) is not int or not -(2**31) <= event["exit_code"] < 2**31))
                    or event["source_blake3"] != current):
                return fail("invalid_check_binding")
            if current_snapshot is not None and event["snapshot_id"] != current_snapshot:
                return fail("snapshot_chain_changed")
            current_snapshot = event["snapshot_id"]
            argv = event["argv"]
            expected = TEST_ARGV if checked == list(MODULES) else (
                TEST_ARGV + ["-p", "test_" + checked[0][:-3] + ".py"] if len(checked) == 1 else None)
            if expected is None or argv != expected:
                return fail("check_command_not_exact")
            for path in checked:
                passes.pop(path, None)
                if event["exit_code"] == 0 and event["streams_complete"] and event["protected_bytes_unchanged"]:
                    if path in mutated:
                        passes[path] = current[path]
    if current != value["final_blake3"]:
        return fail("unobserved_final_source_change")
    result["verified_modules"] = [path for path in MODULES if path in mutated and passes.get(path) == current[path]]
    if result["verified_modules"] != list(MODULES):
        return fail("missing_fresh_module_checks")
    result["ordered_verified_milestones"] = True
    result["reason"] = "verified"
    return result
