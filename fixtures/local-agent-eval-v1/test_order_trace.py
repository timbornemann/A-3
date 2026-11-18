"""Synthetic observer-contract controls; no model or product performance claim."""
from __future__ import annotations

import copy
from pathlib import Path
import tempfile
import unittest

import order_trace
import cases
import run_baseline as runner
import supervisor


def hex_id(number):
    return f"{number:064x}"


class Trace:
    def __init__(self):
        self.current = {path: hex_id(index + 10) for index, path in enumerate(order_trace.MODULES)}
        self.snapshot = hex_id(100)
        self.value = {"version": 1, "source": order_trace.SOURCE, "case_id": "A10",
                      "task_id": hex_id(1), "run_id": hex_id(2), "module_paths": list(order_trace.MODULES),
                      "initial_blake3": dict(self.current), "final_blake3": dict(self.current),
                      "events": [], "truncated": False, "invalid_reason": None}

    def common(self, kind):
        number = len(self.value["events"]) + 1
        return {"seq": number, "kind": kind, "run_id": hex_id(2), "step_id": hex_id(20 + number),
                "tool_run_id": hex_id(40 + number), "attempt": 1, "snapshot_id": self.snapshot,
                "approval_request_id": hex_id(60 + number), "action_fingerprint": hex_id(80 + number),
                "source_blake3": dict(self.current), "actual_bound": True}

    def mutation(self, paths, *, state="applied", change=True):
        event = self.common("mutation")
        before = dict(self.current)
        previous_snapshot = self.snapshot
        if change:
            for path in paths:
                self.current[path] = hex_id(1000 + len(self.value["events"]) * 10 + order_trace.MODULES.index(path))
            self.snapshot = hex_id(2000 + len(self.value["events"]))
        event.update({"affected_paths": list(paths), "before_blake3": before,
                      "after_blake3": dict(self.current), "source_blake3": dict(self.current),
                      "pre_snapshot_id": previous_snapshot, "post_snapshot_id": self.snapshot,
                      "application_state": state})
        self.value["events"].append(event)
        self.value["final_blake3"] = dict(self.current)
        return event

    def check(self, path=None, *, exit_code=0, streams_complete=True, protected=True):
        event = self.common("check")
        paths = list(order_trace.MODULES) if path is None else [path]
        argv = list(order_trace.TEST_ARGV)
        if path is not None:
            argv += ["-p", "test_" + path[:-3] + ".py"]
        event.update({"checked_paths": paths, "argv": argv, "cwd": ".", "exit_code": exit_code,
                      "process_id_hash64": hex_id(3000 + len(self.value["events"])),
                      "streams_complete": streams_complete, "protected_bytes_unchanged": protected})
        self.value["events"].append(event)
        return event

    def ordered(self):
        for path in order_trace.MODULES:
            self.mutation([path])
            self.check(path)
        return self


