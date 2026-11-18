"""Offline control-channel tests; these do not claim any real model result."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import cases
import run_baseline as runner
import supervisor


class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="a3-plan22-supervisor-test-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def prepare(self, case_id):
        run_dir = self.root / case_id
        run_dir.mkdir()
        cases.materialize(case_id, "development", run_dir)
        return supervisor.Supervisor(run_dir, case_id, "development", cases.fixture_files(case_id, "development"),
                                     cases.intervention_files(case_id, "development"))

    def request(self, case_id, *, nonce="a" * 64):
        return {"phase": "awaiting_intervention", "intervention_request": {
            "version": 1, "request_id": nonce, "case_id": case_id, "stage": supervisor.STAGES[case_id],
            "relative_paths": sorted(cases.intervention_files(case_id, "development")),
            "at_before_first_patch_approval": case_id != "Q05",
            "before_blake3": {path: "b" * 64 if path in cases.fixture_files(case_id, "development") else None
                              for path in cases.intervention_files(case_id, "development")}}}

    def test_current_native_marker_complete_shape_for_every_intervention(self):
        # These hash-shaped values deliberately do not pretend to recompute BLAKE3.
        for case_id in ("A09", "A11", "Q05"):
            with self.subTest(case_id=case_id):
                control = self.prepare(case_id)
                marker = self.request(case_id)["intervention_request"]
                self.assertEqual(set(marker), {"version", "request_id", "case_id", "stage",
                    "relative_paths", "at_before_first_patch_approval", "before_blake3"})
                supervisor.atomic_json(control.run_dir / "intervention-request.json", marker)
                process = mock.Mock()
                process.poll.return_value = None
                control.poll(process)
                ack = runner.read_json(control.run_dir / "intervention.json")
                self.assertEqual(ack["request_id"], marker["request_id"])
                self.assertNotIn("before_blake3", ack)
                for path, body in cases.intervention_files(case_id, "development").items():
                    self.assertEqual((control.worktree / path).read_bytes(), body)
                    self.assertEqual(ack["after_sha256"][path], hashlib.sha256(body).hexdigest())
                    self.assertEqual(bytes.fromhex(ack["expected_after_hex"][path]), body)

    def test_native_before_metadata_rejects_missing_unknown_paths_hashes_and_presence(self):
        control = self.prepare("A09")
        original = (control.worktree / "subject.py").read_bytes()
        changes = ({}, {"other.py": "b" * 64}, {"subject.py": "B" * 64},
                   {"subject.py": "b" * 63}, {"subject.py": None}, {"subject.py": True})
        for invalid in changes:
            with self.subTest(before_blake3=invalid):
                request = self.request("A09")
                request["intervention_request"]["before_blake3"] = invalid
                with self.assertRaisesRegex(supervisor.SupervisorFailure, "intervention_before_metadata"):
                    control.apply(request)
        for key in ("before_blake3", "at_before_first_patch_approval"):
            request = self.request("A09")
            del request["intervention_request"][key]
            with self.assertRaisesRegex(supervisor.SupervisorFailure, "intervention_request_fields"):
                control.apply(request)
        for alteration in ({"unknown": "ignored?"}, {"at_before_first_patch_approval": False}):
            request = self.request("A09")
            request["intervention_request"].update(alteration)
            with self.assertRaisesRegex(supervisor.SupervisorFailure, "intervention_request_fields"):
                control.apply(request)
        self.assertEqual((control.worktree / "subject.py").read_bytes(), original)
        self.assertFalse((control.run_dir / "intervention.json").exists())

    def test_exact_user_edit_and_ack_are_bound_and_idempotent(self):
        control = self.prepare("A09")
        original = (control.worktree / "subject.py").read_bytes()
        control.apply(self.request("A09"))
        expected = cases.intervention_files("A09", "development")["subject.py"]
        self.assertEqual((control.worktree / "subject.py").read_bytes(), expected)
        ack = runner.read_json(control.run_dir / "intervention.json")
        self.assertEqual(ack["before_sha256"], {"subject.py": hashlib.sha256(original).hexdigest()})
        self.assertEqual(ack["after_sha256"], {"subject.py": hashlib.sha256(expected).hexdigest()})
        self.assertEqual(set(ack), {"version", "request_id", "case_id", "stage", "before_sha256", "after_sha256",
                                    "expected_before_hex", "expected_after_hex"})
        self.assertEqual(bytes.fromhex(ack["expected_before_hex"]["subject.py"]), original)
        self.assertEqual(bytes.fromhex(ack["expected_after_hex"]["subject.py"]), expected)
        self.assertNotIn("expected_before_hex", control.receipt)
        self.assertNotIn("expected_after_hex", runner.read_json(control.run_dir / "intervention-supervisor.json"))
        before = (control.worktree / "subject.py").stat().st_mtime_ns
        control.apply(self.request("A09"))
        self.assertEqual((control.worktree / "subject.py").stat().st_mtime_ns, before)
        with self.assertRaisesRegex(supervisor.SupervisorFailure, "repeated_intervention_request"):
            control.apply(self.request("A09", nonce="b" * 64))

    def test_already_changed_function_is_never_overwritten(self):
        control = self.prepare("A09")
        source = control.worktree / "subject.py"
        source.write_bytes(b"already changed by another owner\n")
        with self.assertRaisesRegex(supervisor.SupervisorFailure, "intervention_precondition_changed"):
            control.apply(self.request("A09"))
        self.assertEqual(source.read_bytes(), b"already changed by another owner\n")
        self.assertFalse((control.run_dir / "intervention.json").exists())

    def test_scope_and_identity_cannot_choose_unfrozen_paths(self):
        control = self.prepare("Q05")
        for key, value in (("relative_paths", ["../outside.py"]), ("case_id", "A09"),
                           ("request_id", "invented"), ("stage", "before_first_patch_approval")):
            request = self.request("Q05")
            request["intervention_request"][key] = value
            with self.assertRaises(supervisor.SupervisorFailure):
                control.apply(request)
        self.assertEqual((control.worktree / "config.py").read_bytes(), cases.fixture_files("Q05", "development")["config.py"])

    def test_new_caller_files_use_absence_preconditions(self):
        control = self.prepare("A11")
        control.apply(self.request("A11"))
        for path, expected in cases.intervention_files("A11", "development").items():
            self.assertEqual((control.worktree / path).read_bytes(), expected)
            self.assertIsNone(control.receipt["before_sha256"][path])

    def test_partial_effect_is_retained_without_ack_or_rollback(self):
        control = self.prepare("A11")
        real_link = os.link
        calls = []

        def fail_second(source, destination):
            calls.append(destination)
            if len(calls) == 2:
                raise OSError("fixed test failure")
            return real_link(source, destination)

        with mock.patch.object(supervisor.os, "link", fail_second):
            with self.assertRaises(OSError):
                control.apply(self.request("A11"))
        receipt = runner.read_json(control.run_dir / "intervention-supervisor.json")
        self.assertEqual(receipt["status"], "incomplete")
        self.assertEqual(len(receipt["applied_paths"]), 1)
        self.assertTrue((control.worktree / receipt["applied_paths"][0]).is_file())
        self.assertFalse((control.run_dir / "intervention.json").exists())

    def test_a08_requires_current_closed_native_evidence(self):
        control = self.prepare("A08")
        self.assertEqual(control.observations({"observations": {"observed_failed_check": True,
                                                               "corrected_after_failure": True}}), {})
        evidence = {"version": 1, "source": "production_process_inspection_and_mutation_journal",
                    "failed_checks_before_first_patch": 1, "successful_checks_after_correction": 1,
                    "applied_patch_count": 1, "check_count": 2, "current_successful_check": True}
        self.assertEqual(control.observations({"observation_evidence": evidence}),
                         {"observed_failed_check": True, "corrected_after_failure": True})
        for alteration in ({"source": "model"}, {"version": 2}, {"check_count": True}):
            self.assertEqual(control.observations({"observation_evidence": {**evidence, **alteration}}), {})
        stale = control.observations({"observation_evidence": {**evidence, "current_successful_check": False}})
        self.assertTrue(stale["observed_failed_check"])
        self.assertFalse(stale["corrected_after_failure"])

    def test_evidence_nonce_must_match_supervisor_receipt(self):
        control = self.prepare("Q05")
        control.apply(self.request("Q05"))
        evidence = {"version": 1, "source": "production_journal_and_scoped_intervention", "case_id": "Q05",
                    "intervention_request_id": "a" * 64, "initial_answer_completed": True,
                    "initial_config_blake3": "b" * 64, "post_edit_config_blake3": "c" * 64,
                    "initial_cited_config_blake3": "b" * 64, "final_cited_config_blake3": "c" * 64}
        self.assertTrue(all(control.observations({"observation_evidence": evidence}).values()))
        mismatched = control.observations({"observation_evidence": {**evidence, "intervention_request_id": "d" * 64}})
        self.assertFalse(any(mismatched.values()))

    def test_a12_requires_closed_initial_failure_proof_not_worker_flags(self):
        control = self.prepare("A12")
        raw = {"observations": {"preexisting_failures_recorded": True, "unrelated_failure_reported": True}}
        self.assertEqual(control.observations(raw), {"preexisting_failures_recorded": False})
        proof = {"version": 1, "source": "production_initial_unittest_inspection", "failed_exit": True,
                 "initial_bytes_unchanged": True, "publication_matches": True, "before_first_mutation": True,
                 "legacy_failure_header": True, "stream_truncated": False,
                 "process_id_hash64": "a" * 64, "stream_digest64": "b" * 64}
        self.assertEqual(control.observations({"legacy_failure_evidence": proof}), {"preexisting_failures_recorded": True})
        for invalid in ({"version": True}, {"version": 2}, {"source": "model"}, {"failed_exit": False},
                        {"initial_bytes_unchanged": False}, {"publication_matches": False},
                        {"before_first_mutation": False}, {"legacy_failure_header": False},
                        {"stream_truncated": True}, {"process_id_hash64": "fake"}, {"stream_digest64": "z" * 64}):
            result = control.observations({"legacy_failure_evidence": {**proof, **invalid}})
            self.assertEqual(result, {"preexisting_failures_recorded": False})

    def test_agent_report_keeps_only_hash_and_task_bound_closed_metadata(self):
        native = {"agent_answer": "secret-answer-sentinel", "agent_answer_truncated": False,
                  "agent_answer_metadata": {"entry_kind": "FinalReport", "entry_sequence": 7,
                                            "task_matches": True, "raw": "secret-metadata"}}
        self.assertEqual(supervisor.bound_agent_answer(native), "secret-answer-sentinel")
        safe = runner._safe_worker(native)
        self.assertEqual(safe["agent_answer_metadata"], {"entry_kind": "FinalReport", "entry_sequence": 7, "task_matches": True})
        self.assertEqual(safe["agent_answer_bytes"], len(native["agent_answer"]))
        self.assertNotIn("secret", json.dumps(safe))
        for metadata in ({"entry_kind": "UserMessage"}, {"entry_sequence": 0}, {"entry_sequence": True}, {"task_matches": False}):
            bad = {**native, "agent_answer_metadata": {**native["agent_answer_metadata"], **metadata}}
            self.assertIsNone(supervisor.bound_agent_answer(bad))
        self.assertIsNone(supervisor.bound_agent_answer({**native, "agent_answer_truncated": True}))

    def test_scope_hash_and_journal_summaries_exclude_raw_fields(self):
        native = {"denied_scope": {"kind": "process", "execution_mode": "Shell", "root_cwd": True,
                                  "argv_matches": False, "arguments": ["secret-argument"]},
                  "metrics": {"records": [{"stage": "A^3 AgentActionV6", "message_blake3": "a" * 64,
                                             "schema_blake3": None, "raw_prompt": "secret-prompt"}]},
                  "report_projection_truncated": True, "projection_records_omitted": {"journal": 1, "metrics": 2, "raw": "secret"},
                  "journal": {"truncated": False, "records": [
                      {"sequence": 1, "kind": "run_started", "code": "UserRequest", "raw": "secret-source"},
                      {"sequence": 2, "kind": "model_interaction", "code": "InvalidModelOutput",
                       "outcome": "Failed", "action": None, "repair": "One"},
                      {"sequence": 3, "kind": "diagnostic", "code": "secret-arbitrary-code"}]}}
        safe = runner._safe_worker(native)
        self.assertEqual(safe["journal"]["invalid_record_count"], 1)
        self.assertEqual(len(safe["journal"]["records"]), 2)
        self.assertEqual(safe["metrics"]["records"][0]["message_blake3"], "a" * 64)
        self.assertEqual(safe["metrics"]["records"][0]["stage"], "A^3 AgentActionV6")
        self.assertEqual(safe["projection_records_omitted"], {"journal": 1, "metrics": 2})
        self.assertNotIn("secret", json.dumps(safe))

    def test_restart_rejects_changed_request_and_unstopped_owner(self):
        control = self.prepare("A10")
        process = {"process_id": 1234, "exit_code": 0, "timed_out": False, "interrupted": False}
        control.record_process("start", process, {})
        checkpoint = {"version": 1, "ready": True, "request_blake3": "a" * 64, "source_pid": 1234,
                      "elapsed_task_millis": 100, "remaining_task_millis": 1799900}
        runner.atomic_json(control.run_dir / "checkpoint.json", checkpoint)
        native = {"phase": "restart_ready", "terminal": False, "worker_succeeded": True,
                  "cleanup": {"conversation_stopped": True, "agent_stopped": False, "scheduler_joined": True}}
        request_hash = runner.file_sha256(control.run_dir / "request.json")
        with self.assertRaisesRegex(supervisor.SupervisorFailure, "restart_owner_not_stopped"):
            control.prepare_restart(native, process, request_file_sha256=request_hash, task_budget_seconds=1800)
        native["cleanup"]["agent_stopped"] = True
        (control.run_dir / "request.json").write_text("{}")
        with self.assertRaisesRegex(supervisor.SupervisorFailure, "restart_request_changed"):
            control.prepare_restart(native, process, request_file_sha256=request_hash, task_budget_seconds=1800)
        self.assertFalse((control.run_dir / "worker-start.json").exists())

    def test_request_v2_runtime_effects_remain_exact_and_case_bound(self):
        basic = {"mode": "agent", "objective": "A public fixture.", "allowed_write_paths": [], "allowed_processes": []}
        valid = {**basic, "allowed_network_requests": [{"method": "GET", "url": "http://127.0.0.1:18765/"}]}
        self.assertEqual(runner.validate_case(valid, "A04", "development")["allowed_network_requests"], valid["allowed_network_requests"])
        for url in ("http://localhost:18765/", "http://127.0.0.1:18766/", "http://127.0.0.1:18765/?x=1"):
            with self.assertRaises(runner.RunnerFailure):
                runner.validate_case({**valid, "allowed_network_requests": [{"method": "GET", "url": url}]}, "A04", "development")
        with self.assertRaises(runner.RunnerFailure):
            runner.validate_case(valid, "A01", "development")
        effect = {**basic, "process_effect_paths": [".a3-probe-notes.json"]}
        self.assertEqual(runner.validate_case(effect, "A05", "development")["process_effect_paths"], [".a3-probe-notes.json"])
        with self.assertRaises(runner.RunnerFailure):
            runner.validate_case(effect, "A01", "development")
        with self.assertRaises(runner.RunnerFailure):
            runner.validate_case({**effect, "process_effect_paths": ["other.json"]}, "A05", "development")


class ProcessProtocolTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="a3-plan22-protocol-test-")
        self.root = Path(self.temporary.name)
        self.run_dir = self.root / "attempt"
        self.run_dir.mkdir()
        self.worker = self.root / "fake_worker.py"

    def tearDown(self):
        self.temporary.cleanup()

    def attempt(self, case_id, source, evaluate=None):
        self.worker.write_text(source, encoding="utf-8")
        return runner.run_attempt(repository=self.root, run_dir=self.run_dir, model="qwen38-4k:latest",
            case_id=case_id, variant="development", phase="pilot", worker=Path(sys.executable),
            materialize=cases.materialize, evaluate=evaluate or (lambda *args: {"passed": True, "manual_review_required": False}),
            metadata_provider=lambda _: {"digest": "a" * 64}, runtime_provider=lambda _: {},
            freeze_provider=lambda: {"models": {"qwen38-4k:latest": {"digest": "a" * 64,
                                                                    "context_limit": 4096, "output_limit": 1024}}},
            worker_command=[sys.executable, str(self.worker)], gpu_lock_path=self.root / "gpu.lock", worker_timeout=5)

    PRELUDE = """import json,os,pathlib,time,sys