class OrderTraceTests(unittest.TestCase):
    def assert_failed(self, trace, reason=None):
        result = order_trace.validate(trace.value)
        self.assertFalse(result["ordered_verified_milestones"])
        if reason is not None:
            self.assertEqual(result["reason"], reason)

    def test_actual_four_individual_edits_then_current_exact_checks_pass(self):
        trace = Trace().ordered()
        result = order_trace.validate(trace.value)
        self.assertTrue(result["ordered_verified_milestones"])
        self.assertEqual(result["verified_modules"], list(order_trace.MODULES))
        self.assertEqual(result["event_count"], 8)

    def test_completed_counts_and_model_flags_are_not_a_trace(self):
        for value in (None, {}, {"completed": 4, "ordered_verified_milestones": True}):
            self.assertFalse(order_trace.validate(value)["ordered_verified_milestones"])

    def test_single_bulk_patch_fails_even_if_all_later_checks_pass(self):
        trace = Trace()
        trace.mutation(order_trace.MODULES)
        trace.check()
        self.assert_failed(trace, "next_module_started_before_fresh_pass")

    def test_no_mutation_or_noop_edits_never_gain_delivery_credit(self):
        for state in ("applied", "not_applied"):
            trace = Trace()
            for path in order_trace.MODULES:
                trace.mutation([path], state=state, change=False)
                trace.check(path)
            self.assert_failed(trace, "missing_fresh_module_checks")
        trace = Trace()
        trace.check()
        trace.ordered()
        self.assertTrue(order_trace.validate(trace.value)["ordered_verified_milestones"])

    def test_wrong_module_or_failed_incomplete_or_changed_test_check_never_unlocks_next(self):
        for kwargs in ({"exit_code": 1}, {"exit_code": None}, {"streams_complete": False}, {"protected": False}):
            trace = Trace()
            trace.mutation([order_trace.MODULES[0]])
            trace.check(order_trace.MODULES[0], **kwargs)
            trace.mutation([order_trace.MODULES[1]])
            self.assert_failed(trace, "next_module_started_before_fresh_pass")
        trace = Trace()
        trace.mutation([order_trace.MODULES[0]])
        trace.check(order_trace.MODULES[1])
        trace.mutation([order_trace.MODULES[1]])
        self.assert_failed(trace, "next_module_started_before_fresh_pass")

    def test_intervening_prior_source_edit_invalidates_old_pass_before_next_first_edit(self):
        trace = Trace()
        first, second = order_trace.MODULES[:2]
        trace.mutation([first])
        trace.check(first)
        trace.mutation([first])
        trace.mutation([second])
        self.assert_failed(trace, "next_module_started_before_fresh_pass")

    def test_final_source_change_or_later_failed_check_invalidates_success(self):
        trace = Trace().ordered()
        trace.mutation([order_trace.MODULES[0]])
        self.assert_failed(trace, "missing_fresh_module_checks")
        trace.check(order_trace.MODULES[0])
        self.assertTrue(order_trace.validate(trace.value)["ordered_verified_milestones"])
        trace.check(order_trace.MODULES[0], exit_code=1)
        self.assert_failed(trace, "missing_fresh_module_checks")
        trace = Trace().ordered()
        trace.value["final_blake3"][order_trace.MODULES[0]] = hex_id(9999)
        self.assert_failed(trace, "unobserved_final_source_change")

    def test_exact_argv_and_checked_pathset_are_bound(self):
        for alteration in ({"argv": ["python", "-c", "print('ok')"]}, {"cwd": "tests"},
                           {"checked_paths": [order_trace.MODULES[1]]}, {"argv": order_trace.TEST_ARGV + ["--help"]}):
            trace = Trace().ordered()
            trace.value["events"][1].update(alteration)
            self.assert_failed(trace)

    def test_source_snapshot_run_step_tool_approval_and_attempt_bindings_are_required(self):
        alterations = ({"run_id": hex_id(9)}, {"step_id": None}, {"tool_run_id": "short"},
                      {"attempt": True}, {"attempt": 0}, {"approval_request_id": None},
                      {"action_fingerprint": "A" * 64}, {"actual_bound": False},
                      {"snapshot_id": hex_id(9999)}, {"seq": 3}, {"unknown": True})
        for alteration in alterations:
            trace = Trace().ordered()
            trace.value["events"][1].update(alteration)
            self.assert_failed(trace)
        trace = Trace().ordered()
        trace.value["events"][2]["before_blake3"][order_trace.MODULES[0]] = hex_id(8888)
        self.assert_failed(trace, "invalid_mutation_binding")

    def test_replayed_tool_attempt_and_unknown_mutation_do_not_qualify(self):
        trace = Trace().ordered()
        trace.value["events"][3].update({key: trace.value["events"][1][key]
            for key in ("run_id", "step_id", "tool_run_id", "attempt")})
        self.assert_failed(trace, "duplicate_event_binding")
        trace = Trace().ordered()
        trace.value["events"][0]["application_state"] = "unknown"
        self.assert_failed(trace, "mutation_result_unknown")

    def test_restart_preserves_exact_original_trace_prefix_and_ids(self):
        trace = Trace()
        trace.mutation([order_trace.MODULES[0]])
        trace.check(order_trace.MODULES[0])
        prior = copy.deepcopy(trace.value)
        for path in order_trace.MODULES[1:]:
            trace.mutation([path])
            trace.check(path)
        self.assertTrue(order_trace.validate(trace.value, prior_trace=prior)["ordered_verified_milestones"])
        for mutate in (lambda v: v["events"][0].update({"approval_request_id": hex_id(9)}),
                       lambda v: v.update({"task_id": hex_id(9)}),
                       lambda v: v["events"].pop(0)):
            modified = copy.deepcopy(trace.value)
            mutate(modified)
            result = order_trace.validate(modified, prior_trace=prior)
            self.assertFalse(result["ordered_verified_milestones"])
            self.assertEqual(result["reason"], "restart_trace_prefix_changed")
        altered_prior = copy.deepcopy(prior)
        altered_prior["final_blake3"][order_trace.MODULES[0]] = hex_id(9999)
        self.assertEqual(order_trace.validate(trace.value, prior_trace=altered_prior)["reason"],
                         "restart_trace_prefix_changed")

    def test_bounds_native_invalidity_and_unknown_top_fields_never_pass(self):
        for alteration in ({"truncated": True}, {"invalid_reason": "ambiguous_actual_attempt"},
                           {"unknown": "untrusted"}, {"events": [{}] * 65}, {"version": True}):
            trace = Trace().ordered()
            trace.value.update(alteration)
            self.assert_failed(trace)


class SupervisorOrderIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="a3-plan22-order-control-")
        run_dir = Path(self.temporary.name)
        cases.materialize("A10", "development", run_dir)
        self.control = supervisor.Supervisor(run_dir, "A10", "development",
            cases.fixture_files("A10", "development"), cases.intervention_files("A10", "development"))

    def tearDown(self):
        self.temporary.cleanup()

    def test_model_flag_is_ignored_and_only_closed_trace_derives_order_observation(self):
        self.assertFalse(self.control.observations({"observations": {"ordered_verified_milestones": True}})
                         ["ordered_verified_milestones"])
        native = {"order_evidence": Trace().ordered().value,
                  "observations": {"ordered_verified_milestones": False}}
        self.assertTrue(self.control.observations(native)["ordered_verified_milestones"])

    def test_stopped_stage_prefix_is_copied_and_required_after_restart(self):
        trace = Trace()
        trace.mutation([order_trace.MODULES[0]])
        trace.check(order_trace.MODULES[0])
        prior = copy.deepcopy(trace.value)
        self.control.record_process("start", {"process_id": 101}, {"order_evidence": prior})
        self.control.restart_receipt = {"source_pid": 101}
        # A later mutation of the caller's native dictionary cannot alter our prefix.
        prior["task_id"] = hex_id(9999)
        for path in order_trace.MODULES[1:]:
            trace.mutation([path])
            trace.check(path)
        native = {"order_evidence": trace.value}
        self.assertTrue(self.control.observations(native)["ordered_verified_milestones"])
        trace.value["events"][0]["approval_request_id"] = hex_id(9999)
        self.assertFalse(self.control.observations(native)["ordered_verified_milestones"])
        self.assertEqual(self.control.order_validation(native)["reason"], "restart_trace_prefix_changed")
        self.control.order_start_evidence = None
        self.assertFalse(self.control.observations(native)["ordered_verified_milestones"])

    def test_safe_report_projects_only_content_free_validation_summary(self):
        trace = Trace().ordered()
        trace.value["unknown_raw_content"] = "must never be persisted"
        safe = runner._safe_worker({"order_evidence": trace.value})
        self.assertEqual(set(safe), {"order_validation"})
        self.assertEqual(set(safe["order_validation"]),
                         {"version", "ordered_verified_milestones", "reason", "event_count", "verified_modules"})
        self.assertFalse(safe["order_validation"]["ordered_verified_milestones"])
        self.assertNotIn("must never", str(safe))


if __name__ == "__main__":
    unittest.main()