root=pathlib.Path(os.environ['A3_PLAN22_RUN_DIR'])
request=json.loads((root/'request.json').read_text())
def report(value):
 temp=root/'worker-write.tmp'
 temp.write_text(json.dumps({'version':1,'case_id':request['case_id'],'variant':request['variant'],**value}))
 os.replace(temp,root/'worker.json')
def control(value):
 report({'phase':'awaiting_intervention','terminal':False,'intervention_request':value})
 temp=root/'intervention-request.next'
 temp.write_text(json.dumps(value))
 os.replace(temp,root/'intervention-request.json')
marker=root/'starts.txt'
marker.write_text(marker.read_text()+'1' if marker.exists() else '1')
"""

    def test_native_process_waits_for_exact_fixture_ack(self):
        code = self.PRELUDE + """control({
 'version':1,'request_id':'a'*64,'case_id':'A09','stage':'before_first_patch_approval','relative_paths':['subject.py'],
 'at_before_first_patch_approval':True,'before_blake3':{'subject.py':'b'*64}})
while not (root/'intervention.json').exists():time.sleep(.01)
ack=json.loads((root/'intervention.json').read_text())
assert ack['request_id']=='a'*64
report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True})
"""
        result = self.attempt("A09", code)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["worker_starts"], 1)
        self.assertEqual((self.run_dir / "starts.txt").read_text(), "1")
        self.assertEqual(result["intervention"]["case_id"], "A09")

    def test_progress_observer_allows_atomic_native_report_replacements(self):
        code = self.PRELUDE + """for i in range(150):
 report({'phase':'running','terminal':False,'padding':'x'*1024,'progress':i})
 time.sleep(.001)
control({
 'version':1,'request_id':'a'*64,'case_id':'Q05','stage':'after_initial_answer','relative_paths':['config.py'],
 'at_before_first_patch_approval':False,'before_blake3':{'config.py':'b'*64}})
while not (root/'intervention.json').exists():time.sleep(.01)
report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'ask_answer':'A bounded public answer.'})
"""
        result = self.attempt("Q05", code)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["native_process"]["exit_code"], 0)
        self.assertEqual(result["intervention"]["case_id"], "Q05")

    def test_restart_is_explicit_once_and_uses_same_request_and_run(self):
        code = self.PRELUDE + """stage=os.environ.get('A3_PLAN22_STAGE')
if stage=='start':
 (root/'checkpoint.json').write_text(json.dumps({'version':1,'ready':True,'request_blake3':'a'*64,'source_pid':os.getpid(),
  'elapsed_task_millis':100,'remaining_task_millis':1799900}))
 report({'phase':'restart_ready','terminal':False,'worker_succeeded':True,'cleanup':{
  'conversation_stopped':True,'agent_stopped':True,'scheduler_joined':True},'metrics':{'calls':1}})
else:
 assert stage=='resume'
 assert (root/'worker-start.json').exists()
 report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'metrics':{'calls':3}})
"""
        result = self.attempt("A10", code)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["worker_starts"], 2)
        self.assertEqual(result["native_retry_count"], 0)
        self.assertEqual(result["deliberate_restarts"], 1)
        self.assertEqual((self.run_dir / "starts.txt").read_text(), "11")
        self.assertEqual([stage["stage"] for stage in result["native_processes"]], ["start", "resume"])
        self.assertEqual(result["aggregate_native_metrics"]["calls"], 3)

    def test_restored_attempt_counters_are_not_summed_or_reset_across_restart(self):
        stages = [{"report": {"metrics": {"calls": 1, "provider_prompt_tokens": None}}},
                  {"report": {"metrics": {"calls": 3, "provider_prompt_tokens": 99}}}]
        aggregate = runner.aggregate_metrics(stages)
        self.assertEqual(aggregate["calls"], 3)
        self.assertIsNone(aggregate["provider_prompt_tokens"])
        stages[1]["report"]["metrics"]["calls"] = 0
        with self.assertRaisesRegex(runner.RunnerFailure, "native_metrics_regressed"):
            runner.aggregate_metrics(stages)

    def test_unreached_restart_checkpoint_is_not_retried(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':False,'session_state':'AwaitingUser'})\n"
        result = self.attempt("A10", code)
        self.assertEqual(result["status"], "product_halted")
        self.assertEqual(result["worker_starts"], 1)
        self.assertFalse((self.run_dir / "worker-start.json").exists())

    def test_awaiting_user_with_zero_calls_is_not_manual_review_pending(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':False,'session_state':'AwaitingUser','ask_answer':'','metrics':{'calls':0}})\n"
        result = self.attempt("Q01", code, evaluate=lambda *args: {"passed": False, "manual_review_required": True})
        self.assertEqual(result["status"], "product_halted")
        self.assertEqual(result["independent_status"], "manual_review_pending")

    def test_settled_product_failure_is_distinct_from_native_and_review_failure(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':False,'session_state':'Failed','run_state':'Execute','product_failed':True,'executor_failure':'AnchorsChanged','product_stop_reason':'settled_product_owner'})\n"
        result = self.attempt("A09", code, evaluate=lambda *args: {"passed":False,"manual_review_required":True})
        self.assertEqual(result["status"], "product_failed")
        self.assertEqual(result["native_process"]["exit_code"], 0)
        self.assertEqual(result["worker"]["executor_failure"], "AnchorsChanged")
        self.assertEqual(result["worker_starts"],1)
        self.assertFalse(result["independently_correct"])

    def test_completed_substantive_ask_still_requires_manual_review(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'session_state':'Completed','ask_answer':'A public bounded fixture answer.','metrics':{'calls':1}})\n"
        result = self.attempt("Q01", code, evaluate=lambda *args: {"passed": False, "manual_review_required": True})
        self.assertEqual(result["status"], "manual_review_pending")

    def test_environment_block_is_not_a_model_failure_or_a_proven_false_done(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'session_state':'Completed'})\n"
        result = self.attempt("A04",code,evaluate=lambda *args:{"passed":False,"manual_review_required":False,
            "environment_failure":"default_port_unavailable","checks":[{"id":"oracle_environment",
                "passed":False,"detail":"default_port_unavailable","environment_blocked":True}]})
        self.assertEqual(result["status"],"oracle_environment_failure")
        self.assertFalse(result["false_done"])
        self.assertTrue(result["false_done_environment_pending"])
        self.assertFalse(result["independently_correct"])

    def test_environment_block_does_not_hide_a_known_wrong_artifact(self):
        code = self.PRELUDE + "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'session_state':'Completed'})\n"
        result = self.attempt("A04",code,evaluate=lambda *args:{"passed":False,"manual_review_required":False,
            "environment_failure":"default_port_unavailable","checks":[{"id":"http:GET:/","passed":False},
                {"id":"oracle_environment","passed":False,"detail":"default_port_unavailable","environment_blocked":True}]})
        self.assertEqual(result["status"],"oracle_environment_failure")
        self.assertTrue(result["false_done"])
        self.assertFalse(result["false_done_environment_pending"])

    A12_REPORT = "report({'phase':'terminal','terminal':True,'worker_succeeded':True,'formal_completed':True,'run_state':'Done','session_state':'Completed','agent_answer':'Scoped target check passed; the unrelated legacy check remains red.','agent_answer_truncated':False,'agent_answer_metadata':{'entry_kind':'FinalReport','entry_sequence':7,'task_matches':True}})\n"

    def test_a12_known_wrong_code_is_false_done_even_when_review_is_missing(self):
        seen = []

        def evaluate(case_id, variant, worktree, answer, observations):
            seen.append(answer)
            return {"passed": False, "manual_review_required": True, "checks": [
                {"id": "target_behavior", "passed": False},
                {"id": "operator_semantic_review_record", "passed": False}]}

        result = self.attempt("A12", self.PRELUDE + self.A12_REPORT, evaluate=evaluate)
        self.assertTrue(result["false_done"])
        self.assertFalse(result["false_done_review_pending"])
        self.assertEqual(result["status"], "incorrect")
        self.assertTrue(seen[0].startswith("Scoped target"))

    def test_a12_only_missing_review_is_pending_and_never_a_gate_pass(self):
        result = self.attempt("A12", self.PRELUDE + self.A12_REPORT, evaluate=lambda *args: {
            "passed": False, "manual_review_required": True, "checks": [
                {"id": "target_behavior", "passed": True},
                {"id": "operator_semantic_review_record", "passed": False}]})
        self.assertFalse(result["false_done"])
        self.assertTrue(result["false_done_review_pending"])
        self.assertEqual(result["status"], "manual_review_pending")
        self.assertFalse(result["independently_correct"])


if __name__ == "__main__":
    unittest.main()
